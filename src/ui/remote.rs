//! The board's half of the phone remote: the button and QR code that pair a phone, and the link
//! that carries the phone's commands to the whiteboard.

use std::fmt::Write;

use dioxus::prelude::*;
use qrcodegen::{QrCode, QrCodeEcc};

use crate::app::{Mode, use_app};
use crate::domain::Class;
use crate::domain::remote::{
    BoardStatus, Command, FromBoard, FromRemote, Pairing, Refusal, Seat, decode, remote_link,
    socket_path,
};
use crate::platform::{self, LocalStorage};
use crate::remote::{LinkState, Outlet, keep_open, load_pairing, save_pairing};

/// The board's remote, provided through context by the app. Commands wait in `pending` until the
/// whiteboard, which owns the oracle's overlay, carries them out.
#[derive(Clone, Copy)]
pub struct BoardRemote {
    pairing: Signal<Option<Pairing>>,
    link: Signal<LinkState>,
    pending: Signal<Vec<Command>>,
    outlet: CopyValue<Outlet>,
}

impl BoardRemote {
    /// Picks up the pairing this browser kept, so a phone paired yesterday works today.
    pub fn load() -> BoardRemote {
        BoardRemote {
            pairing: Signal::new(load_pairing(&LocalStorage)),
            link: Signal::new(LinkState::Connecting),
            pending: Signal::new(Vec::new()),
            outlet: CopyValue::new(Outlet::default()),
        }
    }

    pub fn pairing(&self) -> Option<Pairing> {
        self.pairing.read().clone()
    }

    pub fn link(&self) -> LinkState {
        *self.link.read()
    }

    /// Makes a pairing if there is none. False only when the browser cannot make a random code.
    pub fn start(&mut self) -> bool {
        if self.pairing.peek().is_some() {
            return true;
        }
        let (Some(room), Some(key)) = (platform::random_token(), platform::random_token()) else {
            return false;
        };
        let pairing = Pairing { room, key };
        // Unsaved, the remote still works until the page reloads.
        if let Err(error) = save_pairing(&LocalStorage, Some(&pairing)) {
            dioxus::logger::tracing::warn!("could not keep the remote's pairing: {error}");
        }
        self.link.set(LinkState::Connecting);
        self.pairing.set(Some(pairing));
        true
    }

    /// Turns the room off at the relay, which tells the phone, and forgets it here.
    pub fn turn_off(&mut self) {
        let outlet = self.outlet.read();
        outlet.send(&FromBoard::Unpair);
        // The link's loop ends with its component, but the socket lives in the shared outlet.
        outlet.close();
        drop(outlet);
        if let Err(error) = save_pairing(&LocalStorage, None) {
            dioxus::logger::tracing::warn!("could not forget the remote's pairing: {error}");
        }
        self.pairing.set(None);
        self.link.set(LinkState::Connecting);
    }

    /// Reactive: the calling effect runs again when a command arrives. Takes the commands
    /// waiting, oldest first.
    pub fn take_commands(&mut self) -> Vec<Command> {
        if self.pending.read().is_empty() {
            return Vec::new();
        }
        std::mem::take(&mut *self.pending.write())
    }
}

pub fn use_board_remote() -> BoardRemote {
    use_context::<BoardRemote>()
}

/// What the phone's buttons can do with this class.
pub fn board_status(class: &Class) -> BoardStatus {
    let grouping = class.grouping();
    BoardStatus {
        students: !class.students().is_empty(),
        grouped: grouping.is_some(),
        centers: grouping.and_then(|grouping| grouping.centers()).is_some(),
    }
}

/// Keeps the board connected while it has a pairing. Renders nothing.
#[component]
pub fn RemoteLink() -> Element {
    let remote = use_board_remote();
    match remote.pairing() {
        Some(pairing) => rsx! {
            BoardLink { key: "{pairing.room.as_str()}", pairing }
        },
        None => rsx! {},
    }
}

