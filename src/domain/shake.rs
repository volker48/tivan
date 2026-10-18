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

#[cfg(test)]
const GRAVITY: f64 = 9.81;

/// How much of the gravity estimate each reading leaves in place. Gravity changes only as slowly
/// as the phone tilts, so a shake's jolts pass straight through the estimate.
const GRAVITY_SMOOTHING: f64 = 0.9;

fn length([x, y, z]: [f64; 3]) -> f64 {
    (x * x + y * y + z * z).sqrt()
}

/// Counts jolts, each the moment the force rises past [`ShakeDetector::JOLT`], and reports a shake
/// once enough of them land inside the window. After a shake it rests, so one long shake shuffles
/// once and not three times.
///
/// Readings that include gravity have it taken out against a slow-moving estimate of where gravity
/// points, so a sideways jolt of a phone held upright counts as fully as one with gravity removed by
/// the phone itself.
#[derive(Clone, Debug, Default)]
pub struct ShakeDetector {
    gravity: Option<[f64; 3]>,
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
        let above = self.force(motion) >= Self::JOLT;
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

    /// How hard the phone is being pushed, gravity aside.
    fn force(&mut self, motion: Motion) -> f64 {
        let reading = [motion.x, motion.y, motion.z];
        if !motion.with_gravity {
            return length(reading);
        }
        let gravity = self.gravity.get_or_insert(reading);
        for (estimate, value) in gravity.iter_mut().zip(reading) {
            *estimate = GRAVITY_SMOOTHING * *estimate + (1.0 - GRAVITY_SMOOTHING) * value;
        }
        let [gx, gy, gz] = *gravity;
        length([motion.x - gx, motion.y - gy, motion.z - gz])
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

    #[test]
    fn a_sideways_shake_of_an_upright_phone_counts_when_gravity_is_in_the_reading() {
        // Gravity along y, the shake along x. Taking 9.81 off the reading's length would leave
        // about 12.5 of a 20 m/s² push, under the jolt threshold.
        let upright = |motion: Motion| Motion {
            y: GRAVITY,
            with_gravity: true,
            ..motion
        };
        let mut detector = ShakeDetector::new();
        let resting = (0..1_000).step_by(16).map(|at| upright(still(at)));
        assert!(shakes(&mut detector, resting).is_empty());
        let shaken = shaking(1_000, 3, 160, 20.0).into_iter().map(upright);
        assert_eq!(shakes(&mut detector, shaken), [1_320]);
    }

    #[test]
    fn tilting_the_phone_slowly_is_not_a_shake() {
        let mut detector = ShakeDetector::new();
        // Gravity swings from y to z over two seconds, as when the phone is laid flat.
        let tilting = (0..2_000).step_by(16).map(|at| {
            let angle = std::f64::consts::FRAC_PI_2 * at as f64 / 2_000.0;
            Motion {
                y: GRAVITY * angle.cos(),
                z: GRAVITY * angle.sin(),
                with_gravity: true,
                ..still(at)
            }
        });
        assert!(shakes(&mut detector, tilting).is_empty());
    }
}
