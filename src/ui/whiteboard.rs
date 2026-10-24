use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::app::{AppState, Mode, use_app, use_escape};
use crate::domain::{
    Avatar, DEFAULT_MINUTES, Draw, Grouping, MAX_MINUTES, Millis, Pick, Preset, Reel, StudentId,
    Timer, format_mm_ss, parse_minutes,
};
use crate::platform;
use crate::ui::avatar::AvatarIcon;
use crate::ui::{GroupCard, RotateControls, center_slots, group_cards, hue};

/// The oracle's latest answer. `seq` keys the overlay so each pick replays the entrance
/// animation, which a reused DOM node would otherwise play once and never again. `reel` is the
/// spin that runs before the answer is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Reveal {
    seq: u32,
    pick: Pick,
    reel: Reel,
}

/// Under this the countdown turns red.
const LOW_TIME: Millis = Millis(10_000);

#[component]
pub fn WhiteboardMode() -> Element {
    let mut state = use_app();
    let mut reveal = use_signal(|| None::<Reveal>);
    // Escape closes the topmost overlay first, so dismissing a reveal never exposes Build mode to
    // the class.
    use_escape(move || {
        if reveal.peek().is_some() {
            reveal.set(None);
        } else if matches!(state.timer(), Timer::Finished { .. }) {
            state.cancel_timer();
        } else {
            state.set_mode(Mode::Build);
        }
    });
    let class_name = state
        .with_class(|class| class.name().to_string())
        .unwrap_or_default();
    rsx! {
        div { class: "board-screen",
            header { class: "wb-bar",
                button {
                    class: "btn btn-small",
                    r#type: "button",
                    onclick: move |_| state.set_mode(Mode::Build),
                    "Back to Build"
                }
                span { class: "wb-class", "{class_name}" }
                OracleButton { reveal }
                GroupOracleButton { reveal }
                RotateControls {}
                TimerControls {}
            }
            TimerStrip {}
            GroupWall {}
            OracleReveal { reveal }
            TimesUp {}
        }
    }
}

/// Says whether there was anyone to pick.
fn draw(mut state: AppState, mut reveal: Signal<Option<Reveal>>, kind: Draw) -> bool {
    let Some((pick, reel)) = state
        .edit_class(|class, rng| {
            let pick = class.draw(kind, rng)?;
            let reel = Reel::new(class.candidates(kind), &pick, rng);
            Some((pick, reel))
        })
        .flatten()
    else {
        return false;
    };
    let seq = reveal.peek().as_ref().map_or(0, |current| current.seq + 1);
    reveal.set(Some(Reveal { seq, pick, reel }));
    true
}

/// For a pick who is not in the room: marks them absent for the day and picks again, or closes
/// the reveal when no one is left to pick.
fn not_here(mut state: AppState, mut reveal: Signal<Option<Reveal>>, id: StudentId) {
    let today = state.today();
    state.edit_class(|class, _| class.mark_absent(id, today));
    if !draw(state, reveal, Draw::Student) {
        reveal.set(None);
    }
}

#[component]
fn OracleButton(reveal: Signal<Option<Reveal>>) -> Element {
    let state = use_app();
    let (students, left) = state
        .with_class(|class| {
            (
                class.present().count(),
                class.oracle_remaining(Draw::Student),
            )
        })
        .unwrap_or_default();
    rsx! {
        button {
            class: "btn btn-hero btn-oracle",
            r#type: "button",
            disabled: students == 0,
            onclick: move |_| {
                draw(state, reveal, Draw::Student);
            },
            span { "Ask the oracle" }
            if students > 0 {
                span { class: "sub", "{left} left" }
            }
        }
    }
}

/// The quieter way to ask the oracle: pick one of the groups instead of a student. Only for a
/// class that has groups.
#[component]
fn GroupOracleButton(reveal: Signal<Option<Reveal>>) -> Element {
    let state = use_app();
    let Some(left) = state
        .with_class(|class| class.grouping().map(Grouping::remaining))
        .flatten()
    else {
        return rsx! {};
    };
    rsx! {
        button {
            class: "btn btn-oracle btn-oracle-group",
            r#type: "button",
            onclick: move |_| {
                draw(state, reveal, Draw::Group);
            },
            span { "Pick a group" }
            span { class: "sub", "{left} left" }
        }
    }
}

