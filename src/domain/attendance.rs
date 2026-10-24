//! Who is away today. An absence holds for the one day it was marked on and lapses by itself
//! after it, because the usual case is a child or two out for a day and a teacher should never
//! have to remember to mark them back.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::class::StudentId;
use super::skin::CalendarDay;

/// A date on the teacher's wall clock, so the domain can be given "today" without reading one.
/// `month` is 1 to 12. Orders by date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Day {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Day {
    /// The same date with the year dropped, which is all a season looks at.
    pub fn calendar(self) -> CalendarDay {
        CalendarDay {
            month: self.month,
            day: self.day,
        }
    }
}

/// The students marked absent and the day they were marked on. `students` is never empty: the
/// owning `Class` drops an `Absences` that empties, and keeps it to roster ids.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Absences {
    on: Day,
    students: BTreeSet<StudentId>,
}

impl Absences {
    pub(super) fn new(on: Day, id: StudentId) -> Absences {
        Absences {
            on,
            students: BTreeSet::from([id]),
        }
    }

    pub(super) fn contains(&self, id: StudentId) -> bool {
        self.students.contains(&id)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.students.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.students.is_empty()
    }

    pub(super) fn insert(&mut self, id: StudentId) {
        self.students.insert(id);
    }

    pub(super) fn remove(&mut self, id: StudentId) {
        self.students.remove(&id);
    }

    /// Lapsed once `today` is past the day they were marked on. A day ahead of `today` still
    /// holds, so a device whose clock runs behind never clears what another device marked.
    pub(super) fn lapsed(&self, today: Day) -> bool {
        self.on < today
    }

    pub(super) fn retain(&mut self, keep: impl Fn(StudentId) -> bool) {
        self.students.retain(|&id| keep(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: u16, month: u8, day: u8) -> Day {
        Day { year, month, day }
    }

    #[test]
    fn days_order_by_year_then_month_then_day() {
        assert!(day(2026, 10, 24) < day(2026, 10, 25));
        assert!(day(2026, 10, 31) < day(2026, 11, 1));
        assert!(day(2026, 12, 31) < day(2027, 1, 1));
    }

    #[test]
    fn a_day_drops_its_year_for_the_calendar() {
        assert_eq!(
            day(2026, 10, 24).calendar(),
            CalendarDay { month: 10, day: 24 }
        );
    }

    #[test]
    fn absences_hold_on_their_day_and_lapse_after_it() {
        let absences = Absences::new(day(2026, 10, 24), StudentId(3));
        assert!(!absences.lapsed(day(2026, 10, 24)));
        assert!(absences.lapsed(day(2026, 10, 25)));
        assert!(absences.lapsed(day(2027, 10, 24)));
        assert!(
            !absences.lapsed(day(2026, 10, 23)),
            "a clock behind keeps them"
        );
    }
}
