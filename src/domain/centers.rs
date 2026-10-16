//! Learning centers: fixed stations in the room, one per group. Which group stands at which center
//! is a rotation of the group order, so pressing rotate moves every group one station on and no
//! group is ever dealt a center at random.

use serde::{Deserialize, Serialize};

use super::grouping::{GroupIdx, MoveError};
use super::label::Label;

/// Position of a center in its [`Centers`]. Centers are never inserted or removed individually, so
/// an index stays valid until the groups are dealt afresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CenterIdx(pub(super) usize);

impl CenterIdx {
    pub fn get(self) -> usize {
        self.0
    }
}

/// One center per group: `names.len()` equals the group count of the owning
/// [`Grouping`](super::grouping::Grouping), which is at least 1, and `turn < names.len()`.
/// Group `g` stands at center `(g + turn) % n`, so `n - 1` rotations take every group past every
/// center and `n` rotations bring them home.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Centers {
    names: Vec<Label>,
    turn: usize,
}

fn default_name(number: usize) -> Label {
    Label::parse(&format!("Center {number}")).expect("a default center name is a valid label")
}

impl Centers {
    /// "Center 1" to "Center `count`", at round 1.
    pub(super) fn new(count: usize) -> Centers {
        let mut centers = Centers {
            names: Vec::new(),
            turn: 0,
        };
        centers.fit(count);
        centers
    }

    /// Makes this exactly `count` centers: truncates or appends default names, and wraps `turn`
    /// into range. Idempotent.
    pub(super) fn fit(&mut self, count: usize) {
        self.names.truncate(count);
        for number in self.names.len() + 1..=count {
            self.names.push(default_name(number));
        }
        self.turn = self.turn.checked_rem(count).unwrap_or(0);
    }

    /// Back to round 1, as when the groups are dealt afresh.
    pub(super) fn restart(&mut self) {
        self.turn = 0;
    }

    /// Moves every group to the next center. After the last center the cycle starts over.
    pub fn rotate(&mut self) {
        self.turn = (self.turn + 1) % self.names.len();
    }

    pub fn names(&self) -> &[Label] {
        &self.names
    }

    /// Where the cycle stands, counting from 1: "Round 2 of 5" is `round() == 2` and
    /// `names().len() == 5`.
    pub fn round(&self) -> usize {
        self.turn + 1
    }

    /// `group` must belong to the owning grouping.
    pub fn center_of(&self, group: GroupIdx) -> CenterIdx {
        CenterIdx((group.0 + self.turn) % self.names.len())
    }

    pub fn group_at(&self, center: CenterIdx) -> GroupIdx {
        let count = self.names.len();
        GroupIdx((center.0 + count - self.turn) % count)
    }

