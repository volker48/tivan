//! Messages the teacher must be able to see. The load policy and the save effect produce them;
//! the banner renders them. Text lives here so it is checked without a browser.

use crate::domain::MAX_LABEL_CHARS;

/// Something the banner should say. At most one is shown at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Saved classes could not be read, so the app started from the seed after copying the old
    /// blob aside.
    StartedFresh,
    /// Saved classes could not be read and no copy could be made, so saving stays off for the
    /// session rather than overwrite the only copy.
    BackupFailed(String),
    /// The stored data was written by a newer build. Saving stays off for the whole session so
    /// that data survives.
    OutOfDate,
    /// The browser's storage could not be read at all. Saving stays off so whatever is there
    /// survives.
    StorageUnreadable(String),
    /// The latest write was rejected. Cleared by the next write that succeeds.
    SaveFailed(String),
    /// A pasted roster had lines the app could not use.
    RosterSkipped {
        added: usize,
        too_long: usize,
    },
    /// The account service did not answer when the page asked who is signed in, so the page
    /// works from this browser alone.
    AccountUnavailable(String),
    /// A signed-in teacher's classes could not be loaded from their account. Saving stays off
    /// for the session so the stored copy survives.
    AccountDocUnreadable(String),
    /// The server stopped recognizing the session, so the page fell back to this browser.
    SessionEnded,
    /// Google sign-in did not finish. Holds the short code the server put in the address.
    SignInFailed(String),
    SignOutFailed(String),
}

impl Notice {
    /// Only a paste that lost lines needs saying anything about.
    pub fn roster_skipped(added: usize, too_long: usize) -> Option<Notice> {
        (too_long > 0).then_some(Notice::RosterSkipped { added, too_long })
    }

    /// Notices about the page's ability to keep data are not dismissible, because hiding them
    /// would hide that changes are being lost.
    pub fn is_dismissible(&self) -> bool {
        matches!(
            self,
            Notice::StartedFresh
                | Notice::RosterSkipped { .. }
                | Notice::AccountUnavailable(_)
                | Notice::SessionEnded
                | Notice::SignInFailed(_)
                | Notice::SignOutFailed(_)
        )
    }

    /// About what this browser's storage holds, which stops mattering once the teacher's account
    /// is loaded and saving there.
    pub fn is_about_local_storage(&self) -> bool {
        matches!(
            self,
            Notice::StartedFresh
                | Notice::BackupFailed(_)
                | Notice::OutOfDate
                | Notice::StorageUnreadable(_)
        )
    }

    /// A notice the teacher can dismiss never replaces one they cannot, so hiding a routine
    /// message cannot also hide that changes are not being saved.
    pub fn may_replace(&self, current: &Notice) -> bool {
        current.is_dismissible() || !self.is_dismissible()
    }

    pub fn message(&self) -> String {
        match self {
            Notice::StartedFresh => "Your saved classes could not be read, \
                so Tivan started with a fresh set. A copy of the old data is kept in this browser."
                .to_owned(),
            Notice::BackupFailed(cause) => format!(
                "Your saved classes could not be read, and a copy could not be made ({cause}). \
                They are left in place, so changes will not be saved."
            ),
            Notice::OutOfDate => "This page is out of date. Your saved classes were made by a \
                newer version of Tivan, so changes will not be saved until you reload the page."
                .to_owned(),
            Notice::StorageUnreadable(cause) => format!(
                "This browser's storage could not be opened ({cause}), \
                so changes will not be saved."
            ),
            Notice::SaveFailed(cause) => format!(
                "Your latest changes could not be saved ({cause}). \
                They stay on this page until you close it."
            ),
            Notice::RosterSkipped { added, too_long } => format!(
                "{} added, {} too long. Names can be up to {} characters.",
                count(*added, "name"),
                count(*too_long, "line"),
                MAX_LABEL_CHARS,
            ),
            Notice::AccountUnavailable(cause) => format!(
                "Tivan could not check your account ({cause}), \
                so your classes are saved in this browser only for now."
            ),
            Notice::AccountDocUnreadable(cause) => format!(
                "Your classes could not be loaded from your account ({cause}), \
                so changes will not be saved. Reload the page to try again."
            ),
            Notice::SessionEnded => "You were signed out, so your classes are saved in this \
                browser only. Sign in with Google again to keep them in your account."
                .to_owned(),
            Notice::SignInFailed(code) => {
                format!("Google sign-in did not finish ({code}). Please try again.")
            }
            Notice::SignOutFailed(cause) => {
                format!("Could not sign you out ({cause}). Please try again.")
            }
        }
    }
}

