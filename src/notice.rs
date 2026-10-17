//! Messages the teacher must be able to see. The load policy and the save effect produce them;
//! the banner renders them. Text lives here so it is checked without a browser.

use crate::domain::{MAX_LABEL_CHARS, SyncReport};

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

    /// What to say when the account session ends. Edits go to this browser's storage from then
    /// on, unless that cannot be written either, and then they are lost on close and the
    /// warning must say so and stay.
    pub fn after_session_ended(local_can_save: bool) -> Notice {
        if local_can_save {
            Notice::SessionEnded
        } else {
            Notice::SaveFailed(
                "your session ended and this browser cannot save classes either".to_owned(),
            )
        }
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
            Notice::SessionEnded => "You were signed out. Changes from now on are saved in this \
                browser only, and signing in again loads the classes saved in your account."
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
    plural(n, noun, &format!("{noun}s"))
}

/// "1 class", "2 classes".
fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The line the teacher reads after a Classroom sync, such as "Added 2 classes, 31 new students,
/// removed 3 students."
pub fn sync_summary(report: &SyncReport) -> String {
    let mut added = Vec::new();
    if report.classes_added > 0 {
        added.push(plural(report.classes_added, "class", "classes"));
    }
    if report.students_added > 0 {
        added.push(plural(report.students_added, "new student", "new students"));
    }
    let removed = (report.students_removed > 0)
        .then(|| plural(report.students_removed, "student", "students"));
    match (added.is_empty(), removed) {
        (true, None) => "No changes made.".to_owned(),
        (false, None) => format!("Added {}.", added.join(", ")),
        (true, Some(removed)) => format!("Removed {removed}."),
        (false, Some(removed)) => format!("Added {}, removed {removed}.", added.join(", ")),
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

    /// Syncs `first` into an empty doc, then `later` into the result, and reports the second sync.
    /// A course is `(id, student names)`, and each student's id is their name.
    fn report_of(first: &[(&str, &[&str])], later: &[(&str, &[&str])]) -> SyncReport {
        use crate::domain::testing::course;
        use crate::domain::{ClassroomCourse, Doc, SplitMix64, SyncPlan};
        let build = |courses: &[(&str, &[&str])]| -> Vec<ClassroomCourse> {
            courses
                .iter()
                .map(|(id, names)| {
                    let students: Vec<_> = names.iter().map(|name| (*name, *name)).collect();
                    course(id, id, &students)
                })
                .collect()
        };
        let mut doc = Doc::default();
        let mut rng = SplitMix64::seeded(1);
        SyncPlan::new(&doc, build(first)).apply(&mut doc, &mut rng);
        SyncPlan::new(&doc, build(later)).apply(&mut doc, &mut rng)
    }

    #[test]
    fn a_sync_summary_says_what_was_added_and_what_was_removed() {
        let first = report_of(&[], &[("A", &["x", "y"]), ("B", &["z"])]);
        assert_eq!(sync_summary(&first), "Added 2 classes, 3 new students.");

        let one = report_of(&[], &[("A", &["x"])]);
        assert_eq!(sync_summary(&one), "Added 1 class, 1 new student.");

        let changed = report_of(&[("A", &["x", "y", "z"])], &[("A", &["x", "w"])]);
        assert_eq!(
            sync_summary(&changed),
            "Added 1 new student, removed 2 students."
        );

        let only_removed = report_of(&[("A", &["x", "y"])], &[("A", &["x"])]);
        assert_eq!(sync_summary(&only_removed), "Removed 1 student.");

        let empty_class = report_of(&[], &[("A", &[])]);
        assert_eq!(sync_summary(&empty_class), "Added 1 class.");
    }

    #[test]
    fn a_sync_that_changes_nothing_says_so_without_claiming_classroom_is_imported() {
        let same = report_of(&[("A", &["x"])], &[("A", &["x"])]);
        assert_eq!(sync_summary(&same), "No changes made.");
    }

    #[test]
    fn an_ended_session_promises_browser_saving_only_when_the_browser_can_save() {
        assert_eq!(Notice::after_session_ended(true), Notice::SessionEnded);
        let lost = Notice::after_session_ended(false);
        assert!(!lost.is_dismissible());
        assert!(lost.message().contains("could not be saved"));
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
