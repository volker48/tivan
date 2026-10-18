//! The phone remote's shell: what each end keeps in this browser, and the loop that keeps a
//! socket to the relay open. The rules and the wire format are in
//! [`domain::remote`](crate::domain::remote).

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::logger::tracing::warn;
use dioxus::prelude::*;
use futures_channel::mpsc;
use futures_util::StreamExt;
use gloo_timers::future::TimeoutFuture;
use serde::Serialize;

use crate::domain::remote::{Backoff, Pairing, Refusal, Token, encode, presence};
use crate::platform::{Socket, SocketEvent};
use crate::store::{Storage, StorageError};

/// The board's pairing, as JSON, or `null` once the remote is turned off. Kept apart from the
/// saved doc, so it never travels to the teacher's account: a pairing belongs to one screen.
pub const PAIRING_KEY: &str = "tivan.remote";

/// The phone's device key, which proves to the relay that this is the phone that paired.
pub const DEVICE_KEY: &str = "tivan.remote.device";

/// `None` when nothing is stored or what is stored cannot be read, which only means pairing again.
pub fn load_pairing(storage: &impl Storage) -> Option<Pairing> {
    let stored = match storage.get(PAIRING_KEY) {
        Ok(stored) => stored?,
        Err(error) => {
            warn!("could not read the remote's pairing: {error}");
            return None;
        }
    };
    serde_json::from_str::<Option<Pairing>>(&stored).unwrap_or_else(|error| {
        warn!("ignoring an unreadable remote pairing: {error}");
        None
    })
}

pub fn save_pairing(storage: &impl Storage, pairing: Option<&Pairing>) -> Result<(), StorageError> {
    let json = serde_json::to_string(&pairing).expect("a pairing always serializes");
    storage.set(PAIRING_KEY, &json)
}

/// The stored key, or `fresh` saved for next time. When storage is blocked the key lasts this
/// page only, so reloading the page means pairing again.
pub fn device_key(storage: &impl Storage, fresh: Token) -> Token {
    match storage.get(DEVICE_KEY) {
        Ok(Some(stored)) => {
            if let Ok(key) = Token::parse(&stored) {
                return key;
            }
        }
        Ok(None) => {}
        Err(error) => warn!("could not read the remote's device key: {error}"),
    }
    if let Err(error) = storage.set(DEVICE_KEY, fresh.as_str()) {
        warn!("could not keep the remote's device key: {error}");
    }
    fresh
}

/// Where a connection to the relay stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// The first attempt has not opened yet.
    Connecting,
    /// Connected to the relay. `peer` says whether the other end is too.
    Open { peer: bool },
    /// The connection dropped, and the next attempt is waiting or under way.
    Retrying,
    /// The relay refused this device for good. Nothing more is tried.
    Refused(Refusal),
}

impl LinkState {
    pub fn peer(self) -> bool {
        self == LinkState::Open { peer: true }
    }
}

/// The current socket, if any, shared by the loop that replaces it and the code that sends on it.
#[derive(Clone, Default)]
pub struct Outlet(Rc<RefCell<Option<Socket>>>);

impl Outlet {
    /// Dropped when nothing is connected. See [`Socket::send`].
    pub fn send(&self, message: &impl Serialize) {
        if let Some(socket) = self.0.borrow().as_ref() {
            socket.send(&encode(message));
        }
    }

    /// Closes the current socket after anything already sent has gone out.
    pub fn close(&self) {
        self.set(None);
    }

    fn set(&self, socket: Option<Socket>) {
        // Out of the cell before it drops, so a close handler that runs inside the drop cannot
        // find the cell borrowed.
        let old = self.0.replace(socket);
        drop(old);
    }
}

