//! The phone remote: a teacher's phone drives the whiteboard through a relay room both of them
//! join. This is the wire format and the rules both ends share. The board sends only what the
//! phone's buttons need, never a student's name.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::timer::Millis;

/// Bytes of randomness in a [`Token`]. 128 bits cannot be guessed by trying.
pub const TOKEN_BYTES: usize = 16;

/// A room code or a device key: [`TOKEN_BYTES`] random bytes in unpadded base64url, which the
/// relay checks for the same shape.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Token(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadToken;

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

impl Token {
    const CHARS: usize = (TOKEN_BYTES * 8).div_ceil(6);

    pub fn parse(raw: &str) -> Result<Token, BadToken> {
        let valid = raw.len() == Self::CHARS && raw.bytes().all(|byte| BASE64URL.contains(&byte));
        if valid {
            Ok(Token(raw.to_owned()))
        } else {
            Err(BadToken)
        }
    }

    /// Encodes random bytes from the platform's cryptographic source.
    pub fn from_bytes(bytes: [u8; TOKEN_BYTES]) -> Token {
        let mut bits = 0u32;
        let mut held = 0;
        let mut out = String::with_capacity(Self::CHARS);
        for byte in bytes {
            bits = (bits << 8) | u32::from(byte);
            held += 8;
            while held >= 6 {
                held -= 6;
                out.push(char::from(BASE64URL[(bits >> held) as usize & 63]));
            }
        }
        if held > 0 {
            out.push(char::from(BASE64URL[(bits << (6 - held)) as usize & 63]));
        }
        Token(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Token {
    type Error = BadToken;

    fn try_from(raw: String) -> Result<Token, BadToken> {
        Token::parse(&raw)
    }
}

impl From<Token> for String {
    fn from(token: Token) -> String {
        token.0
    }
}

impl fmt::Display for BadToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a room code or device key")
    }
}

/// What the board keeps to rejoin its room: the room's code, which the phone learns from the QR
/// code, and the board's own key, which nobody else ever sees.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pairing {
    pub room: Token,
    pub key: Token,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Board,
    Remote,
}

impl Seat {
    pub fn name(self) -> &'static str {
        match self {
            Seat::Board => "board",
            Seat::Remote => "remote",
        }
    }
}

/// The relay's path for a seat in a room. The page's origin goes in front.
pub fn socket_path(room: &Token, seat: Seat, key: &Token) -> String {
    format!(
        "/api/remote/{}?role={}&key={}",
        room.as_str(),
        seat.name(),
        key.as_str()
    )
}

const LINK_PREFIX: &str = "#remote=";

/// The address the phone opens: the app itself, told by the fragment to be a remote. A fragment
/// never reaches a server, so the room code stays out of request logs.
pub fn remote_link(origin: &str, room: &Token) -> String {
    format!("{origin}/{LINK_PREFIX}{}", room.as_str())
}

/// What the page's fragment asks for: nothing (the teacher's app), or the remote for a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Address {
    App,
    Remote(Result<Token, BadToken>),
}

pub fn read_address(hash: &str) -> Address {
    match hash.strip_prefix(LINK_PREFIX) {
        Some(code) => Address::Remote(Token::parse(code)),
        None => Address::App,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Command {
    Shuffle,
    Oracle,
    Group,
    Rotate,
}

/// What the phone's buttons need to know about the board's class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardStatus {
    pub students: bool,
    pub grouped: bool,
    pub centers: bool,
}

impl BoardStatus {
    /// Whether the board can act on `command` right now. Rotating needs two groups or more, but
    /// with one group the board rotates to the same place, which does no harm.
    pub fn allows(self, command: Command) -> bool {
        match command {
            Command::Oracle => self.students,
            Command::Shuffle | Command::Group => self.grouped,
            Command::Rotate => self.centers,
        }
    }
}

/// What the phone sends the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FromRemote {
    Command { command: Command },
}

/// What the board sends the phone. `Unpair` stops at the relay, which turns the room off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FromBoard {
    Status(BoardStatus),
    Done { command: Command },
    Unpair,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FromRelay {
    Presence { peer: bool },
}

/// Whether the other seat has a device on it, when `text` is the relay saying so.
pub fn presence(text: &str) -> Option<bool> {
    match decode::<FromRelay>(text)? {
        FromRelay::Presence { peer } => Some(peer),
    }
}

pub fn encode<T: Serialize>(message: &T) -> String {
    serde_json::to_string(message).expect("remote messages always serialize")
}

