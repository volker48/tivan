use std::fmt;

use serde::{Deserialize, Serialize};

use super::avatar::Avatar;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A month and day with no year or clock, so the domain can be given "today" without reading one.
/// `month` is 1 to 12. Orders by calendar position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalendarDay {
    pub month: u8,
    pub day: u8,
}

impl fmt::Display for CalendarDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", MONTHS[usize::from(self.month) - 1], self.day)
    }
}

/// The days of a year, both ends included, that a skin is in season. It never spans New Year.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Season {
    pub from: CalendarDay,
    pub to: CalendarDay,
}

impl Season {
    pub fn contains(self, day: CalendarDay) -> bool {
        self.from <= day && day <= self.to
    }
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} to {}", self.from, self.to)
    }
}

/// A holiday look drawn over every student's avatar, each avatar wearing one of its costumes.
/// Stored by name, so renaming or removing a variant is a schema change, and adding one moves the
/// schema version too, so an older build refuses a blob naming a skin it cannot draw. Seasons must
/// not overlap, so at most one skin is ever in season.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Skin {
    Halloween,
    Christmas,
}

impl Skin {
    pub const ALL: [Skin; 2] = [Skin::Halloween, Skin::Christmas];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            Skin::Halloween => "halloween",
            Skin::Christmas => "christmas",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Skin::Halloween => "Halloween",
            Skin::Christmas => "Christmas",
        }
    }

    pub fn season(self) -> Season {
        let day = |month, day| CalendarDay { month, day };
        match self {
            Skin::Halloween => Season {
                from: day(10, 1),
                to: day(10, 31),
            },
            Skin::Christmas => Season {
                from: day(12, 1),
                to: day(12, 25),
            },
        }
    }

    /// The costumes this skin hands out, in turn order.
    fn costumes(self) -> &'static [Costume] {
        match self {
            Skin::Halloween => &[
                Costume::WitchHat,
                Costume::PumpkinHat,
                Costume::Bat,
                Costume::Spider,
            ],
            Skin::Christmas => &[Costume::SantaHat, Costume::Holly],
        }
    }

    /// What `avatar` wears in this skin. Avatars take turns, in [`Avatar::ALL`] order, through the
    /// costumes that fit them, so neighbours dress differently and an avatar always wears the same
    /// one. Every skin has a costume that fits every avatar.
    pub fn costume(self, avatar: Avatar) -> Costume {
        let fitting: Vec<Costume> = self
            .costumes()
            .iter()
            .copied()
            .filter(|costume| costume.fits(avatar))
            .collect();
        fitting[avatar as usize % fitting.len()]
    }
}

/// One piece of art a skin draws over an avatar. Worked out from the skin and the avatar each time
/// and never stored, so it has no serde, and adding or renaming one is not a schema change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Costume {
    WitchHat,
    PumpkinHat,
    Bat,
    Spider,
    SantaHat,
    Holly,
}

impl Costume {
    pub const ALL: [Costume; 6] = [
        Costume::WitchHat,
        Costume::PumpkinHat,
        Costume::Bat,
        Costume::Spider,
        Costume::SantaHat,
        Costume::Holly,
    ];

    /// The suffix of the sprite symbol `costume-<name>`.
    pub fn name(self) -> &'static str {
        match self {
            Costume::WitchHat => "witch-hat",
            Costume::PumpkinHat => "pumpkin-hat",
            Costume::Bat => "bat",
            Costume::Spider => "spider",
            Costume::SantaHat => "santa-hat",
            Costume::Holly => "holly",
        }
    }

    /// Whether this costume looks right on `avatar`. A Halloween hat rests on the crown, which the
    /// frog's eyes and the bunny's ears take up, so on them it hovers. Any hat would hide the
    /// unicorn's horn and the shark's fin, and the bat perches on the crown too, so it would hide
    /// them as well. The spider and the holly hang to one side and fit everyone.
    fn fits(self, avatar: Avatar) -> bool {
        match self {
            Costume::WitchHat | Costume::PumpkinHat => !matches!(
                avatar,
                Avatar::Frog | Avatar::Bunny | Avatar::Unicorn | Avatar::Shark
            ),
            Costume::SantaHat => !matches!(avatar, Avatar::Unicorn | Avatar::Shark),
            Costume::Bat => !matches!(avatar, Avatar::Unicorn | Avatar::Shark),
            Costume::Spider | Costume::Holly => true,
        }
    }
}

