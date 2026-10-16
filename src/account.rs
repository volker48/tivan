//! Who is using the page and where their classes are saved: the pure half of accounts. The shell
//! asks [`boot`] once at startup and keeps the answer in signals, and [`save_target`] turns those
//! signals into the one place a change may be written, so a save can never reach the server
//! before the server's copy was read.

use crate::api::{Api, ApiError, Http, short_code};
use crate::domain::{Doc, Rng, seed_doc};
use crate::notice::Notice;
use crate::persist::{DecodeError, decode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Session {
    /// The page has asked the server who is signed in and has not heard back yet.
    Checking,
    SignedOut,
    SignedIn {
        email: String,
        name: String,
    },
}

impl Session {
    /// What the account chip says: the teacher's first name, or their email when Google gave no
    /// name. `None` when nobody is signed in.
    pub fn short_name(&self) -> Option<&str> {
        let Session::SignedIn { email, name } = self else {
            return None;
        };
        Some(name.split_whitespace().next().unwrap_or(email))
    }
}

/// What this page knows about the teacher's classes on the server. Only a signed-in session has
/// any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteDoc {
    /// Not read yet, or there is no one signed in to read it for.
    Unread,
    /// Read, or known to be absent. This page may write it.
    Ready,
    /// Could not be read, or was written by a newer build. It is never written, so it survives.
    Unusable,
}

/// Where a change to the classes is saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveTarget {
    Nowhere,
    /// This browser's `localStorage`.
    Local,
    /// The teacher's account on the server.
    Remote,
}

/// `local_can_save` is [`store::Loaded::can_save`](crate::store::Loaded::can_save): false when
/// what is in this browser's storage must survive untouched. The server is written only after its
/// copy was read, and this browser's storage only when nobody is signed in.
pub fn save_target(session: &Session, remote: RemoteDoc, local_can_save: bool) -> SaveTarget {
    match (session, remote) {
        (Session::SignedOut, _) if local_can_save => SaveTarget::Local,
        (Session::SignedIn { .. }, RemoteDoc::Ready) => SaveTarget::Remote,
        _ => SaveTarget::Nowhere,
    }
}

/// What the startup check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Booted {
    pub session: Session,
    pub remote: RemoteDoc,
    /// Replaces the doc the page is showing. `None` keeps it.
    pub doc: Option<Doc>,
    /// The envelope the server is known to hold, so the first save does not send it back.
    pub synced: Option<String>,
    pub notice: Option<Notice>,
}

impl Booted {
    fn signed_out(notice: Option<Notice>) -> Booted {
        Booted {
            session: Session::SignedOut,
            remote: RemoteDoc::Unread,
            doc: None,
            synced: None,
            notice,
        }
    }

    fn unusable(session: Session, notice: Notice) -> Booted {
        Booted {
            session,
            remote: RemoteDoc::Unusable,
            doc: None,
            synced: None,
            notice: Some(notice),
        }
    }
}

/// Asks who is signed in and, for a teacher, reads their classes. `local` is the doc the page
/// is showing, which an account with nothing stored takes over, except that the sample classes
/// are not a teacher's data and an empty doc stands in for them. A doc already on the server
/// always wins, and the copy in this browser is left alone. Never writes: a read that fails
/// leaves the server's copy `Unusable`, so nothing can overwrite what could not be read.
/// `rng` picks avatars when the stored doc predates them.
pub async fn boot<H: Http>(api: &Api<H>, local: &Doc, rng: &mut impl Rng) -> Booted {
    let session = match api.me().await {
        Ok(Some(me)) => Session::SignedIn {
            email: me.email,
            name: me.name,
        },
        Ok(None) => return Booted::signed_out(None),
        Err(error) => {
            return Booted::signed_out(Some(Notice::AccountUnavailable(error.to_string())));
        }
    };
    let stored = match api.doc().await {
        Ok(stored) => stored,
        Err(ApiError::Unauthorized) => return Booted::signed_out(Some(Notice::SessionEnded)),
        Err(error) => {
            return Booted::unusable(session, Notice::AccountDocUnreadable(error.to_string()));
        }
    };
    let Some(envelope) = stored else {
        return Booted {
            session,
            remote: RemoteDoc::Ready,
            doc: (*local == seed_doc()).then(Doc::default),
            synced: None,
            notice: None,
        };
    };
    match decode(&envelope, rng) {
        Ok(doc) => Booted {
            session,
            remote: RemoteDoc::Ready,
            doc: Some(doc),
            synced: Some(envelope),
            notice: None,
        },
        Err(DecodeError::NewerVersion(_)) => Booted::unusable(session, Notice::OutOfDate),
        Err(DecodeError::Malformed(cause)) => {
            Booted::unusable(session, Notice::AccountDocUnreadable(cause))
        }
    }
}