/// The count line under a pick, and once the round is spent, what the next pick does.
fn left_line(kind: Draw, left: usize) -> String {
    let (count, all) = match kind {
        Draw::Student => (format!("{left} left"), "everyone"),
        Draw::Group if left == 1 => ("1 group left".to_owned(), "every group"),
        Draw::Group => (format!("{left} groups left"), "every group"),
    };
    if left == 0 {
        format!("{count}. That is {all}, so the next pick starts a new round.")
    } else {
        count
    }
}

fn reveal_class(pick: &Pick) -> String {
    match pick {
        Pick::Student(_) => "reveal-card".to_owned(),
        Pick::Group { idx, .. } => format!("reveal-card reveal-group hue-{}", hue(*idx)),
    }
}

#[component]
fn OracleReveal(reveal: Signal<Option<Reveal>>) -> Element {
    // `key` only takes effect on the root of a block, so the overlay is the root of this `if`.
    rsx! {
        if let Some(Reveal { seq, pick, reel }) = reveal() {
            div {
                key: "{seq}",
                class: "overlay",
                onclick: move |_| reveal.set(None),
                RevealStage { reveal, pick, reel }
            }
        }
    }
}

/// Where the overlay is: flashing through the candidates, or showing the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Spinning { frame: usize },
    Landed,
}

impl Stage {
    /// Skips the spin when there is nothing to spin through or the teacher asked for less motion.
    fn first(reel: &Reel, reduced_motion: bool) -> Stage {
        if reel.is_empty() || reduced_motion {
            Stage::Landed
        } else {
            Stage::Spinning { frame: 0 }
        }
    }
}

/// The spin and then the card. Each pick mounts a fresh stage under the keyed overlay, so the
/// spin restarts per pick, and dismissing the overlay drops the spin's timer with it.
#[component]
fn RevealStage(reveal: Signal<Option<Reveal>>, pick: Pick, reel: Reel) -> Element {
    let mut stage = use_signal(|| Stage::first(&reel, platform::prefers_reduced_motion()));
    let spin = reel.clone();
    use_future(move || {
        let spin = spin.clone();
        async move {
            if *stage.peek() == Stage::Landed {
                return;
            }
            for (frame, flash) in spin.frames().iter().enumerate() {
                stage.set(Stage::Spinning { frame });
                TimeoutFuture::new(flash.hold.0 as u32).await;
            }
            stage.set(Stage::Landed);
        }
    });
    match stage() {
        Stage::Spinning { frame } => rsx! {
            SpinFace { pick: reel.frames()[frame].pick.clone(), frame }
        },
        Stage::Landed => rsx! { RevealCard { reveal, pick } },
    }
}

/// One flash of the spin. It does not stop clicks, so tapping anywhere mid-spin dismisses the
/// overlay.
#[component]
fn SpinFace(pick: Pick, frame: usize) -> Element {
    // `key` only takes effect on the root of a block, so each name is the root of its `match` arm.
    // A new key remounts the name, which restarts its flash animation.
    rsx! {
        div {
            class: "{reveal_class(&pick)} reveal-spinning",
            aria_hidden: "true",
            match &pick {
                Pick::Student(student) => rsx! {
                    strong { key: "{frame}", class: "reveal-name",
                        span { class: "reveal-avatar",
                            AvatarIcon { avatar: student.avatar }
                        }
                        "{student.name}"
                    }
                },
                Pick::Group { name, .. } => rsx! {
                    strong { key: "{frame}", class: "reveal-name", "{name}" }
                },
            }
        }
    }
}

