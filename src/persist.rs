//! Persistence format and load policy: one versioned JSON envelope, `{"version":1,"doc":{..}}`,
//! stored under [`KEY`]. This is the pure half. It owns the format and the corrupt or newer-blob
//! policy and runs natively under `cargo test`. The domain types are the stored shape, so a schema
//! change bumps [`SCHEMA_VERSION`] and adds a `decode` arm that reads the old shape from a frozen
//! copy of the old structs. `tests/fixtures/v1.json` is that guard: it must never be edited.

use serde::Serialize;
use serde_json::Value;

use crate::domain::Doc;

pub const KEY: &str = "tivan.state";
/// Where an unreadable blob is copied before the app starts over.
pub const BACKUP_KEY: &str = "tivan.state.backup";
pub const SCHEMA_VERSION: u64 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not JSON, not an envelope, or a document that does not fit its version's shape.
    Malformed(String),
    /// Written by a newer build. Kept apart because the blob is probably fine and only this
    /// build is old.
    NewerVersion(u64),
}

#[derive(Serialize)]
struct Envelope<'a> {
    version: u64,
    doc: &'a Doc,
}

pub fn encode(doc: &Doc) -> String {
    let envelope = Envelope {
        version: SCHEMA_VERSION,
        doc,
    };
    serde_json::to_string(&envelope).expect("a Doc always serializes")
}

/// Parses a stored blob. A version 1 document is repaired with [`Doc::reconcile`] rather than
/// rejected, so dangling references from a hand edit or an older bug do not cost a teacher their
/// classes.
pub fn decode(raw: &str) -> Result<Doc, DecodeError> {
    let mut envelope: Value =
        serde_json::from_str(raw).map_err(|e| DecodeError::Malformed(e.to_string()))?;
    let version = envelope
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| DecodeError::Malformed("missing numeric version".to_owned()))?;
    match version {
        SCHEMA_VERSION => {
            let doc = envelope
                .get_mut("doc")
                .ok_or_else(|| DecodeError::Malformed("missing doc".to_owned()))?
                .take();
            let mut doc: Doc =
                serde_json::from_value(doc).map_err(|e| DecodeError::Malformed(e.to_string()))?;
            doc.reconcile();
            Ok(doc)
        }
        newer if newer > SCHEMA_VERSION => Err(DecodeError::NewerVersion(newer)),
        other => Err(DecodeError::Malformed(format!(
            "unsupported version {other}"
        ))),
    }
}

/// What to do with whatever was in storage. Pure, so the policy is testable.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadPlan {
    /// Nothing stored. Seed silently.
    FirstRun,
    Use(Doc),
    /// Keep the unreadable blob under [`BACKUP_KEY`], start from the seed, tell the teacher.
    /// Neither a corrupt nor a newer-version blob is ever silently overwritten.
    StartFresh {
        stash: String,
        reason: DecodeError,
    },
}