/// The code in `?auth_error=<code>`, where the server sends a teacher whose Google sign-in did
/// not finish. A value that is not a short code is dropped rather than shown, because anyone can
/// send a teacher a link.
pub fn auth_error_code(search: &str) -> Option<String> {
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("auth_error="))
        .and_then(short_code)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Method;
    use crate::api::fake::{FakeHttp, block_on};
    use crate::domain::SplitMix64;
    use crate::persist::{SCHEMA_VERSION, encode};

    const ME: &str = r#"{"email":"ada@school.org","name":"Ada Lovelace"}"#;

    fn rng() -> SplitMix64 {
        SplitMix64::seeded(1)
    }

    fn signed_in() -> Session {
        Session::SignedIn {
            email: "ada@school.org".to_owned(),
            name: "Ada Lovelace".to_owned(),
        }
    }

    fn account_server() -> FakeHttp {
        FakeHttp::default().reply(Method::Get, "/api/me", 200, ME)
    }

    fn boot_with(server: &FakeHttp, local: &Doc) -> Booted {
        block_on(boot(&Api::new(server), local, &mut rng()))
    }

    /// A doc a teacher has been working in: one sample class gone.
    fn worked_on() -> Doc {
        let mut doc = seed_doc();
        doc.delete_class(doc.classes()[0].id());
        doc
    }

    fn target(booted: &Booted, local_can_save: bool) -> SaveTarget {
        save_target(&booted.session, booted.remote, local_can_save)
    }

    #[test]
    fn the_server_is_written_only_for_a_teacher_whose_server_copy_was_read() {
        let sessions = [Session::Checking, Session::SignedOut, signed_in()];
        let remotes = [RemoteDoc::Unread, RemoteDoc::Ready, RemoteDoc::Unusable];
        for session in &sessions {
            for remote in remotes {
                for local_can_save in [true, false] {
                    let expected = match (session, remote, local_can_save) {
                        (Session::SignedOut, _, true) => SaveTarget::Local,
                        (Session::SignedIn { .. }, RemoteDoc::Ready, _) => SaveTarget::Remote,
                        _ => SaveTarget::Nowhere,
                    };
                    assert_eq!(
                        save_target(session, remote, local_can_save),
                        expected,
                        "{session:?} {remote:?} {local_can_save}"
                    );
                }
            }
        }
    }

    #[test]
    fn nothing_is_written_while_the_page_is_still_checking_who_is_signed_in() {
        for remote in [RemoteDoc::Unread, RemoteDoc::Ready, RemoteDoc::Unusable] {
            assert_eq!(
                save_target(&Session::Checking, remote, true),
                SaveTarget::Nowhere
            );
        }
    }

    #[test]
    fn the_chip_shows_the_first_name_or_the_email() {
        let named = |name: &str| Session::SignedIn {
            email: "ada@school.org".to_owned(),
            name: name.to_owned(),
        };
        assert_eq!(named("Ada Lovelace").short_name(), Some("Ada"));
        assert_eq!(named("  Grace  ").short_name(), Some("Grace"));
        assert_eq!(named("Cher").short_name(), Some("Cher"));
        assert_eq!(named("").short_name(), Some("ada@school.org"));
        assert_eq!(named("  ").short_name(), Some("ada@school.org"));
        assert_eq!(Session::SignedOut.short_name(), None);
        assert_eq!(Session::Checking.short_name(), None);
    }

    #[test]
    fn nobody_signed_in_keeps_the_page_as_it_is_and_saves_locally() {
        let server = FakeHttp::default().reply(Method::Get, "/api/me", 401, "");

        let booted = boot_with(&server, &worked_on());

        assert_eq!(booted, Booted::signed_out(None));
        assert_eq!(target(&booted, true), SaveTarget::Local);
        assert_eq!(
            server.requests().len(),
            1,
            "a signed-out teacher never touches the doc"
        );
    }

    #[test]
    fn a_host_without_an_account_service_is_just_signed_out() {
        for server in [
            FakeHttp::default(),
            FakeHttp::default().reply(Method::Get, "/api/me", 200, "<!DOCTYPE html>"),
        ] {
            let booted = boot_with(&server, &seed_doc());
            assert_eq!(booted, Booted::signed_out(None));
        }
    }

    #[test]
    fn an_account_service_that_cannot_be_reached_still_leaves_the_page_saving_locally() {
        for server in [
            FakeHttp::default().unreachable(Method::Get, "/api/me"),
            FakeHttp::default().reply(Method::Get, "/api/me", 500, r#"{"error":"db_down"}"#),
        ] {
            let booted = boot_with(&server, &seed_doc());

            assert_eq!(booted.session, Session::SignedOut);
            assert!(matches!(booted.notice, Some(Notice::AccountUnavailable(_))));
            assert_eq!(target(&booted, true), SaveTarget::Local);
            assert_eq!(booted.doc, None);
        }
    }

    #[test]
    fn the_servers_doc_wins_and_is_not_sent_back() {
        let stored = encode(&worked_on());
        let server = account_server().reply(Method::Get, "/api/doc", 200, &stored);

        let booted = boot_with(&server, &seed_doc());

        assert_eq!(booted.session, signed_in());
        assert_eq!(booted.remote, RemoteDoc::Ready);
        assert_eq!(booted.doc, Some(worked_on()));
        assert_eq!(booted.synced, Some(stored));
        assert_eq!(booted.notice, None);
        assert_eq!(target(&booted, true), SaveTarget::Remote);
        assert!(!server.wrote());
    }

    #[test]
    fn the_servers_doc_wins_over_a_local_doc_that_is_not_the_seed() {
        let mut local = worked_on();
        local.delete_class(local.classes()[0].id());
        let server = account_server().reply(Method::Get, "/api/doc", 200, &encode(&worked_on()));

        assert_eq!(boot_with(&server, &local).doc, Some(worked_on()));
    }

    #[test]
    fn an_account_with_nothing_stored_takes_over_what_the_browser_has() {
        let server = account_server().reply(Method::Get, "/api/doc", 204, "");

        let booted = boot_with(&server, &worked_on());

        assert_eq!(booted.session, signed_in());
        assert_eq!(booted.remote, RemoteDoc::Ready);
        assert_eq!(booted.doc, None, "the page keeps its own classes");
        assert_eq!(booted.synced, None, "so the first save uploads them");
        assert_eq!(target(&booted, true), SaveTarget::Remote);
    }

    #[test]
    fn the_sample_classes_are_not_uploaded_as_a_teachers_data() {
        let server = account_server().reply(Method::Get, "/api/doc", 204, "");

        let booted = boot_with(&server, &seed_doc());

        assert_eq!(booted.doc, Some(Doc::default()));
        assert_eq!(booted.remote, RemoteDoc::Ready);
    }

    #[test]
    fn a_doc_from_a_newer_build_is_never_replaced() {
        let newer = format!(r#"{{"version":{},"doc":{{}}}}"#, SCHEMA_VERSION + 1);
        let server = account_server().reply(Method::Get, "/api/doc", 200, &newer);

        let booted = boot_with(&server, &worked_on());

        assert_eq!(booted.session, signed_in());
        assert_eq!(booted.remote, RemoteDoc::Unusable);
        assert_eq!(booted.notice, Some(Notice::OutOfDate));
        assert_eq!(booted.doc, None);
        assert_eq!(target(&booted, true), SaveTarget::Nowhere);
        assert!(!server.wrote());
    }

    #[test]
    fn a_doc_that_cannot_be_read_is_never_replaced_and_says_why() {
        let server = account_server().reply(Method::Get, "/api/doc", 200, "{not json");

        let booted = boot_with(&server, &worked_on());

        assert_eq!(booted.remote, RemoteDoc::Unusable);
        assert!(matches!(
            booted.notice,
            Some(Notice::AccountDocUnreadable(_))
        ));
        assert_eq!(target(&booted, true), SaveTarget::Nowhere);
        assert!(!server.wrote());
    }

    #[test]
    fn a_failed_read_of_the_doc_never_leads_to_a_write() {
        let failures = [
            account_server().reply(Method::Get, "/api/doc", 500, r#"{"error":"db_down"}"#),
            account_server().reply(Method::Get, "/api/doc", 503, ""),
            account_server().unreachable(Method::Get, "/api/doc"),
        ];
        for server in failures {
            let booted = boot_with(&server, &seed_doc());

            assert_eq!(booted.session, signed_in());
            assert_eq!(booted.remote, RemoteDoc::Unusable);
            assert!(matches!(
                booted.notice,
                Some(Notice::AccountDocUnreadable(_))
            ));
            assert_eq!(booted.doc, None);
            assert_eq!(target(&booted, true), SaveTarget::Nowhere);
            assert!(!server.wrote());
        }
    }

    #[test]
    fn a_session_that_ends_between_the_two_questions_signs_the_teacher_out() {
        let server = account_server().reply(Method::Get, "/api/doc", 401, "");

        let booted = boot_with(&server, &worked_on());

        assert_eq!(booted, Booted::signed_out(Some(Notice::SessionEnded)));
        assert_eq!(target(&booted, true), SaveTarget::Local);
    }

    #[test]
    fn the_auth_error_is_read_from_the_query_and_only_when_it_is_a_short_code() {
        assert_eq!(
            auth_error_code("?auth_error=access_denied"),
            Some("access_denied".to_owned())
        );
        assert_eq!(
            auth_error_code("?x=1&auth_error=state_mismatch&y=2"),
            Some("state_mismatch".to_owned())
        );
        for search in [
            "",
            "?",
            "?other=1",
            "?auth_error=",
            "?auth_error=Call%20555",
            "?auth_error=<b>",
            "?not_auth_error=x",
        ] {
            assert_eq!(auth_error_code(search), None, "{search}");
        }
    }
}