/// `None` for anything this build does not understand, which a page ignores.
pub fn decode<T: for<'de> Deserialize<'de>>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}

/// Why the relay closed a socket for good, from its close code. Anything else is a dropped
/// connection worth retrying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Another device already holds this seat.
    Claimed,
    /// The same device opened this seat again, in another tab.
    Replaced,
    /// The board turned this room off.
    Unpaired,
}

impl Refusal {
    pub fn from_close_code(code: u16) -> Option<Refusal> {
        match code {
            4001 => Some(Refusal::Claimed),
            4002 => Some(Refusal::Replaced),
            4003 => Some(Refusal::Unpaired),
            _ => None,
        }
    }
}

/// Waits between reconnect attempts: quick at first, so a blip goes unnoticed, then slower, so a
/// board left open over a weekend without a network does not hammer anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backoff {
    attempt: u32,
}

impl Backoff {
    const FIRST: Millis = Millis(500);
    const MAX: Millis = Millis(15_000);

    pub fn new() -> Backoff {
        Backoff { attempt: 0 }
    }

    /// The wait before the next attempt, which doubles each time up to the cap.
    pub fn next_delay(&mut self) -> Millis {
        let delay = Self::FIRST
            .0
            .saturating_mul(1 << self.attempt.min(16))
            .min(Self::MAX.0);
        self.attempt = self.attempt.saturating_add(1);
        Millis(delay)
    }