#[component]
fn RevealCard(reveal: Signal<Option<Reveal>>, pick: Pick) -> Element {
    let state = use_app();
    let kind = pick.kind();
    let left = state
        .with_class(|class| class.oracle_remaining(kind))
        .unwrap_or_default();
    let student = match &pick {
        Pick::Student(student) => Some(student.id),
        Pick::Group { .. } => None,
    };
    rsx! {
        div {
            class: reveal_class(&pick),
            role: "dialog",
            aria_label: "The oracle has chosen",
            onclick: move |event| event.stop_propagation(),
            p { class: "reveal-line",
                "The oracle has chosen "
                match &pick {
                    Pick::Student(student) => rsx! {
                        strong { class: "reveal-name",
                            span { class: "reveal-avatar",
                                Sparks {}
                                AvatarIcon { avatar: student.avatar }
                            }
                            "{student.name}!"
                        }
                    },
                    Pick::Group { name, .. } => rsx! {
                        strong { class: "reveal-name", "{name}!" }
                    },
                }
            }
            if let Pick::Group { members, .. } = &pick {
                ul { class: "wall-members reveal-members",
                    for (n , student) in members.iter().enumerate() {
                        li { key: "{student.id.get()}", style: "--n: {n}",
                            WallAvatar { avatar: student.avatar }
                            span { class: "wall-member-name", "{student.name}" }
                        }
                    }
                }
            }
            p { class: "reveal-left", "{left_line(kind, left)}" }
            div { class: "form-actions",
                button {
                    class: "btn btn-primary",
                    r#type: "button",
                    onclick: move |_| {
                        draw(state, reveal, kind);
                    },
                    "Pick again"
                }
                if let Some(id) = student {
                    button {
                        class: "btn",
                        r#type: "button",
                        onclick: move |_| not_here(state, reveal, id),
                        "Not here today"
                    }
                }
                button {
                    class: "btn",
                    r#type: "button",
                    onclick: move |_| reveal.set(None),
                    "Done"
                }
            }
        }
    }
}

/// How many sparks burst out from behind a chosen student's avatar. The stylesheet aims and colors
/// each one by its place.
const SPARKS: usize = 10;

/// A burst of sparks behind the avatar of the student the oracle chose. Decorative.
#[component]
fn Sparks() -> Element {
    rsx! {
        span { class: "sparks", aria_hidden: "true",
            for n in 0..SPARKS {
                i { key: "{n}" }
            }
        }
    }
}

#[component]
fn TimerControls() -> Element {
    let mut state = use_app();
    let timer = state.timer();
    // Held here rather than in `CustomTimer`, which unmounts while a countdown runs, so the field
    // comes back with the teacher's last entry.
    let minutes = use_signal(|| DEFAULT_MINUTES.to_string());
    rsx! {
        div { class: "timer-controls", role: "group", aria_label: "Timer",
            match timer {
                Timer::Idle | Timer::Finished { .. } => rsx! {
                    span { class: "bar-label", "Timer" }
                    for preset in Preset::ALL {
                        button {
                            class: "btn btn-preset",
                            r#type: "button",
                            aria_label: "Start a {preset.minutes()} minute timer",
                            onclick: move |_| {
                                state.update_timer(|_, now| Timer::start(preset.duration(), now));
                            },
                            "{preset.minutes()} min"
                        }
                    }
                    CustomTimer { minutes }
                },
                Timer::Running { .. } => rsx! {
                    button {
                        class: "btn",
                        r#type: "button",
                        onclick: move |_| state.update_timer(|timer, now| timer.pause(now)),
                        "Pause"
                    }
                },
                Timer::Paused { .. } => rsx! {
                    button {
                        class: "btn btn-primary",
                        r#type: "button",
                        onclick: move |_| state.update_timer(|timer, now| timer.resume(now)),
                        "Resume"
                    }
                },
            }
            if matches!(timer, Timer::Running { .. } | Timer::Paused { .. }) {
                button {
                    class: "btn",
                    r#type: "button",
                    onclick: move |_| state.cancel_timer(),
                    "Cancel"
                }
            }
        }
    }
}

