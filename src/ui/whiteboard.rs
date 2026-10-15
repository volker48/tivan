use dioxus::prelude::*;

use crate::app::{AppState, Mode, use_app, use_escape};
use crate::domain::{Millis, Pick, Preset, Timer, format_mm_ss};
use crate::ui::avatar::AvatarIcon;
use crate::ui::{group_cards, hue};

/// The oracle's latest answer. `seq` keys the overlay so each pick replays the entrance
/// animation, which a reused DOM node would otherwise play once and never again.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Reveal {
    seq: u32,
    pick: Pick,
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
                TimerControls {}
            }
            TimerStrip {}
            GroupWall {}
            OracleReveal { reveal }
            TimesUp {}
        }
    }
}

fn draw(mut state: AppState, mut reveal: Signal<Option<Reveal>>) {
    if let Some(pick) = state.edit_class(|class, rng| class.draw(rng)).flatten() {
        let seq = reveal.peek().as_ref().map_or(0, |current| current.seq + 1);
        reveal.set(Some(Reveal { seq, pick }));
    }
}

#[component]
fn OracleButton(reveal: Signal<Option<Reveal>>) -> Element {
    let state = use_app();
    let (students, left) = state
        .with_class(|class| (class.students().len(), class.oracle_remaining()))
        .unwrap_or_default();
    rsx! {
        button {
            class: "btn btn-hero btn-oracle",
            r#type: "button",
            disabled: students == 0,
            onclick: move |_| draw(state, reveal),
            span { "Ask the oracle" }
            if students > 0 {
                span { class: "sub", "{left} left" }
            }
        }
    }
}

#[component]
fn OracleReveal(reveal: Signal<Option<Reveal>>) -> Element {
    let state = use_app();
    let left = state
        .with_class(|class| class.oracle_remaining())
        .unwrap_or_default();
    // `key` only takes effect on the root of a block, so the overlay is the root of this `if`.
    rsx! {
        if let Some(Reveal { seq, pick }) = reveal() {
            div {
                key: "{seq}",
                class: "overlay",
                onclick: move |_| reveal.set(None),
                div {
                    class: "reveal-card",
                    role: "dialog",
                    aria_label: "The oracle has chosen",
                    onclick: move |event| event.stop_propagation(),
                    p { class: "reveal-line",
                        "The oracle has chosen "
                        strong { class: "reveal-name",
                            AvatarIcon { avatar: pick.avatar }
                            "{pick.name}!"
                        }
                    }
                    p { class: "reveal-left",
                        "{left} left"
                        if left == 0 {
                            ". That is everyone, so the next pick starts a new round."
                        }
                    }
                    div { class: "form-actions",
                        button {
                            class: "btn btn-primary",
                            r#type: "button",
                            onclick: move |_| draw(state, reveal),
                            "Pick again"
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
    }
}

#[component]
fn TimerControls() -> Element {
    let mut state = use_app();
    let timer = state.timer();
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
    /// Text lines to fit in the fullest card: its members plus two for the group name and padding.
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

fn wall_layout(groups: usize, fullest: usize) -> WallLayout {
    let cols = wall_columns(groups);
    WallLayout {
        cols,
        rows: groups.div_ceil(cols).max(1),
        lines: fullest + 2,
    }
}

#[component]
fn GroupWall() -> Element {
    let state = use_app();
    let Some((cards, students)) =
        state.with_class(|class| (group_cards(class), class.students().len()))
    else {
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
    let WallLayout { cols, rows, lines } = wall_layout(cards.len(), fullest);
    rsx! {
        section {
            class: "wall",
            style: "--cols: {cols}; --rows: {rows}; --lines: {lines}",
            for card in cards {
                article { key: "{card.idx.get()}", class: "wall-card hue-{hue(card.idx)}",
                    h2 { class: "wall-name", "{card.name}" }
                    ul { class: "wall-members",
                        for student in card.members {
                            li { key: "{student.id.get()}",
                                AvatarIcon { avatar: student.avatar }
                                "{student.name}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MAX_GROUPS;

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
    fn layout_counts_rows_and_reserves_lines_for_the_fullest_card() {
        assert_eq!(
            wall_layout(4, 5),
            WallLayout {
                cols: 2,
                rows: 2,
                lines: 7
            }
        );
        assert_eq!(
            wall_layout(5, 3),
            WallLayout {
                cols: 3,
                rows: 2,
                lines: 5
            }
        );
    }
}
