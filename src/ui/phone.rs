//! The phone's half of the phone remote: the page a teacher's phone opens from the QR code. It
//! never sees the class, only what its buttons can do, so a lost phone gives nothing away.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::domain::ShakeDetector;
use crate::domain::remote::{
    BadToken, BoardStatus, Command, FromBoard, FromRemote, Refusal, Seat, Token, decode,
    socket_path,
};
use crate::platform::{self, LocalStorage, MotionAccess, MotionGuard};
use crate::remote::{LinkState, Outlet, device_key, keep_open};

/// How long the "Shuffled!" confirmation stays up.
const CONFIRM_MS: u32 = 1_400;

fn confirmation(command: Command) -> &'static str {
    match command {
        Command::Shuffle => "Shuffled!",
        Command::Oracle => "The oracle is choosing…",
        Command::Group => "Picking a group…",
        Command::Rotate => "Rotated!",
    }
}

/// What the phone says while it cannot drive the board.
fn waiting_line(link: LinkState) -> &'static str {
    match link {
        LinkState::Connecting => "Connecting…",
        LinkState::Open { peer: false } => {
            "Waiting for the whiteboard. Open Tivan on the classroom screen."
        }
        LinkState::Open { peer: true } => "Connected",
        LinkState::Retrying => "Reconnecting…",
        LinkState::Refused(Refusal::Claimed) => {
            "This whiteboard is paired with another phone. To use this one instead, press Phone \
             remote on the whiteboard, turn the remote off, and scan the new code."
        }
        LinkState::Refused(Refusal::Replaced) => {
            "The remote is open in another tab. Use that one, or reload this page to take over."
        }
        LinkState::Refused(Refusal::Unpaired) => {
            "The remote was turned off on the whiteboard. Press Phone remote there and scan the \
             new code."
        }
    }
}

#[component]
pub fn PhoneRemote(room: Result<Token, BadToken>) -> Element {
    let key = use_hook(|| platform::random_token().map(|fresh| device_key(&LocalStorage, fresh)));
    let content = match (room, key) {
        (Ok(room), Some(key)) => rsx! {
            Controls { room, device: key }
        },
        (Err(BadToken), _) => rsx! {
            p { class: "phone-message",
                "This isn't a working remote link. On the whiteboard, press Phone remote and scan the code again."
            }
        },
        (Ok(_), None) => rsx! {
            p { class: "phone-message", "This browser can't run the remote. Try your phone's main browser." }
        },
    };
    rsx! {
        div { class: "phone-screen",
            header { class: "phone-bar",
                span { class: "brand", "Tivan" }
                span { class: "phone-tag", "remote" }
            }
            {content}
        }
    }
}