/// A countdown of any whole number of minutes. Enter in the field starts it too.
#[component]
fn CustomTimer(minutes: Signal<String>) -> Element {
    let mut state = use_app();
    let duration = parse_minutes(&minutes.read());
    rsx! {
        form {
            class: "custom-timer",
            onsubmit: move |event| {
                event.prevent_default();
                if let Some(duration) = duration {
                    state.update_timer(|_, now| Timer::start(duration, now));
                }
            },
            input {
                r#type: "number",
                min: "1",
                max: "{MAX_MINUTES}",
                step: "1",
                inputmode: "numeric",
                aria_label: "Minutes",
                aria_invalid: "{duration.is_none()}",
                value: "{minutes}",
                oninput: move |event| minutes.set(event.value()),
            }
            span { "min" }
            button {
                class: "btn btn-preset",
                r#type: "submit",
                disabled: duration.is_none(),
                aria_label: match duration {
                    Some(duration) => format!("Start a {} minute timer", duration.0 / 60_000),
                    None => "Start timer".to_string(),
                },
                "Start"
            }
        }
    }
}

/// The big countdown.
#[component]
fn TimerStrip() -> Element {
    let state = use_app();
    let paused = match state.timer() {
        Timer::Running { .. } => false,
        Timer::Paused { .. } => true,
        Timer::Idle | Timer::Finished { .. } => return rsx! {},
    };
    let remaining = state.remaining();
    rsx! {
        div {
            class: "timer-strip",
            role: "timer",
            "data-paused": "{paused}",
            "data-low": "{!paused && remaining <= LOW_TIME}",
            span { class: "digits", "{format_mm_ss(remaining)}" }
            if paused {
                span { class: "paused-tag", "Paused" }
            }
        }
    }
}

#[component]
fn TimesUp() -> Element {
    let mut state = use_app();
    if !matches!(state.timer(), Timer::Finished { .. }) {
        return rsx! {};
    }
    rsx! {
        div { class: "overlay times-up", role: "alertdialog", aria_label: "Time's up",
            h2 { "Time's up!" }
            button {
                class: "btn btn-hero",
                r#type: "button",
                onclick: move |_| state.cancel_timer(),
                "Dismiss"
            }
        }
    }
}

/// How the wall's grid and type are sized, handed to the stylesheet as custom properties so CSS
/// can fit the biggest group's text to its cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WallLayout {
    cols: usize,
    rows: usize,
    /// Members in the fullest card: its rows when its names sit in one column.
    members: usize,
    /// Its rows when its names sit in two columns.
    pairs: usize,
    /// Text lines in a card besides its members: two for the group name and padding, and two more
    /// when centers are on: the center name above the card and the frame around it.
    lines: usize,
}

/// Columns for a 16:9 wall, chosen by hand for small counts so a row is never left with one
/// orphan card, and by a wide-aspect square root beyond that.
fn wall_columns(groups: usize) -> usize {
    match groups {
        0 => 1,
        1..=3 => groups,
        4 => 2,
        5 | 6 => 3,
        7 | 8 => 4,
        9 => 3,
        10 => 5,
        11 | 12 => 4,
        _ => (groups as f64 * 1.8).sqrt().ceil() as usize,
    }
}

fn wall_layout(groups: usize, fullest: usize, centers: bool) -> WallLayout {
    let cols = wall_columns(groups);
    WallLayout {
        cols,
        rows: groups.div_ceil(cols).max(1),
        members: fullest,
        pairs: fullest.div_ceil(2),
        lines: 2 + 2 * usize::from(centers),
    }
}

#[component]
fn GroupWall() -> Element {
    let state = use_app();
    let Some((cards, centers, students)) = state.with_class(|class| {
        (
            group_cards(class),
            center_slots(class),
            class.students().len(),
        )
    }) else {
        return rsx! {};
    };
    if cards.is_empty() {
        return rsx! {
            div { class: "wall-empty",
                if students == 0 {
                    p { "This class has no students yet." }
                } else {
                    p { "No groups yet." }
                    p { class: "sub", "Go back to Build and press Make groups." }
                }
            }
        };
    }
    let fullest = cards
        .iter()
        .map(|card| card.members.len())
        .max()
        .unwrap_or(0);
    let WallLayout {
        cols,
        rows,
        members,
        pairs,
        lines,
    } = wall_layout(cards.len(), fullest, centers.is_some());
    rsx! {
        section {
            class: "wall",
            style: "--cols: {cols}; --rows: {rows}; --members: {members}; --pairs: {pairs}; --lines: {lines}",
            if let Some(centers) = centers {
                for center in centers {
                    div { key: "{center.idx.get()}", class: "wall-center",
                        p { class: "wall-center-name", "{center.name}" }
                        WallCard { card: cards[center.group.get()].clone() }
                    }
                }
            } else {
                for card in cards {
                    WallCard { key: "{card.idx.get()}", card }
                }
            }
        }
    }
}

