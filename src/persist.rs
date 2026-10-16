//! Persistence format and load policy: one versioned JSON envelope, `{"version":5,"doc":{..}}`,
//! stored under [`KEY`]. This is the pure half. It owns the format and the corrupt or newer-blob
//! policy and runs natively under `cargo test`. The domain types are the stored shape, so a schema
//! change bumps [`SCHEMA_VERSION`] and adds a `decode` arm for the old shape. Version 1 differs
//! only by lacking each student's avatar, so its arm upgrades the JSON value in place and reads it
//! as the current shape instead of keeping a frozen copy of the old structs. Version 2 differs
//! only by lacking a grouping's optional `centers`, which an absent key already reads as none, so
//! it shares the current arm. Version 3 differs only by lacking a grouping's optional group round
//! (`oracle`), which an absent key already reads as empty, so it shares the arm too. Version 4
//! differs only by lacking the document's `settings`, which an absent key reads as the defaults,
//! so it shares the arm as well. The version still moves with each additive field, so an older
//! build refuses a newer blob instead of dropping the field on its next save. The fixtures under
//! `tests/fixtures/` are the guard: they must never be edited.

use serde::Serialize;
use serde_json::Value;

use crate::domain::{Doc, Rng, avatar};

pub const KEY: &str = "tivan.state";
/// Where an unreadable blob is copied before the app starts over.
pub const BACKUP_KEY: &str = "tivan.state.backup";
pub const SCHEMA_VERSION: u64 = 5;

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

/// Parses a stored blob. A document is repaired with [`Doc::reconcile`] rather than rejected, so
/// dangling references from a hand edit or an older bug do not cost a teacher their classes.
/// A version 1 document gets an avatar for each student, chosen with `rng`.
pub fn decode(raw: &str, rng: &mut impl Rng) -> Result<Doc, DecodeError> {
    let mut envelope: Value =
        serde_json::from_str(raw).map_err(|e| DecodeError::Malformed(e.to_string()))?;
    let version = envelope
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| DecodeError::Malformed("missing numeric version".to_owned()))?;
    match version {
        1 | 2 | 3 | 4 | SCHEMA_VERSION => {
            let doc = envelope
                .get_mut("doc")
                .ok_or_else(|| DecodeError::Malformed("missing doc".to_owned()))?;
            if version == 1 {
                add_avatars(doc, rng);
            }
            let mut doc: Doc = serde_json::from_value(doc.take())
                .map_err(|e| DecodeError::Malformed(e.to_string()))?;
            doc.reconcile();
            Ok(doc)
        }
        newer if newer > SCHEMA_VERSION => Err(DecodeError::NewerVersion(newer)),
        other => Err(DecodeError::Malformed(format!(
            "unsupported version {other}"
        ))),
    }
}