impl PartialEq for Outlet {
    fn eq(&self, other: &Outlet) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Keeps a socket to `path` open, reconnecting after drops, until the relay refuses it or the
/// future is dropped. The relay's presence reports go into `state`; every other message goes to
/// `on_message`.
pub async fn keep_open(
    path: String,
    outlet: Outlet,
    mut state: Signal<LinkState>,
    mut on_message: impl FnMut(&str),
) {
    let mut backoff = Backoff::new();
    loop {
        let (sender, mut events) = mpsc::unbounded();
        match Socket::open(&path, sender) {
            Ok(socket) => outlet.set(Some(socket)),
            Err(error) => warn!("{error}"),
        }
        let mut closed_with = None;
        while let Some(event) = events.next().await {
            match event {
                SocketEvent::Opened => {
                    backoff.reset();
                    state.set(LinkState::Open { peer: false });
                }
                SocketEvent::Received(text) => match presence(&text) {
                    Some(peer) => state.set(LinkState::Open { peer }),
                    None => on_message(&text),
                },
                SocketEvent::Closed { code } => {
                    closed_with = Some(code);
                    break;
                }
            }
        }
        outlet.set(None);
        if let Some(refusal) = closed_with.and_then(Refusal::from_close_code) {
            state.set(LinkState::Refused(refusal));
            return;
        }
        state.set(LinkState::Retrying);
        TimeoutFuture::new(backoff.next_delay().0 as u32).await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::domain::remote::TOKEN_BYTES;

    #[derive(Default)]
    struct Memory {
        values: RefCell<HashMap<String, String>>,
        blocked: bool,
    }

    impl Storage for Memory {
        fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
            if self.blocked {
                return Err(StorageError::new("blocked"));
            }
            Ok(self.values.borrow().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
            if self.blocked {
                return Err(StorageError::new("blocked"));
            }
            self.values
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
    }

    fn token(byte: u8) -> Token {
        Token::from_bytes([byte; TOKEN_BYTES])
    }

    fn pairing() -> Pairing {
        Pairing {
            room: token(1),
            key: token(2),
        }
    }

    #[test]
    fn a_saved_pairing_loads_back_and_turning_it_off_forgets_it() {
        let storage = Memory::default();
        assert_eq!(load_pairing(&storage), None);
        save_pairing(&storage, Some(&pairing())).unwrap();
        assert_eq!(load_pairing(&storage), Some(pairing()));
        save_pairing(&storage, None).unwrap();
        assert_eq!(load_pairing(&storage), None);
    }

    #[test]
    fn an_unreadable_pairing_or_blocked_storage_means_pairing_again() {
        let storage = Memory::default();
        storage.set(PAIRING_KEY, "{not json").unwrap();
        assert_eq!(load_pairing(&storage), None);
        storage
            .set(PAIRING_KEY, r#"{"room":"short","key":"short"}"#)
            .unwrap();
        assert_eq!(load_pairing(&storage), None);
        let blocked = Memory {
            blocked: true,
            ..Memory::default()
        };
        assert_eq!(load_pairing(&blocked), None);
        assert!(save_pairing(&blocked, Some(&pairing())).is_err());
    }

    #[test]
    fn the_device_key_is_made_once_and_kept() {
        let storage = Memory::default();
        assert_eq!(device_key(&storage, token(3)), token(3));
        assert_eq!(device_key(&storage, token(4)), token(3));
    }

    #[test]
    fn a_damaged_device_key_is_replaced_and_blocked_storage_still_gives_one() {
        let storage = Memory::default();
        storage.set(DEVICE_KEY, "garbage").unwrap();
        assert_eq!(device_key(&storage, token(5)), token(5));
        assert_eq!(device_key(&storage, token(6)), token(5));
        let blocked = Memory {
            blocked: true,
            ..Memory::default()
        };
        assert_eq!(device_key(&blocked, token(7)), token(7));
    }

    #[test]
    fn only_an_open_link_with_the_other_end_there_has_a_peer() {
        assert!(LinkState::Open { peer: true }.peer());
        assert!(!LinkState::Open { peer: false }.peer());
        assert!(!LinkState::Connecting.peer());
        assert!(!LinkState::Retrying.peer());
        assert!(!LinkState::Refused(Refusal::Claimed).peer());
    }
}