/// What the teacher asked for. Stored as `"plain"`, `"auto"`, or `{"fixed":"<skin>"}`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinChoice {
    /// No skin, ever.
    Plain,
    /// The skin whose season contains today, and none outside every season.
    #[default]
    Auto,
    /// This skin, in or out of season.
    #[serde(rename = "fixed")]
    Skin(Skin),
}

impl SkinChoice {
    /// The choices in the order a teacher is offered them.
    pub fn options() -> impl Iterator<Item = SkinChoice> {
        [SkinChoice::Plain, SkinChoice::Auto]
            .into_iter()
            .chain(Skin::ALL.map(SkinChoice::Skin))
    }

    pub fn label(self) -> &'static str {
        match self {
            SkinChoice::Plain => "Plain",
            SkinChoice::Auto => "Seasonal (automatic)",
            SkinChoice::Skin(skin) => skin.label(),
        }
    }

    /// The skin this choice draws on `today`.
    pub fn resolve(self, today: CalendarDay) -> Option<Skin> {
        match self {
            SkinChoice::Plain => None,
            SkinChoice::Auto => Skin::ALL
                .into_iter()
                .find(|skin| skin.season().contains(today)),
            SkinChoice::Skin(skin) => Some(skin),
        }
    }
}

/// Preferences that travel with the classes. Each has one writer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    skin: SkinChoice,
}

impl Settings {
    pub fn skin(&self) -> SkinChoice {
        self.skin
    }

    pub fn set_skin(&mut self, skin: SkinChoice) {
        self.skin = skin;
    }