pub fn plan_load(stored: Option<&str>) -> LoadPlan {
    let Some(raw) = stored else {
        return LoadPlan::FirstRun;
    };
    match decode(raw) {
        Ok(doc) => LoadPlan::Use(doc),
        Err(reason) => LoadPlan::StartFresh {
            stash: raw.to_owned(),
            reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GroupCount, Label, SplitMix64, seed_doc};

    const FIXTURE: &str = include_str!("../tests/fixtures/v1.json");

    fn fixture_value() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// A doc where counters have moved past the live ids, with a grouping, a manual rename,
    /// and a partly used oracle round.
    fn worked_doc() -> Doc {
        let mut doc = seed_doc();
        let mut rng = SplitMix64::seeded(11);
        let doomed = doc.create_class(label("Doomed"), vec![label("Zed")]);
        doc.delete_class(doomed);
        doc.select(doc.classes()[1].id());

        let class = doc.selected_mut().unwrap();
        let first = class.students()[0].id;
        class.remove_student(first);
        class.add_student(label("Newcomer"));
        let groups = GroupCount::new(4, class.students().len()).unwrap();
        class.regroup(groups, &mut rng);
        let idx = class.group_views()[2].idx;
        class.rename_group(idx, label("Table Three")).unwrap();
        class.draw(&mut rng).unwrap();
        class.draw(&mut rng).unwrap();
        doc
    }

    #[test]
    fn the_seed_survives_an_encode_decode_round_trip() {
        let doc = seed_doc();
        assert_eq!(decode(&encode(&doc)).unwrap(), doc);
    }

    #[test]
    fn groupings_oracle_rounds_and_counters_survive_a_round_trip() {
        let doc = worked_doc();
        let class = doc.selected().unwrap();
        assert!(class.grouping().is_some());
        assert!(class.oracle_remaining() < class.students().len());

        let decoded = decode(&encode(&doc)).unwrap();

        assert_eq!(decoded, doc);
        decoded.assert_invariants();
    }

    #[test]
    fn encode_wraps_the_doc_in_a_versioned_envelope() {
        let value: Value = serde_json::from_str(&encode(&seed_doc())).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["doc"]["classes"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn the_frozen_v1_fixture_still_decodes_to_the_same_classes() {
        let doc = decode(FIXTURE).unwrap();

        assert_eq!(doc.classes().len(), 2);
        assert_eq!(doc.selected().unwrap().name().as_str(), "Second Class");

        let class = &doc.classes()[0];
        assert_eq!(class.name().as_str(), "Fixture Class");
        let views = class.group_views();
        let described: Vec<(&str, Vec<&str>)> = views
            .iter()
            .map(|g| {
                let members = g.members.iter().map(|s| s.name.as_str()).collect();
                (g.name.as_str(), members)
            })
            .collect();
        assert_eq!(
            described,
            [
                ("Brave Bears", vec!["Ava", "Chloe"]),
                ("Sunny Seals", vec!["Ben", "Diego"]),
            ]
        );
        assert_eq!(class.oracle_remaining(), 2);
        assert!(doc.classes()[1].grouping().is_none());
    }

    #[test]
    fn the_frozen_v1_fixture_re_encodes_to_itself() {
        let doc = decode(FIXTURE).unwrap();
        let again: Value = serde_json::from_str(&encode(&doc)).unwrap();
        assert_eq!(
            again,
            fixture_value(),
            "stored shape drifted: bump SCHEMA_VERSION"
        );
    }

    #[test]
    fn counters_from_the_fixture_keep_ids_that_were_handed_out_before_unique() {
        let mut doc = decode(FIXTURE).unwrap();
        let first_class = doc.classes()[0].id();
        doc.select(first_class);

        let student = doc.selected_mut().unwrap().add_student(label("New"));
        let class = doc.create_class(label("Third"), vec![]);

        assert_eq!(student.get(), 7, "students 5 and 6 were removed earlier");
        assert_eq!(class.get(), 5, "classes 3 and 4 were deleted earlier");
    }

    #[test]
    fn no_stored_blob_means_first_run() {
        assert_eq!(plan_load(None), LoadPlan::FirstRun);
    }

    #[test]
    fn a_valid_blob_is_used_as_is() {
        let doc = worked_doc();
        assert_eq!(plan_load(Some(&encode(&doc))), LoadPlan::Use(doc));
    }

    #[test]
    fn unreadable_blobs_are_stashed_byte_for_byte_and_never_dropped() {
        let doc_with = |tweak: fn(&mut Value)| {
            let mut value = fixture_value();
            tweak(&mut value);
            value.to_string()
        };
        let blobs = [
            "{\"version\":1,\"doc\":".to_owned(),
            String::new(),
            "null".to_owned(),
            "[]".to_owned(),
            "\"hello\"".to_owned(),
            "{}".to_owned(),
            "{\"version\":\"1\",\"doc\":{}}".to_owned(),
            "{\"version\":1}".to_owned(),
            "{\"version\":1,\"doc\":[]}".to_owned(),
            "{\"version\":0,\"doc\":{}}".to_owned(),
            doc_with(|v| v["doc"]["classes"][0]["name"] = "   ".into()),
            doc_with(|v| v["doc"]["classes"][0]["students"][0]["id"] = "zero".into()),
            doc_with(|v| v["doc"]["classes"][0]["grouping"]["of"] = serde_json::json!({"x": 0})),
            doc_with(|v| {
                v["doc"]
                    .as_object_mut()
                    .unwrap()
                    .remove("next_class")
                    .map(drop)
                    .unwrap()
            }),
        ];
        for raw in blobs {
            match plan_load(Some(&raw)) {
                LoadPlan::StartFresh {
                    stash,
                    reason: DecodeError::Malformed(_),
                } => assert_eq!(stash, raw),
                other => panic!("{raw:?} should be unreadable, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_newer_version_is_stashed_untouched_instead_of_repaired() {
        let raw = r#"{"version":2,"somethingNew":{"classes":"different"}}"#;
        assert_eq!(
            plan_load(Some(raw)),
            LoadPlan::StartFresh {
                stash: raw.to_owned(),
                reason: DecodeError::NewerVersion(2),
            }
        );
        assert_eq!(
            decode(r#"{"version":99}"#),
            Err(DecodeError::NewerVersion(99))
        );
    }

    #[test]
    fn dangling_references_are_repaired_not_rejected() {
        let mut value = fixture_value();
        let class = &mut value["doc"]["classes"][0];
        class["grouping"]["of"] = serde_json::json!({"0": 0, "1": 1, "4": 7, "77": 0});
        class["oracle"]["drawn"] = serde_json::json!([99, 1]);
        class["next_student"] = 0.into();
        value["doc"]["selected"] = 42.into();
        value["doc"]["next_class"] = 0.into();

        let LoadPlan::Use(mut doc) = plan_load(Some(&value.to_string())) else {
            panic!("a repairable blob must load");
        };

        doc.assert_invariants();
        assert_eq!(doc.selected().unwrap().name().as_str(), "Fixture Class");
        let class = doc.selected().unwrap();
        let grouping = class.grouping().unwrap();
        assert!(
            class
                .students()
                .iter()
                .all(|s| grouping.group_of(s.id).is_some())
        );
        assert_eq!(class.oracle_remaining(), 3, "only student 1 had gone");

        let fresh_student = doc.selected_mut().unwrap().add_student(label("New"));
        assert_eq!(fresh_student.get(), 5);
        assert_eq!(doc.create_class(label("Later"), vec![]).get(), 3);
    }

    #[test]
    fn an_empty_doc_round_trips_with_nothing_selected() {
        let doc = Doc::default();
        let decoded = decode(&encode(&doc)).unwrap();
        assert_eq!(decoded, doc);
        assert!(decoded.selected().is_none());
    }
}