#[component]
fn WallCard(card: GroupCard) -> Element {
    rsx! {
        article {
            class: "wall-card hue-{hue(card.idx)}",
            style: "--c: {card.idx.get()}",
            "data-group": "{card.idx.get()}",
            h2 { class: "wall-name", "{card.name}" }
            // The box is what the member tiles measure to size their avatars.
            div { class: "wall-members-box",
                ul { class: "wall-members",
                    for (n , student) in card.members.into_iter().enumerate() {
                        li { key: "{student.id.get()}", style: "--n: {n}",
                            WallAvatar { avatar: student.avatar }
                            span { class: "wall-member-name", "{student.name}" }
                        }
                    }
                }
            }
        }
    }
}

/// A wall member's avatar, breathing inside a box that takes its arrival, its hops and the fidgets
/// avatar_life.js hands out. Both move by transform alone, so neither interrupts the other.
#[component]
fn WallAvatar(avatar: Avatar) -> Element {
    rsx! {
        span { class: "wall-avatar",
            AvatarIcon { avatar }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::roster;
    use crate::domain::{MAX_GROUPS, SplitMix64};

    #[test]
    fn the_spin_runs_only_for_a_reel_with_frames_and_motion_allowed() {
        let pool: Vec<Pick> = roster(5).into_iter().map(Pick::Student).collect();
        let spin = Reel::new(pool.clone(), &pool[2], &mut SplitMix64::seeded(1));
        assert_eq!(Stage::first(&spin, false), Stage::Spinning { frame: 0 });
        assert_eq!(Stage::first(&spin, true), Stage::Landed);
        assert_eq!(Stage::first(&Reel::default(), false), Stage::Landed);
    }

    #[test]
    fn the_count_line_names_the_unit_and_says_what_a_spent_round_does_next() {
        assert_eq!(left_line(Draw::Student, 23), "23 left");
        assert_eq!(left_line(Draw::Student, 1), "1 left");
        assert_eq!(
            left_line(Draw::Student, 0),
            "0 left. That is everyone, so the next pick starts a new round."
        );
        assert_eq!(left_line(Draw::Group, 3), "3 groups left");
        assert_eq!(left_line(Draw::Group, 1), "1 group left");
        assert_eq!(
            left_line(Draw::Group, 0),
            "0 groups left. That is every group, so the next pick starts a new round."
        );
    }

    #[test]
    fn small_group_counts_never_leave_a_row_with_one_orphan() {
        for groups in 1..=12 {
            let cols = wall_columns(groups);
            assert_ne!(groups % cols, 1, "{groups} groups in {cols} columns");
        }
    }

    #[test]
    fn columns_stay_within_the_group_count_up_to_the_cap() {
        for groups in 1..=MAX_GROUPS {
            let cols = wall_columns(groups);
            assert!(
                (1..=groups).contains(&cols),
                "{groups} groups in {cols} columns"
            );
        }
    }

    #[test]
    fn layout_counts_rows_and_the_fullest_cards_members_in_one_and_two_columns() {
        assert_eq!(
            wall_layout(4, 5, false),
            WallLayout {
                cols: 2,
                rows: 2,
                members: 5,
                pairs: 3,
                lines: 2
            }
        );
        assert_eq!(
            wall_layout(5, 4, false),
            WallLayout {
                cols: 3,
                rows: 2,
                members: 4,
                pairs: 2,
                lines: 2
            }
        );
    }

    #[test]
    fn layout_reserves_two_more_lines_for_the_center_frame_when_centers_are_on() {
        let plain = wall_layout(4, 5, false);
        let centered = wall_layout(4, 5, true);
        assert_eq!(
            centered,
            WallLayout {
                lines: plain.lines + 2,
                ..plain
            }
        );
    }
}