/// One room's connection. Keyed by the room, so a new pairing starts a fresh one and turning the
/// remote off drops the socket with the component.
#[component]
fn BoardLink(pairing: Pairing) -> Element {
    let remote = use_board_remote();
    let state = use_app();
    let status = use_memo(move || state.with_class(board_status).unwrap_or_default());
    // The phone learns what its buttons can do as soon as it connects, and again on every change.
    use_effect(move || {
        let status = status();
        if remote.link().peer() {
            remote.outlet.read().send(&FromBoard::Status(status));
        }
    });
    use_future(move || {
        let path = socket_path(&pairing.room, Seat::Board, &pairing.key);
        let outlet = remote.outlet.read().clone();
        let mut remote = remote;
        let mut state = state;
        async move {
            let replies = outlet.clone();
            keep_open(path, outlet, remote.link, move |text| {
                let Some(FromRemote::Command { command }) = decode(text) else {
                    return;
                };
                if !status.peek().allows(command) {
                    return;
                }
                replies.send(&FromBoard::Done { command });
                if state.mode() != Mode::Whiteboard {
                    state.set_mode(Mode::Whiteboard);
                }
                remote.pending.write().push(command);
            })
            .await;
        }
    });
    rsx! {}
}

/// What the panel says about the connection.
fn status_line(link: LinkState) -> &'static str {
    match link {
        LinkState::Connecting => "Connecting to Tivan…",
        LinkState::Open { peer: false } => "Waiting for your phone…",
        LinkState::Open { peer: true } => "Phone connected. Shake it to shuffle the groups!",
        LinkState::Retrying => "Can't reach Tivan right now. Trying again…",
        LinkState::Refused(Refusal::Claimed) => {
            "Another screen holds this remote. Turn it off and start again."
        }
        LinkState::Refused(Refusal::Replaced) => {
            "This remote is open in another tab or window. Use or turn it off there."
        }
        LinkState::Refused(Refusal::Unpaired) => {
            "This remote was turned off. Turn it off here and start again."
        }
    }
}

fn link_tone(link: LinkState) -> &'static str {
    match link {
        LinkState::Open { peer: true } => "on",
        LinkState::Connecting | LinkState::Open { peer: false } => "waiting",
        LinkState::Retrying | LinkState::Refused(_) => "trouble",
    }
}

/// Opens the panel, pairing first if this board has never paired.
#[component]
pub fn RemoteButton(panel: Signal<bool>) -> Element {
    let mut remote = use_board_remote();
    let paired = remote.pairing().is_some();
    let link = remote.link();
    let tone = if paired { link_tone(link) } else { "off" };
    rsx! {
        button {
            class: "btn btn-remote",
            r#type: "button",
            "data-tone": "{tone}",
            aria_haspopup: "dialog",
            onclick: move |_| {
                if remote.start() {
                    panel.set(true);
                }
            },
            span { class: "remote-dot", aria_hidden: "true" }
            if link.peer() {
                "Remote on"
            } else {
                "Phone remote"
            }
        }
    }
}

#[component]
pub fn RemotePanel(panel: Signal<bool>) -> Element {
    let mut remote = use_board_remote();
    if !panel() {
        return rsx! {};
    }
    let Some(pairing) = remote.pairing() else {
        return rsx! {};
    };
    let link = remote_link(&platform::origin().unwrap_or_default(), &pairing.room);
    let state = remote.link();
    rsx! {
        div { class: "overlay", onclick: move |_| panel.set(false),
            div {
                class: "remote-panel",
                role: "dialog",
                aria_label: "Phone remote",
                onclick: move |event| event.stop_propagation(),
                h2 { class: "remote-title", "Shake your phone to shuffle" }
                div { class: "remote-body",
                    QrImage { text: link.clone() }
                    div { class: "remote-steps",
                        ol {
                            li { "Scan this code with your phone's camera." }
                            li { "Shake the phone. The groups shuffle up here." }
                            li { "Tap its buttons to ask the oracle or pick a group." }
                        }
                        p {
                            class: "remote-status",
                            role: "status",
                            "data-tone": "{link_tone(state)}",
                            "{status_line(state)}"
                        }
                        p { class: "remote-link",
                            "No camera? Open "
                            code { "{link}" }
                        }
                    }
                }
                div { class: "form-actions",
                    button {
                        class: "btn btn-primary",
                        r#type: "button",
                        onclick: move |_| panel.set(false),
                        "Done"
                    }
                    // A replaced tab has no socket to tell the relay, and the tab that replaced
                    // it shares this pairing, so turning off belongs to that tab.
                    if state != LinkState::Refused(Refusal::Replaced) {
                        button {
                            class: "btn",
                            r#type: "button",
                            onclick: move |_| {
                                remote.turn_off();
                                panel.set(false);
                            },
                            "Turn off remote"
                        }
                    }
                }
            }
        }
    }
}