/// Gives every student of a version 1 document an avatar, distinct within their class until the
/// class outgrows the set. Anything not shaped like version 1 is left alone for deserializing to
/// reject.
fn add_avatars(doc: &mut Value, rng: &mut impl Rng) {
    let Some(classes) = doc.get_mut("classes").and_then(Value::as_array_mut) else {
        return;
    };
    for class in classes {
        let Some(students) = class.get_mut("students").and_then(Value::as_array_mut) else {
            continue;
        };
        let mut taken = Vec::new();
        for student in students.iter_mut().filter_map(Value::as_object_mut) {
            let avatar = avatar::pick(taken.iter().copied(), rng);
            taken.push(avatar);
            student.insert("avatar".to_owned(), avatar.name().into());
        }
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

pub fn plan_load(stored: Option<&str>, rng: &mut impl Rng) -> LoadPlan {
    let Some(raw) = stored else {
        return LoadPlan::FirstRun;
    };
    match decode(raw, rng) {
        Ok(doc) => LoadPlan::Use(doc),
        Err(reason) => LoadPlan::StartFresh {
            stash: raw.to_owned(),
            reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::*;
    use crate::domain::{
        Avatar, Draw, GroupCount, Label, Pick, Skin, SkinChoice, SplitMix64, seed_doc,
    };

    const V1: &str = include_str!("../tests/fixtures/v1.json");
    const V2: &str = include_str!("../tests/fixtures/v2.json");
    const V3: &str = include_str!("../tests/fixtures/v3.json");
    const V4: &str = include_str!("../tests/fixtures/v4.json");
    const V5: &str = include_str!("../tests/fixtures/v5.json");

    fn rng() -> SplitMix64 {
        SplitMix64::seeded(1)
    }

    fn parse(raw: &str) -> Value {
        serde_json::from_str(raw).unwrap()
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// A doc where counters have moved past the live ids, with a grouping, a manual rename,
    /// centers partway round their cycle with one renamed, and partly used student and group
    /// oracle rounds.
    fn worked_doc() -> Doc {
        let mut doc = seed_doc();
        let mut rng = SplitMix64::seeded(11);
        let doomed = doc.create_class(label("Doomed"), vec![label("Zed")], &mut rng);
        doc.delete_class(doomed);
        doc.select(doc.classes()[1].id());

        let class = doc.selected_mut().unwrap();
        let first = class.students()[0].id;
        class.remove_student(first);
        class.add_student(label("Newcomer"), &mut rng);
        let groups = GroupCount::new(4, class.students().len()).unwrap();
        class.regroup(groups, &mut rng);
        let idx = class.group_views()[2].idx;
        class.rename_group(idx, label("Table Three")).unwrap();
        class.add_centers();
        class.rotate_centers();
        class.rotate_centers();
        let center = class.center_views().unwrap()[1].idx;
        class.rename_center(center, label("Reading Nook")).unwrap();
        class.draw(Draw::Student, &mut rng).unwrap();
        class.draw(Draw::Student, &mut rng).unwrap();
        class.draw(Draw::Group, &mut rng).unwrap();
        doc
    }

    /// What every fixture says apart from avatars, which version 1 never stored, centers, which
    /// version 3 and later store, and the group round, which version 4 and later store.
    fn assert_fixture_classes(doc: &Doc) {
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
        assert_eq!(class.oracle_remaining(Draw::Student), 2);
        assert!(doc.classes()[1].grouping().is_none());
    }

    /// An older fixture as the current build writes it back: the new version number and the
    /// settings an absent key reads as.
    fn written_by_this_build(raw: &str) -> Value {
        let mut value = parse(raw);
        value["version"] = SCHEMA_VERSION.into();
        value["doc"]["settings"] = json!({"skin": "auto"});
        value
    }

    fn avatars_by_class(doc: &Doc) -> Vec<Vec<Avatar>> {
        doc.classes()
            .iter()
            .map(|class| class.students().iter().map(|s| s.avatar).collect())
            .collect()
    }

    #[test]
    fn the_seed_survives_an_encode_decode_round_trip() {
        let doc = seed_doc();
        assert_eq!(decode(&encode(&doc), &mut rng()).unwrap(), doc);
    }

    #[test]
    fn groupings_centers_oracle_rounds_and_counters_survive_a_round_trip() {
        let doc = worked_doc();
        let class = doc.selected().unwrap();
        assert!(class.grouping().is_some());
        assert_eq!(class.grouping().unwrap().centers().unwrap().round(), 3);
        assert!(class.oracle_remaining(Draw::Student) < class.students().len());
        assert!(class.oracle_remaining(Draw::Group) < class.grouping().unwrap().len());

        let decoded = decode(&encode(&doc), &mut rng()).unwrap();

        assert_eq!(decoded, doc);
        decoded.assert_invariants();
    }

    #[test]
    fn encode_wraps_the_doc_in_a_versioned_envelope() {
        let doc = seed_doc();
        let value = parse(&encode(&doc));
        assert_eq!(value["version"], 5);
        assert_eq!(value["doc"]["classes"].as_array().unwrap().len(), 3);
        let first = &doc.classes()[0].students()[0];
        assert_eq!(
            value["doc"]["classes"][0]["students"][0]["avatar"],
            first.avatar.name()
        );
    }

    #[test]
    fn the_frozen_v1_fixture_is_upgraded_to_the_same_classes() {
        assert_fixture_classes(&decode(V1, &mut rng()).unwrap());
    }

    #[test]
    fn upgrading_v1_gives_each_student_an_avatar_distinct_within_their_class() {
        let mut value = parse(V1);
        value["doc"]["classes"][1]["students"] = (0..18)
            .map(|id| json!({"id": id, "name": format!("Student {id}")}))
            .collect();

        for seed in 0..10 {
            let doc = decode(&value.to_string(), &mut SplitMix64::seeded(seed)).unwrap();
            for class in doc.classes() {
                let distinct: HashSet<Avatar> = class.students().iter().map(|s| s.avatar).collect();
                assert_eq!(
                    distinct.len(),
                    class.students().len(),
                    "seed {seed}, {}",
                    class.name()
                );
            }
        }
    }

    #[test]
    fn the_frozen_v2_fixture_decodes_to_its_stored_avatars_whatever_the_rng() {
        for seed in [1, 2] {
            let doc = decode(V2, &mut SplitMix64::seeded(seed)).unwrap();

            assert_fixture_classes(&doc);
            assert_eq!(
                avatars_by_class(&doc),
                [
                    vec![Avatar::Bear, Avatar::Panda, Avatar::Cat, Avatar::Fox],
                    vec![Avatar::Bunny],
                ]
            );
        }
    }

    #[test]
    fn the_frozen_v2_fixture_re_encodes_with_only_the_version_and_settings_added() {
        let doc = decode(V2, &mut rng()).unwrap();
        assert_eq!(
            parse(&encode(&doc)),
            written_by_this_build(V2),
            "stored shape drifted: bump SCHEMA_VERSION"
        );
    }

    #[test]
    fn the_frozen_v1_and_v2_fixtures_have_no_centers() {
        for raw in [V1, V2] {
            let doc = decode(raw, &mut rng()).unwrap();
            assert!(doc.classes()[0].grouping().unwrap().centers().is_none());
            assert!(doc.classes()[0].center_views().is_none());
        }
    }

    #[test]
    fn the_frozen_v3_fixture_decodes_with_its_centers_and_turn() {
        let doc = decode(V3, &mut rng()).unwrap();

        assert_fixture_classes(&doc);
        let class = &doc.classes()[0];
        assert_eq!(class.grouping().unwrap().centers().unwrap().round(), 2);
        let names = class.grouping().unwrap().names();
        let standing: Vec<(&str, &str)> = class
            .center_views()
            .unwrap()
            .iter()
            .map(|center| (center.name.as_str(), names[center.group.get()].as_str()))
            .collect();
        assert_eq!(
            standing,
            [("Reading Nook", "Sunny Seals"), ("Center 2", "Brave Bears")]
        );
        doc.assert_invariants();
    }

    #[test]
    fn the_frozen_v3_fixture_re_encodes_with_only_the_version_and_settings_added() {
        let doc = decode(V3, &mut rng()).unwrap();
        assert_eq!(
            parse(&encode(&doc)),
            written_by_this_build(V3),
            "stored shape drifted: bump SCHEMA_VERSION"
        );
    }

    #[test]
    fn the_frozen_v1_to_v3_fixtures_start_with_every_group_waiting() {
        for raw in [V1, V2, V3] {
            let doc = decode(raw, &mut rng()).unwrap();
            assert_eq!(doc.classes()[0].oracle_remaining(Draw::Group), 2);
        }
    }

    #[test]
    fn the_frozen_v4_fixture_decodes_with_its_group_round() {
        let mut doc = decode(V4, &mut rng()).unwrap();

        assert_fixture_classes(&doc);
        doc.assert_invariants();
        assert_eq!(doc.classes()[0].oracle_remaining(Draw::Group), 1);
        let first = doc.classes()[0].id();
        doc.select(first);
        let pick = doc.selected_mut().unwrap().draw(Draw::Group, &mut rng());
        let Some(Pick::Group { name, members, .. }) = pick else {
            panic!("a grouped class picks a group");
        };
        assert_eq!(name.as_str(), "Brave Bears", "group 1 had already gone");
        let members: Vec<&str> = members.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(members, ["Ava", "Chloe"]);
    }

    #[test]
    fn the_frozen_v4_fixture_re_encodes_with_only_the_version_and_settings_added() {
        let doc = decode(V4, &mut rng()).unwrap();
        assert_eq!(
            parse(&encode(&doc)),
            written_by_this_build(V4),
            "stored shape drifted: bump SCHEMA_VERSION"
        );
    }

    #[test]
    fn the_frozen_v1_to_v4_fixtures_choose_skins_automatically() {
        for raw in [V1, V2, V3, V4] {
            let doc = decode(raw, &mut rng()).unwrap();
            assert_eq!(doc.settings().skin(), SkinChoice::Auto);
        }
    }

    #[test]
    fn the_frozen_v5_fixture_decodes_with_its_stored_skin() {
        let doc = decode(V5, &mut rng()).unwrap();

        assert_fixture_classes(&doc);
        doc.assert_invariants();
        assert_eq!(doc.settings().skin(), SkinChoice::Skin(Skin::Christmas));
    }

    #[test]
    fn the_frozen_v5_fixture_re_encodes_to_itself() {
        let doc = decode(V5, &mut rng()).unwrap();
        assert_eq!(
            parse(&encode(&doc)),
            parse(V5),
            "stored shape drifted: bump SCHEMA_VERSION"
        );
    }

    #[test]
    fn every_skin_choice_survives_a_round_trip() {
        for choice in SkinChoice::options() {
            let mut doc = seed_doc();
            doc.set_skin_choice(choice);
            let decoded = decode(&encode(&doc), &mut rng()).unwrap();
            assert_eq!(decoded.settings().skin(), choice);
        }
    }

    #[test]
    fn counters_from_the_fixture_keep_ids_that_were_handed_out_before_unique() {
        let mut doc = decode(V2, &mut rng()).unwrap();
        let first_class = doc.classes()[0].id();
        doc.select(first_class);

        let student = doc
            .selected_mut()
            .unwrap()
            .add_student(label("New"), &mut rng());
        let class = doc.create_class(label("Third"), vec![], &mut rng());

        assert_eq!(student.get(), 7, "students 5 and 6 were removed earlier");
        assert_eq!(class.get(), 5, "classes 3 and 4 were deleted earlier");
    }

    #[test]
    fn no_stored_blob_means_first_run() {
        assert_eq!(plan_load(None, &mut rng()), LoadPlan::FirstRun);
    }

    #[test]
    fn a_valid_blob_is_used_as_is() {
        let doc = worked_doc();
        assert_eq!(
            plan_load(Some(&encode(&doc)), &mut rng()),
            LoadPlan::Use(doc)
        );
    }

    #[test]
    fn unreadable_blobs_are_stashed_byte_for_byte_and_never_dropped() {
        let doc_with = |tweak: fn(&mut Value)| {
            let mut value = parse(V1);
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
            doc_with(|v| v["doc"]["classes"][0]["students"] = "nobody".into()),
            doc_with(|v| v["doc"]["classes"][0]["students"][0] = "Ava".into()),
            doc_with(|v| v["doc"]["classes"][0]["grouping"]["of"] = json!({"x": 0})),
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
            match plan_load(Some(&raw), &mut rng()) {
                LoadPlan::StartFresh {
                    stash,
                    reason: DecodeError::Malformed(_),
                } => assert_eq!(stash, raw),
                other => panic!("{raw:?} should be unreadable, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_v2_student_without_a_known_avatar_is_unreadable() {
        let doc_with = |tweak: fn(&mut Value)| {
            let mut value = parse(V2);
            tweak(&mut value);
            value.to_string()
        };
        let blobs = [
            doc_with(|v| {
                v["doc"]["classes"][0]["students"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("avatar");
            }),
            doc_with(|v| v["doc"]["classes"][0]["students"][0]["avatar"] = "dodo".into()),
            doc_with(|v| v["doc"]["classes"][0]["students"][0]["avatar"] = 3.into()),
        ];
        for raw in blobs {
            assert!(
                matches!(decode(&raw, &mut rng()), Err(DecodeError::Malformed(_))),
                "{raw}"
            );
        }
    }

    #[test]
    fn a_newer_version_is_stashed_untouched_instead_of_repaired() {
        let raw = r#"{"version":6,"somethingNew":{"classes":"different"}}"#;
        assert_eq!(
            plan_load(Some(raw), &mut rng()),
            LoadPlan::StartFresh {
                stash: raw.to_owned(),
                reason: DecodeError::NewerVersion(6),
            }
        );
        assert_eq!(
            decode(r#"{"version":99}"#, &mut rng()),
            Err(DecodeError::NewerVersion(99))
        );
    }

    #[test]
    fn centers_that_disagree_with_the_groups_are_repaired_not_rejected() {
        let cases = [
            (json!(["A", "B", "C", "D"]), 9, ["A", "B"]),
            (json!(["Only"]), 1, ["Only", "Center 2"]),
            (json!([]), 5, ["Center 1", "Center 2"]),
        ];
        for (names, turn, repaired) in cases {
            let mut value = parse(V3);
            value["doc"]["classes"][0]["grouping"]["centers"] =
                json!({"names": names, "turn": turn});

            let doc = decode(&value.to_string(), &mut rng()).unwrap();

            doc.assert_invariants();
            let centers = doc.classes()[0].grouping().unwrap().centers().unwrap();
            let names: Vec<&str> = centers.names().iter().map(Label::as_str).collect();
            assert_eq!(names, repaired, "{turn}");
        }
    }

    #[test]
    fn a_v3_blob_with_unreadable_centers_is_stashed_not_repaired() {
        let centers = [
            json!({"names": ["  "], "turn": 0}),
            json!({"names": ["A", "B"], "turn": -1}),
            json!({"names": ["A", "B"]}),
            json!("rotate"),
        ];
        for centers in centers {
            let mut value = parse(V3);
            value["doc"]["classes"][0]["grouping"]["centers"] = centers;
            let raw = value.to_string();
            assert!(
                matches!(decode(&raw, &mut rng()), Err(DecodeError::Malformed(_))),
                "{raw}"
            );
        }
    }

    #[test]
    fn dangling_references_are_repaired_not_rejected() {
        let mut value = parse(V1);
        let class = &mut value["doc"]["classes"][0];
        class["grouping"]["of"] = json!({"0": 0, "1": 1, "4": 7, "77": 0});
        class["oracle"]["drawn"] = json!([99, 1]);
        class["next_student"] = 0.into();
        value["doc"]["selected"] = 42.into();
        value["doc"]["next_class"] = 0.into();

        let LoadPlan::Use(mut doc) = plan_load(Some(&value.to_string()), &mut rng()) else {
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
        assert_eq!(
            class.oracle_remaining(Draw::Student),
            3,
            "only student 1 had gone"
        );

        let fresh_student = doc
            .selected_mut()
            .unwrap()
            .add_student(label("New"), &mut rng());
        assert_eq!(fresh_student.get(), 5);
        assert_eq!(
            doc.create_class(label("Later"), vec![], &mut rng()).get(),
            3
        );
    }

    #[test]
    fn a_stale_group_in_a_stored_round_is_dropped_on_load() {
        let mut value = parse(V4);
        value["doc"]["classes"][0]["grouping"]["oracle"]["drawn"] = json!([9, 1]);

        let LoadPlan::Use(doc) = plan_load(Some(&value.to_string()), &mut rng()) else {
            panic!("a repairable blob must load");
        };

        doc.assert_invariants();
        assert_eq!(doc.classes()[0].oracle_remaining(Draw::Group), 1);
        assert_eq!(
            parse(&encode(&doc))["doc"]["classes"][0]["grouping"]["oracle"],
            json!({"drawn": [1]})
        );
    }

    #[test]
    fn an_empty_doc_round_trips_with_nothing_selected() {
        let doc = Doc::default();
        let decoded = decode(&encode(&doc), &mut rng()).unwrap();
        assert_eq!(decoded, doc);
        assert!(decoded.selected().is_none());
    }
}
