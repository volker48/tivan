//! Telling a deliberate shake of the phone from carrying it around. A shake is several hard jolts
//! close together; walking, setting the phone down or one bump is at most one or two.

use super::timer::Millis;

/// One reading of the phone's motion sensor, in m/s². `with_gravity` says whether the reading
/// includes the 9.8 m/s² of gravity, which some phones cannot separate out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub at: Millis,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub with_gravity: bool,
}

const GRAVITY: f64 = 9.81;

impl Motion {
    /// How hard the phone is being pushed, gravity aside. With gravity in the reading this is only
    /// an estimate, because the phone's tilt is unknown, but a shake dwarfs the error.
    fn force(self) -> f64 {
        let magnitude = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if self.with_gravity {
            (magnitude - GRAVITY).abs()
        } else {
            magnitude
        }
    }
}

/// Counts jolts, each the moment the force rises past [`ShakeDetector::JOLT`], and reports a shake
/// once enough of them land inside the window. After a shake it rests, so one long shake shuffles
/// once and not three times.
#[derive(Clone, Debug, Default)]
pub struct ShakeDetector {
    above: bool,
    jolts: Vec<Millis>,
    resting_until: Option<Millis>,
}

impl ShakeDetector {
    /// About 1.4 g on top of gravity: a flick of the wrist, not a step.
    pub const JOLT: f64 = 14.0;
    pub const JOLTS: usize = 3;
    pub const WINDOW: Millis = Millis(1_000);
    pub const REST: Millis = Millis(1_500);

    pub fn new() -> ShakeDetector {
        ShakeDetector::default()
    }

    /// Takes one reading and says whether it completed a shake.
    pub fn sample(&mut self, motion: Motion) -> bool {
        let now = motion.at;
        let above = motion.force() >= Self::JOLT;
        let rising = above && !self.above;
        self.above = above;
        if self.resting_until.is_some_and(|until| now < until) {
            return false;
        }
        self.resting_until = None;
        self.jolts
            .retain(|&jolt| now.0.saturating_sub(jolt.0) < Self::WINDOW.0);
        if !rising {
            return false;
        }
        self.jolts.push(now);
        if self.jolts.len() < Self::JOLTS {
            return false;
        }
        self.jolts.clear();
        self.resting_until = Some(Millis(now.0 + Self::REST.0));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sixty readings a second, like the phones Tivan runs on.
    const FRAME_MS: u64 = 16;

    fn still(at: u64) -> Motion {
        Motion {
            at: Millis(at),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            with_gravity: false,
        }
    }

    fn pushed(at: u64, force: f64) -> Motion {
        Motion {
            x: force,
            ..still(at)
        }
    }

    /// Feeds `readings` and returns the times a shake was reported.
    fn shakes(
        detector: &mut ShakeDetector,
        readings: impl IntoIterator<Item = Motion>,
    ) -> Vec<u64> {
        readings
            .into_iter()
            .filter(|&motion| detector.sample(motion))
            .map(|motion| motion.at.0)
            .collect()
    }

    /// A wrist shake back and forth: a strong push every `period` ms for `swings` swings, three
    /// frames each, starting at `start`, with stillness between.
    fn shaking(start: u64, swings: u64, period: u64, force: f64) -> Vec<Motion> {
        let end = start + swings * period;
        (start..end)
            .step_by(FRAME_MS as usize)
            .map(|at| {
                let into_swing = (at - start) % period;
                if into_swing < 3 * FRAME_MS {
                    let direction = if ((at - start) / period).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    };
                    pushed(at, direction * force)
                } else {
                    still(at)
                }
            })
            .collect()
    }

    #[test]
    fn holding_the_phone_still_is_not_a_shake() {
        let mut detector = ShakeDetector::new();
        assert!(shakes(&mut detector, (0..5_000).step_by(16).map(still)).is_empty());
    }

    #[test]
    fn a_vigorous_shake_is_reported_once_on_its_third_jolt() {
        let mut detector = ShakeDetector::new();
        assert_eq!(shakes(&mut detector, shaking(0, 3, 160, 20.0)), [320]);
    }

    #[test]
    fn a_long_shake_rests_between_reports() {
        let mut detector = ShakeDetector::new();
        let reported = shakes(&mut detector, shaking(0, 30, 160, 20.0));
        assert!(reported.len() >= 2, "{reported:?}");
        for pair in reported.windows(2) {
            assert!(pair[1] - pair[0] >= ShakeDetector::REST.0, "{reported:?}");
        }
    }

    #[test]
    fn a_single_bump_or_a_gentle_sway_is_not_a_shake() {
        let mut detector = ShakeDetector::new();
        let bump = (0..1_000).step_by(16).map(|at| {
            if (200..250).contains(&at) {
                pushed(at, 30.0)
            } else {
                still(at)
            }
        });
        assert!(shakes(&mut detector, bump).is_empty());
        assert!(shakes(&mut detector, shaking(2_000, 10, 160, 8.0)).is_empty());
    }

    #[test]
    fn jolts_spread_wider_than_the_window_never_add_up() {
        let mut detector = ShakeDetector::new();
        assert!(shakes(&mut detector, shaking(0, 10, 600, 25.0)).is_empty());
    }

    #[test]
    fn a_long_push_counts_as_one_jolt() {
        let mut detector = ShakeDetector::new();
        let held = (0..2_000).step_by(16).map(|at| pushed(at, 25.0));
        assert!(shakes(&mut detector, held).is_empty());
    }

    #[test]
    fn gravity_alone_is_no_force_when_the_reading_includes_it() {
        let mut detector = ShakeDetector::new();
        let resting = (0..3_000).step_by(16).map(|at| Motion {
            z: GRAVITY,
            with_gravity: true,
            ..still(at)
        });
        assert!(shakes(&mut detector, resting).is_empty());
        let shaken = shaking(4_000, 3, 160, 26.0)
            .into_iter()
            .map(|motion| Motion {
                z: GRAVITY,
                with_gravity: true,
                ..motion
            });
        assert_eq!(shakes(&mut detector, shaken), [4_320]);
    }
}