/// "1 name", "3 names".
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_skipped_reads_as_names_added_and_lines_too_long() {
        let notice = Notice::RosterSkipped {
            added: 12,
            too_long: 3,
        };
        assert!(
            notice
                .message()
                .starts_with("12 names added, 3 lines too long.")
        );
    }

    #[test]
    fn a_clean_paste_has_nothing_to_report() {
        assert_eq!(Notice::roster_skipped(12, 0), None);
        assert!(Notice::roster_skipped(12, 1).is_some());
    }

    #[test]
    fn counts_are_singular_for_one() {
        let notice = Notice::RosterSkipped {
            added: 1,
            too_long: 1,
        };
        assert!(
            notice
                .message()
                .starts_with("1 name added, 1 line too long.")
        );
    }

    #[test]
    fn only_notices_that_do_not_hide_data_loss_can_be_dismissed() {
        assert!(Notice::StartedFresh.is_dismissible());
        assert!(
            Notice::RosterSkipped {
                added: 1,
                too_long: 1
            }
            .is_dismissible()
        );
        assert!(!Notice::OutOfDate.is_dismissible());
        assert!(!Notice::SaveFailed("full".into()).is_dismissible());
        assert!(!Notice::StorageUnreadable("blocked".into()).is_dismissible());
        assert!(!Notice::BackupFailed("full".into()).is_dismissible());
    }

    #[test]
    fn a_dismissible_notice_cannot_replace_one_that_must_stay() {
        let skipped = Notice::roster_skipped(3, 1).unwrap();
        for sticky in [
            Notice::OutOfDate,
            Notice::SaveFailed("full".into()),
            Notice::StorageUnreadable("blocked".into()),
        ] {
            assert!(!skipped.may_replace(&sticky));
            assert!(sticky.may_replace(&skipped));
        }
        assert!(skipped.may_replace(&skipped));
        assert!(Notice::OutOfDate.may_replace(&Notice::SaveFailed("full".into())));
    }

    #[test]
    fn account_notices_that_hide_unsaved_changes_cannot_be_dismissed() {
        assert!(!Notice::AccountDocUnreadable("db down".into()).is_dismissible());
        for routine in [
            Notice::AccountUnavailable("offline".into()),
            Notice::SessionEnded,
            Notice::SignInFailed("access_denied".into()),
            Notice::SignOutFailed("offline".into()),
        ] {
            assert!(routine.is_dismissible(), "{routine:?}");
            assert!(!routine.message().is_empty());
        }
        assert!(
            !Notice::SessionEnded.may_replace(&Notice::AccountDocUnreadable("x".into())),
            "a banner that can be dismissed must not hide one that cannot"
        );
    }

    #[test]
    fn account_messages_carry_the_cause_or_code() {
        for notice in [
            Notice::AccountUnavailable("the server answered 500".into()),
            Notice::AccountDocUnreadable("the server answered 500".into()),
            Notice::SignOutFailed("the server answered 500".into()),
        ] {
            assert!(notice.message().contains("the server answered 500"));
        }
        assert!(
            Notice::SignInFailed("access_denied".into())
                .message()
                .contains("access_denied")
        );
    }

    #[test]
    fn only_notices_about_this_browsers_storage_are_cleared_when_the_account_loads() {
        for local in [
            Notice::StartedFresh,
            Notice::BackupFailed("full".into()),
            Notice::OutOfDate,
            Notice::StorageUnreadable("blocked".into()),
        ] {
            assert!(local.is_about_local_storage(), "{local:?}");
        }
        for other in [
            Notice::SaveFailed("full".into()),
            Notice::SessionEnded,
            Notice::roster_skipped(1, 1).unwrap(),
            Notice::AccountDocUnreadable("x".into()),
        ] {
            assert!(!other.is_about_local_storage(), "{other:?}");
        }
    }

    #[test]
    fn failed_backup_message_carries_the_cause() {
        let notice = Notice::BackupFailed("quota exceeded".into());
        assert!(notice.message().contains("quota exceeded"));
    }
}