    pub fn rename(&mut self, idx: CenterIdx, name: Label) -> Result<(), MoveError> {
        let slot = self.names.get_mut(idx.0).ok_or(MoveError::UnknownCenter)?;
        *slot = name;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self, groups: usize) {
        assert_eq!(self.names.len(), groups, "one center per group");
        assert!(self.turn < groups, "turn out of range");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::names::MAX_GROUPS;
    use crate::domain::testing::label;

    fn names(centers: &Centers) -> Vec<&str> {
        centers.names().iter().map(Label::as_str).collect()
    }

    #[test]
    fn new_centers_are_numbered_from_one_and_start_at_round_one() {
        let centers = Centers::new(3);
        assert_eq!(names(&centers), ["Center 1", "Center 2", "Center 3"]);
        assert_eq!(centers.round(), 1);
        centers.assert_invariants(3);
    }

    #[test]
    fn at_round_one_group_i_stands_at_center_i() {
        let centers = Centers::new(5);
        for index in 0..5 {
            assert_eq!(centers.center_of(GroupIdx(index)), CenterIdx(index));
        }
    }

    #[test]
    fn a_group_visits_every_center_once_and_comes_home_after_n_rotations() {
        for count in 1..=MAX_GROUPS {
            let mut centers = Centers::new(count);
            let mut visited: Vec<Vec<CenterIdx>> = vec![Vec::new(); count];
            for _ in 0..count {
                for (group, trail) in visited.iter_mut().enumerate() {
                    trail.push(centers.center_of(GroupIdx(group)));
                }
                centers.rotate();
                centers.assert_invariants(count);
            }

            for (group, trail) in visited.iter_mut().enumerate() {
                assert_eq!(
                    trail[0],
                    CenterIdx(group),
                    "n={count} group {group} starts home"
                );
                trail.sort();
                let every_center: Vec<CenterIdx> = (0..count).map(CenterIdx).collect();
                assert_eq!(*trail, every_center, "n={count} group {group}");
                assert_eq!(centers.center_of(GroupIdx(group)), CenterIdx(group));
            }
            assert_eq!(centers.round(), 1);
        }
    }

    #[test]
    fn rotating_moves_every_group_to_the_next_center() {
        let mut centers = Centers::new(4);
        centers.rotate();
        assert_eq!(centers.round(), 2);
        let after: Vec<usize> = (0..4)
            .map(|group| centers.center_of(GroupIdx(group)).get())
            .collect();
        assert_eq!(after, [1, 2, 3, 0]);
    }

    #[test]
    fn each_center_holds_exactly_one_group_at_every_round() {
        for count in 1..=MAX_GROUPS {
            let mut centers = Centers::new(count);
            for _ in 0..count {
                for group in (0..count).map(GroupIdx) {
                    assert_eq!(centers.group_at(centers.center_of(group)), group);
                }
                for center in (0..count).map(CenterIdx) {
                    assert_eq!(centers.center_of(centers.group_at(center)), center);
                }
                centers.rotate();
            }
        }
    }

    #[test]
    fn a_single_center_rotates_in_place() {
        let mut centers = Centers::new(1);
        centers.rotate();
        assert_eq!(centers.round(), 1);
        assert_eq!(centers.center_of(GroupIdx(0)), CenterIdx(0));
    }

    #[test]
    fn fit_truncates_or_appends_default_names_and_keeps_the_rest() {
        let mut centers = Centers::new(3);
        centers.rename(CenterIdx(1), label("Reading Nook")).unwrap();

        centers.fit(5);
        assert_eq!(
            names(&centers),
            [
                "Center 1",
                "Reading Nook",
                "Center 3",
                "Center 4",
                "Center 5"
            ]
        );

        centers.fit(2);
        assert_eq!(names(&centers), ["Center 1", "Reading Nook"]);
        centers.assert_invariants(2);
    }

    #[test]
    fn fit_wraps_the_turn_into_range_and_is_idempotent() {
        let mut centers = Centers::new(5);
        for _ in 0..4 {
            centers.rotate();
        }
        assert_eq!(centers.round(), 5);

        centers.fit(3);
        assert_eq!(centers.round(), 2, "round 5 wraps to 5 % 3 in a cycle of 3");
        let once = centers.clone();
        centers.fit(3);
        assert_eq!(centers, once);
        centers.assert_invariants(3);
    }

    #[test]
    fn fit_to_nothing_does_not_panic() {
        let mut centers = Centers::new(2);
        centers.rotate();
        centers.fit(0);
        assert!(centers.names().is_empty());
        assert_eq!(centers.round(), 1);
    }

    #[test]
    fn restart_returns_to_round_one_and_keeps_the_names() {
        let mut centers = Centers::new(3);
        centers.rename(CenterIdx(0), label("Computers")).unwrap();
        centers.rotate();
        centers.rotate();

        centers.restart();

        assert_eq!(centers.round(), 1);
        assert_eq!(names(&centers), ["Computers", "Center 2", "Center 3"]);
    }

    #[test]
    fn rename_replaces_one_name_and_rejects_an_unknown_center() {
        let mut centers = Centers::new(2);
        centers.rename(CenterIdx(1), label("Art Table")).unwrap();
        assert_eq!(names(&centers), ["Center 1", "Art Table"]);

        let before = centers.clone();
        assert_eq!(
            centers.rename(CenterIdx(2), label("x")),
            Err(MoveError::UnknownCenter)
        );
        assert_eq!(centers, before);
    }

    #[test]
    fn json_round_trips_the_names_and_the_turn() {
        let mut centers = Centers::new(3);
        centers.rename(CenterIdx(2), label("Listening")).unwrap();
        centers.rotate();

        let json = serde_json::to_value(&centers).unwrap();

        assert_eq!(
            json,
            serde_json::json!({"names": ["Center 1", "Center 2", "Listening"], "turn": 1})
        );
        assert_eq!(serde_json::from_value::<Centers>(json).unwrap(), centers);
    }
}