    pub fn active_skin(&self, today: CalendarDay) -> Option<Skin> {
        self.skin.resolve(today)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use serde_json::json;

    use super::*;

    fn day(month: u8, day: u8) -> CalendarDay {
        CalendarDay { month, day }
    }

    fn auto_on(today: CalendarDay) -> Option<Skin> {
        Settings::default().active_skin(today)
    }

    #[test]
    fn auto_follows_the_season_through_both_ends_of_each_window() {
        let cases = [
            (day(9, 30), None),
            (day(10, 1), Some(Skin::Halloween)),
            (day(10, 31), Some(Skin::Halloween)),
            (day(11, 1), None),
            (day(11, 30), None),
            (day(12, 1), Some(Skin::Christmas)),
            (day(12, 25), Some(Skin::Christmas)),
            (day(12, 26), None),
            (day(1, 1), None),
        ];
        for (today, expected) in cases {
            assert_eq!(auto_on(today), expected, "{today}");
        }
    }

    #[test]
    fn a_new_settings_value_is_automatic() {
        assert_eq!(Settings::default().skin(), SkinChoice::Auto);
    }

    #[test]
    fn plain_shows_nothing_in_any_season() {
        let mut settings = Settings::default();
        settings.set_skin(SkinChoice::Plain);
        for today in [day(10, 31), day(12, 25), day(7, 4)] {
            assert_eq!(settings.active_skin(today), None, "{today}");
        }
    }

    #[test]
    fn a_fixed_skin_shows_in_and_out_of_season() {
        let mut settings = Settings::default();
        settings.set_skin(SkinChoice::Skin(Skin::Christmas));
        for today in [day(10, 31), day(12, 25), day(7, 4)] {
            assert_eq!(
                settings.active_skin(today),
                Some(Skin::Christmas),
                "{today}"
            );
        }
    }

    #[test]
    fn no_day_belongs_to_two_seasons() {
        for month in 1..=12 {
            for d in 1..=31 {
                let today = day(month, d);
                let in_season: Vec<Skin> = Skin::ALL
                    .into_iter()
                    .filter(|skin| skin.season().contains(today))
                    .collect();
                assert!(
                    in_season.len() <= 1,
                    "{today} is in season for {in_season:?}"
                );
            }
        }
    }

    #[test]
    fn every_season_runs_forward_within_one_year() {
        for skin in Skin::ALL {
            let season = skin.season();
            assert!(season.from <= season.to, "{skin:?}");
        }
    }

    fn costumes_worn(skin: Skin) -> HashMap<Avatar, Costume> {
        Avatar::ALL
            .into_iter()
            .map(|avatar| (avatar, skin.costume(avatar)))
            .collect()
    }

    #[test]
    fn a_hat_never_fits_the_frog_or_the_bunny() {
        for hat in [Costume::WitchHat, Costume::PumpkinHat] {
            for avatar in [Avatar::Frog, Avatar::Bunny] {
                assert!(!hat.fits(avatar), "{hat:?} fits {avatar:?}");
            }
        }
    }

    #[test]
    fn halloween_never_puts_a_hat_on_the_frog_or_the_bunny() {
        let worn = costumes_worn(Skin::Halloween);
        for avatar in [Avatar::Frog, Avatar::Bunny] {
            assert!(
                !matches!(worn[&avatar], Costume::WitchHat | Costume::PumpkinHat),
                "{avatar:?} wears {:?}",
                worn[&avatar]
            );
        }
    }

    #[test]
    fn every_avatar_wears_a_costume_that_fits_it() {
        for skin in Skin::ALL {
            for (avatar, costume) in costumes_worn(skin) {
                assert!(
                    costume.fits(avatar),
                    "{skin:?}: {avatar:?} wears {costume:?}"
                );
            }
        }
    }

    #[test]
    fn halloween_dresses_the_avatars_in_every_one_of_its_costumes() {
        let worn: HashSet<Costume> = costumes_worn(Skin::Halloween).into_values().collect();
        let expected: HashSet<Costume> = [
            Costume::WitchHat,
            Costume::PumpkinHat,
            Costume::Bat,
            Costume::Spider,
        ]
        .into();
        assert_eq!(worn, expected);
    }

    #[test]
    fn christmas_dresses_the_avatars_in_santa_hats_and_holly() {
        let worn: HashSet<Costume> = costumes_worn(Skin::Christmas).into_values().collect();
        assert_eq!(worn, [Costume::SantaHat, Costume::Holly].into());
    }

    #[test]
    fn nothing_hides_the_unicorns_horn_or_the_sharks_fin() {
        let on_the_crown = [
            Costume::WitchHat,
            Costume::PumpkinHat,
            Costume::SantaHat,
            Costume::Bat,
        ];
        for skin in Skin::ALL {
            let worn = costumes_worn(skin);
            for avatar in [Avatar::Unicorn, Avatar::Shark] {
                let costume = worn[&avatar];
                assert!(
                    !on_the_crown.contains(&costume),
                    "{skin:?}: the {avatar:?} wears {costume:?}"
                );
            }
        }
    }

    #[test]
    fn every_costume_is_worn_somewhere() {
        let worn: HashSet<Costume> = Skin::ALL
            .into_iter()
            .flat_map(|skin| costumes_worn(skin).into_values())
            .collect();
        assert_eq!(worn.len(), Costume::ALL.len());
    }

    #[test]
    fn a_season_reads_as_its_two_days() {
        let season = Skin::Christmas.season();
        assert_eq!(season.to_string(), "Dec 1 to Dec 25");
    }

    #[test]
    fn every_skin_is_stored_under_its_name() {
        for skin in Skin::ALL {
            let stored = serde_json::to_string(&skin).unwrap();
            assert_eq!(stored, format!("\"{}\"", skin.name()));
            assert_eq!(serde_json::from_str::<Skin>(&stored).unwrap(), skin);
        }
    }

    #[test]
    fn every_choice_is_stored_in_its_documented_shape() {
        let cases = [
            (SkinChoice::Plain, json!("plain")),
            (SkinChoice::Auto, json!("auto")),
            (
                SkinChoice::Skin(Skin::Halloween),
                json!({"fixed": "halloween"}),
            ),
        ];
        for (choice, stored) in cases {
            assert_eq!(serde_json::to_value(choice).unwrap(), stored);
            assert_eq!(
                serde_json::from_value::<SkinChoice>(stored).unwrap(),
                choice
            );
        }
    }

    #[test]
    fn the_options_offer_plain_automatic_then_each_skin() {
        let options: Vec<SkinChoice> = SkinChoice::options().collect();
        assert_eq!(options.len(), Skin::ALL.len() + 2);
        assert_eq!(options[..2], [SkinChoice::Plain, SkinChoice::Auto]);
        for skin in Skin::ALL {
            assert!(options.contains(&SkinChoice::Skin(skin)), "{skin:?}");
        }
    }
}