/// Margin of light modules around the code that scanners need to find its edges.
const QUIET_ZONE: i32 = 4;

/// The code's side in modules, quiet zone included, and an SVG path of its dark modules. `None`
/// only for text longer than a QR code holds, which a remote link never is.
fn qr_path(text: &str) -> Option<(i32, String)> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).ok()?;
    let size = qr.size();
    let mut path = String::new();
    for y in 0..size {
        for x in 0..size {
            if qr.get_module(x, y) {
                let _ = write!(path, "M{},{}h1v1h-1z", x + QUIET_ZONE, y + QUIET_ZONE);
            }
        }
    }
    Some((size + 2 * QUIET_ZONE, path))
}

#[component]
fn QrImage(text: String) -> Element {
    let Some((side, path)) = qr_path(&text) else {
        return rsx! {};
    };
    rsx! {
        svg {
            class: "qr",
            view_box: "0 0 {side} {side}",
            role: "img",
            "aria-label": "QR code for the phone remote",
            shape_rendering: "crispEdges",
            rect { width: "{side}", height: "{side}", fill: "#fff" }
            path { d: "{path}", fill: "#2a1f33" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::remote::{TOKEN_BYTES, Token};
    use crate::domain::testing::{label, numbered};
    use crate::domain::{Doc, GroupCount, SplitMix64};

    #[test]
    fn a_remote_link_makes_a_square_code_with_its_quiet_zone() {
        let room = Token::from_bytes([9; TOKEN_BYTES]);
        let link = remote_link("https://tivan.example.org", &room);
        let (side, path) = qr_path(&link).unwrap();
        // Version 4 (33 modules) holds a link this long at medium correction.
        assert_eq!(side, 33 + 2 * QUIET_ZONE);
        // The top-left finder pattern starts right inside the quiet zone.
        assert!(path.starts_with(&format!("M{QUIET_ZONE},{QUIET_ZONE}h1v1h-1z")));
        assert!(!path.contains(&format!("M{},", side - 1)));
    }

    #[test]
    fn the_status_follows_the_class() {
        let mut rng = SplitMix64::seeded(1);
        let mut doc = Doc::default();
        doc.create_class(label("Empty"), Vec::new(), &mut rng);
        assert_eq!(
            board_status(doc.selected().unwrap()),
            BoardStatus::default()
        );
        doc.create_class(label("Test"), numbered(6), &mut rng);
        let class = doc.selected_mut().unwrap();
        assert_eq!(
            board_status(class),
            BoardStatus {
                students: true,
                grouped: false,
                centers: false
            }
        );
        class.regroup(GroupCount::new(2, 6).unwrap(), &mut rng);
        assert!(board_status(class).grouped);
        assert!(!board_status(class).centers);
        class.add_centers();
        assert!(board_status(class).centers);
    }

    #[test]
    fn every_link_state_has_words_and_a_tone() {
        let states = [
            LinkState::Connecting,
            LinkState::Open { peer: false },
            LinkState::Open { peer: true },
            LinkState::Retrying,
            LinkState::Refused(Refusal::Claimed),
            LinkState::Refused(Refusal::Replaced),
            LinkState::Refused(Refusal::Unpaired),
        ];
        for state in states {
            assert!(!status_line(state).is_empty());
            assert!(["on", "waiting", "trouble"].contains(&link_tone(state)));
        }
        assert_eq!(link_tone(LinkState::Open { peer: true }), "on");
    }
}
