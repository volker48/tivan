//! Countdown as a state machine over an injected clock. `Running` stores a deadline rather than a
//! decrementing counter, so a late or missed tick cannot drift the display and pause/resume are
//! pure transitions. Never persisted.

/// Milliseconds on the shell's clock (epoch ms in production, any origin in tests).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Millis(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    One,
    Three,
    Five,
}

impl Preset {
    /// The one list the UI renders buttons from.
    pub const ALL: [Preset; 3] = [Preset::One, Preset::Three, Preset::Five];

    pub fn minutes(self) -> u64 {
        match self {
            Preset::One => 1,
            Preset::Three => 3,
            Preset::Five => 5,
        }
    }

    pub fn duration(self) -> Millis {
        Millis(self.minutes() * 60_000)
    }
}

/// The longest countdown the minutes field accepts: a long block period, and short enough that the
/// digits stay on one line.
pub const MAX_MINUTES: u64 = 180;

/// What the minutes field starts at.
pub const DEFAULT_MINUTES: u64 = 10;

/// Reads the minutes field: a whole number from 1 to [`MAX_MINUTES`], with surrounding space
/// ignored. Anything else, including the empty text a number field reports mid-edit, is `None`.
pub fn parse_minutes(text: &str) -> Option<Millis> {
    let text = text.trim();
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let minutes: u64 = text.parse().ok()?;
    (1..=MAX_MINUTES)
        .contains(&minutes)
        .then_some(Millis(minutes * 60_000))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Timer {
    #[default]
    Idle,
    Running {
        deadline: Millis,
        total: Millis,
    },
    Paused {
        remaining: Millis,
        total: Millis,
    },
    /// Reached zero. Stays until cancelled or restarted so the UI can flash "Time's up!".
    Finished {
        total: Millis,
    },
}

impl Timer {
    /// Transitions are total: one that does not apply to the current state returns it unchanged,
    /// so double clicks and late ticks are harmless.
    pub fn start(total: Millis, now: Millis) -> Timer {
        Timer::Running {
            deadline: Millis(now.0.saturating_add(total.0)),
            total,
        }
    }

    pub fn pause(self, now: Millis) -> Timer {
        match self.tick(now) {
            Timer::Running { deadline, total } => Timer::Paused {
                remaining: Millis(deadline.0 - now.0),
                total,
            },
            other => other,
        }
    }

    pub fn resume(self, now: Millis) -> Timer {
        match self {
            Timer::Paused { remaining, total } => Timer::Running {
                deadline: Millis(now.0.saturating_add(remaining.0)),
                total,
            },
            other => other,
        }
    }

    pub fn cancel(self) -> Timer {
        Timer::Idle
    }

    /// `Running` at or past its deadline becomes `Finished`. The shell calls this on every tick.
    pub fn tick(self, now: Millis) -> Timer {
        match self {
            Timer::Running { deadline, total } if now >= deadline => Timer::Finished { total },
            other => other,
        }
    }

    /// Time left, clamped at zero. `Idle` and `Finished` have none.
    pub fn remaining(&self, now: Millis) -> Millis {
        match *self {
            Timer::Running { deadline, .. } => Millis(deadline.0.saturating_sub(now.0)),
            Timer::Paused { remaining, .. } => remaining,
            Timer::Idle | Timer::Finished { .. } => Millis(0),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Timer::Running { .. })
    }
}

/// "M:SS", rounding up so the display reads 0:01 until the instant of zero.
pub fn format_mm_ss(remaining: Millis) -> String {
    let seconds = remaining.0.div_ceil(1000);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    fn running(deadline: u64, total: u64) -> Timer {
        Timer::Running {
            deadline: Millis(deadline),
            total: Millis(total),
        }
    }

    fn paused(remaining: u64, total: u64) -> Timer {
        Timer::Paused {
            remaining: Millis(remaining),
            total: Millis(total),
        }
    }

    fn finished(total: u64) -> Timer {
        Timer::Finished {
            total: Millis(total),
        }
    }

    #[test]
    fn presets_cover_one_three_five_minutes() {
        let minutes: Vec<u64> = Preset::ALL.iter().map(|p| p.minutes()).collect();
        assert_eq!(minutes, [1, 3, 5]);
        assert_eq!(Preset::Three.duration(), Millis(3 * MIN));
    }

    #[test]
    fn minutes_field_takes_any_whole_number_from_one_to_the_max() {
        assert_eq!(parse_minutes("1"), Some(Millis(MIN)));
        assert_eq!(parse_minutes("10"), Some(Millis(10 * MIN)));
        assert_eq!(parse_minutes("47"), Some(Millis(47 * MIN)));
        assert_eq!(parse_minutes(" 10 "), Some(Millis(10 * MIN)));
        assert_eq!(parse_minutes("007"), Some(Millis(7 * MIN)));
        assert_eq!(
            parse_minutes(&MAX_MINUTES.to_string()),
            Some(Millis(MAX_MINUTES * MIN))
        );
        assert_eq!(
            parse_minutes(&DEFAULT_MINUTES.to_string()),
            Some(Millis(10 * MIN))
        );
    }

    #[test]
    fn minutes_field_rejects_zero_overflow_and_non_whole_numbers() {
        let max_plus_one = (MAX_MINUTES + 1).to_string();
        for text in [
            "",
            " ",
            "0",
            &max_plus_one,
            "99999999999999999999",
            "abc",
            "1.5",
            "-3",
            "+3",
            "1e2",
        ] {
            assert_eq!(parse_minutes(text), None, "{text:?}");
        }
    }

    #[test]
    fn start_counts_down_from_the_full_duration() {
        let timer = Timer::start(Preset::Five.duration(), Millis(1_000));
        assert_eq!(timer, running(1_000 + 5 * MIN, 5 * MIN));
        assert_eq!(timer.remaining(Millis(1_000)), Millis(5 * MIN));
    }

    #[test]
    fn pause_only_applies_to_a_running_timer() {
        let now = Millis(10_000);
        assert_eq!(running(70_000, MIN).pause(now), paused(60_000, MIN));
        assert_eq!(Timer::Idle.pause(now), Timer::Idle);
        assert_eq!(paused(5, MIN).pause(now), paused(5, MIN));
        assert_eq!(finished(MIN).pause(now), finished(MIN));
    }

    #[test]
    fn pausing_twice_equals_pausing_once() {
        let once = running(70_000, MIN).pause(Millis(10_000));
        assert_eq!(once.pause(Millis(20_000)), once);
    }

    #[test]
    fn pause_after_the_deadline_finishes_instead_of_pausing_at_zero() {
        assert_eq!(running(70_000, MIN).pause(Millis(70_000)), finished(MIN));
        assert_eq!(running(70_000, MIN).pause(Millis(99_999)), finished(MIN));
    }

    #[test]
    fn resume_only_applies_to_a_paused_timer() {
        let now = Millis(50_000);
        assert_eq!(paused(30_000, MIN).resume(now), running(80_000, MIN));
        assert_eq!(Timer::Idle.resume(now), Timer::Idle);
        assert_eq!(running(70_000, MIN).resume(now), running(70_000, MIN));
        assert_eq!(finished(MIN).resume(now), finished(MIN));
    }

    #[test]
    fn pause_then_resume_preserves_remaining_time() {
        let timer = Timer::start(Millis(3 * MIN), Millis(1_000));
        let paused = timer.pause(Millis(61_000));
        assert_eq!(paused.remaining(Millis(500_000)), Millis(2 * MIN));
        let resumed = paused.resume(Millis(500_000));
        assert_eq!(resumed.remaining(Millis(500_000)), Millis(2 * MIN));
        assert_eq!(resumed.remaining(Millis(560_000)), Millis(MIN));
    }

    #[test]
    fn cancel_returns_to_idle_from_every_state() {
        for state in [Timer::Idle, running(9, 9), paused(9, 9), finished(9)] {
            assert_eq!(state.cancel(), Timer::Idle);
        }
    }

    #[test]
    fn tick_finishes_exactly_at_the_deadline() {
        let timer = running(70_000, MIN);
        assert_eq!(timer.tick(Millis(69_999)), timer);
        assert_eq!(timer.tick(Millis(70_000)), finished(MIN));
    }

    #[test]
    fn a_late_tick_finishes_with_nothing_remaining() {
        let late = running(70_000, MIN).tick(Millis(10_000_000));
        assert_eq!(late, finished(MIN));
        assert_eq!(late.remaining(Millis(10_000_000)), Millis(0));
    }

    #[test]
    fn tick_is_idempotent_and_leaves_other_states_alone() {
        let now = Millis(100_000);
        let done = running(70_000, MIN).tick(now);
        assert_eq!(done.tick(now), done);
        assert_eq!(Timer::Idle.tick(now), Timer::Idle);
        assert_eq!(paused(5, MIN).tick(now), paused(5, MIN));
    }

    #[test]
    fn remaining_counts_down_and_clamps_at_zero() {
        let timer = running(70_000, MIN);
        assert_eq!(timer.remaining(Millis(10_000)), Millis(60_000));
        assert_eq!(timer.remaining(Millis(70_000)), Millis(0));
        assert_eq!(timer.remaining(Millis(80_000)), Millis(0));
        assert_eq!(Timer::Idle.remaining(Millis(1)), Millis(0));
        assert_eq!(finished(MIN).remaining(Millis(1)), Millis(0));
    }

    #[test]
    fn only_a_running_timer_reports_running() {
        assert!(running(1, 1).is_running());
        for state in [Timer::Idle, paused(1, 1), finished(1)] {
            assert!(!state.is_running());
        }
    }

    #[test]
    fn display_rounds_up_so_it_reads_0_01_until_the_instant_of_zero() {
        let cases = [
            (0, "0:00"),
            (1, "0:01"),
            (999, "0:01"),
            (1_000, "0:01"),
            (1_001, "0:02"),
            (59_000, "0:59"),
            (59_001, "1:00"),
            (60_000, "1:00"),
            (5 * MIN, "5:00"),
            (10 * MIN, "10:00"),
        ];
        for (ms, expected) in cases {
            assert_eq!(format_mm_ss(Millis(ms)), expected, "{ms} ms");
        }
    }
}