#[component]
fn Controls(room: Token, device: Token) -> Element {
    let link = use_signal(|| LinkState::Connecting);
    let mut status = use_signal(BoardStatus::default);
    // Keys the confirmation, so the same command twice replays its entrance.
    let mut confirmed = use_signal(|| None::<(u32, Command)>);
    let mut jiggles = use_signal(|| 0u32);
    let outlet = use_hook(Outlet::default);

    let incoming = outlet.clone();
    use_future(move || {
        let path = socket_path(&room, Seat::Remote, &device);
        let outlet = incoming.clone();
        async move {
            keep_open(path, outlet, link, move |text| {
                match decode::<FromBoard>(text) {
                    Some(FromBoard::Status(next)) => status.set(next),
                    Some(FromBoard::Done { command }) => {
                        platform::buzz();
                        let seq = confirmed.peek().map_or(0, |(seq, _)| seq + 1);
                        confirmed.set(Some((seq, command)));
                    }
                    Some(FromBoard::Unpair) | None => {}
                }
            })
            .await;
        }
    });
    // Clears the confirmation a moment after the newest one, and not after an older one.
    use_effect(move || {
        let Some((seq, _)) = confirmed() else {
            return;
        };
        spawn(async move {
            TimeoutFuture::new(CONFIRM_MS).await;
            if confirmed.peek().is_some_and(|(current, _)| current == seq) {
                confirmed.set(None);
            }
        });
    });

    let sender = outlet.clone();
    let send = use_callback(move |command: Command| {
        if link.peek().peer() && status.peek().allows(command) {
            if command == Command::Shuffle {
                *jiggles.write() += 1;
            }
            sender.send(&FromRemote::Command { command });
        }
    });

    // One listener and one detector for the page's life, so the detector's rest after a shake
    // holds even though the card remounts.
    let mut access = use_signal(platform::motion_access);
    let guard = use_hook(|| Rc::new(RefCell::new(None::<MotionGuard>)));
    use_effect(move || {
        if access() == MotionAccess::Granted && guard.borrow().is_none() {
            let mut detector = ShakeDetector::new();
            let watch = platform::on_motion(move |motion| {
                if detector.sample(motion) {
                    send.call(Command::Shuffle);
                }
            });
            *guard.borrow_mut() = Some(watch);
        }
    });
    let allow_motion = move |_| async move {
        let granted = platform::request_motion().await;
        access.set(if granted {
            MotionAccess::Granted
        } else {
            MotionAccess::Unsupported
        });
    };

    let state = link();
    if !state.peer() {
        let tone = match state {
            LinkState::Refused(_) => "trouble",
            _ => "waiting",
        };
        return rsx! {
            p { class: "phone-message", role: "status", "data-tone": "{tone}", "{waiting_line(state)}" }
        };
    }
    let current = status();
    rsx! {
        p { class: "phone-connected", role: "status",
            span { class: "remote-dot", aria_hidden: "true" }
            "Connected to the whiteboard"
        }
        ShakeCard {
            jiggles: jiggles(),
            access: access(),
            ready: current.allows(Command::Shuffle),
            on_allow: allow_motion,
        }
        div { class: "phone-actions",
            button {
                class: "btn btn-primary phone-btn",
                r#type: "button",
                disabled: !current.allows(Command::Shuffle),
                onclick: move |_| send(Command::Shuffle),
                "Shuffle"
            }
            button {
                class: "btn btn-hero phone-btn",
                r#type: "button",
                disabled: !current.allows(Command::Oracle),
                onclick: move |_| send(Command::Oracle),
                "Ask the oracle"
            }
            button {
                class: "btn phone-btn",
                r#type: "button",
                disabled: !current.allows(Command::Group),
                onclick: move |_| send(Command::Group),
                "Pick a group"
            }
            if current.centers {
                button {
                    class: "btn phone-btn",
                    r#type: "button",
                    onclick: move |_| send(Command::Rotate),
                    "Rotate centers"
                }
            }
        }
        if !current.grouped {
            p { class: "phone-hint", "Make groups on the whiteboard first, then shake to shuffle them." }
        }
        if let Some((seq, command)) = confirmed() {
            p { key: "{seq}", class: "phone-confirm", role: "status", "{confirmation(command)}" }
        }
    }
}

/// The big shake target. Its root is keyed by the shuffle count, so each shuffle remounts it and
/// replays the jiggle.
#[component]
fn ShakeCard(
    jiggles: u32,
    access: MotionAccess,
    ready: bool,
    on_allow: EventHandler<MouseEvent>,
) -> Element {
    let hint = match access {
        MotionAccess::Granted if ready => "Shake to shuffle",
        MotionAccess::Granted => "Shake to shuffle, once there are groups",
        MotionAccess::Ask => "Shaking needs your OK first",
        MotionAccess::Unsupported => "Tap Shuffle to mix the groups",
    };
    rsx! {
        div { key: "{jiggles}", class: "shake-card", "data-ready": "{ready}",
            svg {
                class: "shake-phone",
                view_box: "0 0 64 64",
                "aria-hidden": "true",
                "focusable": "false",
                path { class: "shake-lines", d: "M8 22 l-4 10 l4 10 M56 22 l4 10 l-4 10" }
                rect {
                    x: "18",
                    y: "8",
                    width: "28",
                    height: "48",
                    rx: "6",
                }
                rect {
                    class: "shake-glass",
                    x: "22",
                    y: "14",
                    width: "20",
                    height: "34",
                    rx: "2",
                }
            }
            p { class: "shake-hint", "{hint}" }
            if access == MotionAccess::Ask {
                button {
                    class: "btn btn-hero",
                    r#type: "button",
                    onclick: move |event| on_allow.call(event),
                    "Turn on shake"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_confirmation_and_every_state_a_line() {
        for command in [
            Command::Shuffle,
            Command::Oracle,
            Command::Group,
            Command::Rotate,
        ] {
            assert!(!confirmation(command).is_empty());
        }
        for refusal in [Refusal::Claimed, Refusal::Replaced, Refusal::Unpaired] {
            assert!(waiting_line(LinkState::Refused(refusal)).len() > 20);
        }
        assert_eq!(waiting_line(LinkState::Retrying), "Reconnecting…");
    }
}