    /// After a connection opened, the next drop retries quickly again.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

impl Default for Backoff {
    fn default() -> Backoff {
        Backoff::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: &str = "AbCdEfGhIjKlMnOpQrSt_-";

    fn room() -> Token {
        Token::parse(ROOM).unwrap()
    }

    #[test]
    fn a_token_is_twenty_two_base64url_characters() {
        assert_eq!(room().as_str(), ROOM);
        assert_eq!(Token::parse(""), Err(BadToken));
        assert_eq!(Token::parse(&ROOM[1..]), Err(BadToken));
        assert_eq!(Token::parse(&format!("{ROOM}A")), Err(BadToken));
        assert_eq!(Token::parse("AbCdEfGhIjKlMnOpQrSt+/"), Err(BadToken));
        assert_eq!(Token::parse("AbCdEfGhIjKlMnOpQrSt=="), Err(BadToken));
    }

    #[test]
    fn bytes_encode_as_unpadded_base64url() {
        assert_eq!(Token::from_bytes([0; TOKEN_BYTES]).as_str(), "A".repeat(22));
        assert_eq!(
            Token::from_bytes([255; TOKEN_BYTES]).as_str(),
            format!("{}w", "_".repeat(21))
        );
        // RFC 4648 test vector "foobar", padded to sixteen bytes with zeros.
        let mut bytes = [0; TOKEN_BYTES];
        bytes[..6].copy_from_slice(b"foobar");
        assert!(Token::from_bytes(bytes).as_str().starts_with("Zm9vYmFy"));
    }

    #[test]
    fn every_encoded_token_parses() {
        for seed in 0..=255u8 {
            let bytes = std::array::from_fn(|i| seed.wrapping_mul(31).wrapping_add(i as u8 * 7));
            let token = Token::from_bytes(bytes);
            assert_eq!(Token::parse(token.as_str()), Ok(token));
        }
    }

    #[test]
    fn a_pairing_round_trips_through_json_and_refuses_a_bad_code() {
        let pairing = Pairing {
            room: room(),
            key: Token::from_bytes([7; TOKEN_BYTES]),
        };
        let json = serde_json::to_string(&pairing).unwrap();
        assert_eq!(serde_json::from_str::<Pairing>(&json).unwrap(), pairing);
        let bad = json.replace(ROOM, "short");
        assert!(serde_json::from_str::<Pairing>(&bad).is_err());
    }

    #[test]
    fn the_socket_path_names_the_room_seat_and_key() {
        let key = Token::from_bytes([0; TOKEN_BYTES]);
        assert_eq!(
            socket_path(&room(), Seat::Remote, &key),
            format!("/api/remote/{ROOM}?role=remote&key={}", "A".repeat(22))
        );
    }

    #[test]
    fn the_link_puts_the_room_in_the_fragment_and_reads_back() {
        let link = remote_link("https://tivan.example", &room());
        assert_eq!(link, format!("https://tivan.example/#remote={ROOM}"));
        let hash = &link[link.find('#').unwrap()..];
        assert_eq!(read_address(hash), Address::Remote(Ok(room())));
    }

    #[test]
    fn only_a_remote_fragment_turns_the_page_into_a_remote() {
        assert_eq!(read_address(""), Address::App);
        assert_eq!(read_address("#"), Address::App);
        assert_eq!(read_address("#settings"), Address::App);
        assert_eq!(read_address("#remote=oops"), Address::Remote(Err(BadToken)));
    }

    #[test]
    fn messages_use_the_relays_shapes() {
        assert_eq!(
            encode(&FromRemote::Command {
                command: Command::Shuffle
            }),
            r#"{"type":"command","command":"shuffle"}"#
        );
        assert_eq!(
            encode(&FromBoard::Status(BoardStatus {
                students: true,
                grouped: true,
                centers: false
            })),
            r#"{"type":"status","students":true,"grouped":true,"centers":false}"#
        );
        assert_eq!(
            encode(&FromBoard::Done {
                command: Command::Rotate
            }),
            r#"{"type":"done","command":"rotate"}"#
        );
        assert_eq!(encode(&FromBoard::Unpair), r#"{"type":"unpair"}"#);
    }

    #[test]
    fn each_end_reads_what_the_relay_forwards_and_ignores_the_rest() {
        assert_eq!(
            decode::<FromRemote>(r#"{"type":"command","command":"group"}"#),
            Some(FromRemote::Command {
                command: Command::Group
            })
        );
        assert_eq!(
            decode::<FromBoard>(
                r#"{"type":"status","students":true,"grouped":false,"centers":false}"#
            ),
            Some(FromBoard::Status(BoardStatus {
                students: true,
                grouped: false,
                centers: false
            }))
        );
        assert_eq!(
            decode::<FromBoard>(r#"{"type":"done","command":"oracle"}"#),
            Some(FromBoard::Done {
                command: Command::Oracle
            })
        );
        assert_eq!(
            decode::<FromRemote>(r#"{"type":"command","command":"x"}"#),
            None
        );
        assert_eq!(decode::<FromBoard>(r#"{"type":"status"}"#), None);
        assert_eq!(decode::<FromBoard>("not json"), None);
    }

    #[test]
    fn presence_is_told_apart_from_the_peers_messages() {
        assert_eq!(presence(r#"{"type":"presence","peer":true}"#), Some(true));
        assert_eq!(presence(r#"{"type":"presence","peer":false}"#), Some(false));
        assert_eq!(presence(r#"{"type":"command","command":"shuffle"}"#), None);
        assert_eq!(presence(r#"{"type":"presence"}"#), None);
        assert_eq!(
            decode::<FromRemote>(r#"{"type":"presence","peer":true}"#),
            None
        );
    }

    #[test]
    fn a_command_is_allowed_only_when_the_board_has_what_it_needs() {
        let empty = BoardStatus::default();
        let ungrouped = BoardStatus {
            students: true,
            ..empty
        };
        let grouped = BoardStatus {
            grouped: true,
            ..ungrouped
        };
        let centers = BoardStatus {
            centers: true,
            ..grouped
        };
        for command in [
            Command::Shuffle,
            Command::Oracle,
            Command::Group,
            Command::Rotate,
        ] {
            assert!(!empty.allows(command), "{command:?}");
            assert!(centers.allows(command), "{command:?}");
        }
        assert!(ungrouped.allows(Command::Oracle));
        assert!(!ungrouped.allows(Command::Shuffle));
        assert!(grouped.allows(Command::Shuffle));
        assert!(grouped.allows(Command::Group));
        assert!(!grouped.allows(Command::Rotate));
    }

    #[test]
    fn only_the_relays_own_close_codes_are_refusals() {
        assert_eq!(Refusal::from_close_code(4001), Some(Refusal::Claimed));
        assert_eq!(Refusal::from_close_code(4002), Some(Refusal::Replaced));
        assert_eq!(Refusal::from_close_code(4003), Some(Refusal::Unpaired));
        for dropped in [1000, 1001, 1006, 1011, 4000, 4004] {
            assert_eq!(Refusal::from_close_code(dropped), None, "{dropped}");
        }
    }

    #[test]
    fn backoff_doubles_to_a_cap_and_starts_over_after_a_reset() {
        let mut backoff = Backoff::new();
        let delays: Vec<u64> = (0..8).map(|_| backoff.next_delay().0).collect();
        assert_eq!(
            delays,
            [500, 1_000, 2_000, 4_000, 8_000, 15_000, 15_000, 15_000]
        );
        for _ in 0..100 {
            assert_eq!(backoff.next_delay(), Millis(15_000));
        }
        backoff.reset();
        assert_eq!(backoff.next_delay(), Millis(500));
    }
}
