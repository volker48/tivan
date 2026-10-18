//! Everything that touches a browser global, so the rest of the crate runs natively under test.

use std::num::NonZeroUsize;

use dioxus::logger::tracing::warn;
use futures_channel::mpsc::UnboundedSender;
use js_sys::{Date, Math};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortSignal, CloseEvent, KeyboardEvent, Location, MessageEvent, RequestCache, RequestInit,
    Response, Storage, WebSocket, Window,
};

use crate::account;
use crate::api::{Api, Http, HttpReply, HttpRequest, TransportError, timeout_ms};
use crate::domain::remote::{Address, TOKEN_BYTES, Token, read_address};
use crate::domain::{CalendarDay, Millis, Motion, Rng};
use crate::store::{self, StorageError};

/// Not cryptographic, which classroom groups do not need. Chosen over `rand` to avoid the
/// `getrandom` wasm backend configuration.
pub struct JsRng;

impl Rng for JsRng {
    fn below(&mut self, n: NonZeroUsize) -> usize {
        // `Math.random()` is below 1, but the product can round up to `n` for very large `n`.
        ((Math::random() * n.get() as f64) as usize).min(n.get() - 1)
    }
}

pub fn now() -> Millis {
    Millis(Date::now() as u64)
}

/// Whether the system asks for less motion. The stylesheet shortens CSS animations under the same
/// query, but it cannot stop a timer-driven change of what is on screen.
pub fn prefers_reduced_motion() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    match window.match_media("(prefers-reduced-motion: reduce)") {
        Ok(query) => query.is_some_and(|query| query.matches()),
        Err(cause) => {
            warn!(
                "could not read the reduced-motion preference: {}",
                describe(&cause)
            );
            false
        }
    }
}

/// The date on the teacher's wall clock, which is what a season means to them.
pub fn today() -> CalendarDay {
    let now = Date::new_0();
    CalendarDay {
        month: now.get_month() as u8 + 1,
        day: now.get_date() as u8,
    }
}

/// `window.localStorage`. Each call opens it afresh, because access itself can throw (blocked
/// site data) and that has to reach the teacher as an error rather than a panic.
pub struct LocalStorage;

impl LocalStorage {
    fn open() -> Result<Storage, StorageError> {
        let window = web_sys::window().ok_or_else(|| StorageError::new("there is no window"))?;
        window
            .local_storage()
            .map_err(|cause| {
                StorageError::new(format!("localStorage is blocked: {}", describe(&cause)))
            })?
            .ok_or_else(|| StorageError::new("localStorage is not available"))
    }
}

impl store::Storage for LocalStorage {
    fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        Self::open()?.get_item(key).map_err(|cause| {
            StorageError::new(format!("could not read {key}: {}", describe(&cause)))
        })
    }

    fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        Self::open()?.set_item(key, value).map_err(|cause| {
            StorageError::new(format!("could not write {key}: {}", describe(&cause)))
        })
    }
}

/// A thrown value as text. `DOMException` (quota, security) is an `Error`, so it has a message.
fn describe(thrown: &JsValue) -> String {
    thrown
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| thrown.as_string())
        .unwrap_or_else(|| format!("{thrown:?}"))
}

/// `fetch` against the page's own origin, which carries the session cookie.
pub struct Fetch;

impl Http for Fetch {
    async fn send(&self, request: HttpRequest<'_>) -> Result<HttpReply, TransportError> {
        let thrown = |cause: JsValue| TransportError::new(describe(&cause));
        let window = web_sys::window().ok_or_else(|| TransportError::new("there is no window"))?;
        let init = RequestInit::new();
        init.set_method(request.method.as_str());
        // A stale answer to a read would later be written back over newer data.
        init.set_cache(RequestCache::NoStore);
        init.set_signal(Some(&AbortSignal::timeout_with_u32(timeout_ms(
            request.path,
        ))));
        if let Some(body) = request.body {
            init.set_body(&JsValue::from_str(body));
        }
        let response = JsFuture::from(window.fetch_with_str_and_init(request.path, &init))
            .await
            .map_err(thrown)?;
        let response: Response = response
            .dyn_into()
            .map_err(|_| TransportError::new("fetch did not return a response"))?;
        let text = response.text().map_err(thrown)?;
        let body = JsFuture::from(text)
            .await
            .map_err(thrown)?
            .as_string()
            .ok_or_else(|| TransportError::new("the reply was not text"))?;
        Ok(HttpReply {
            status: response.status(),
            body,
        })
    }
}

pub fn api() -> Api<Fetch> {
    Api::new(Fetch)
}

/// Sends the browser to Google through the server. The page is replaced, so nothing after this
/// call matters.
pub fn go_to_login() {
    navigate("go to the sign-in page", |location| {
        location.set_href("/auth/login")
    });
}

pub fn reload() {
    navigate("reload the page", Location::reload);
}

/// Runs `action` on the window's location, and warns that the page could not `what` when it fails.
fn navigate(what: &str, action: impl FnOnce(&Location) -> Result<(), JsValue>) {
    let outcome = web_sys::window()
        .ok_or_else(|| JsValue::from_str("there is no window"))
        .and_then(|window| action(&window.location()));
    if let Err(cause) = outcome {
        warn!("could not {what}: {}", describe(&cause));
    }
}

/// The sign-in failure code the server left in the address, if any. The query is cleared as it is
/// read, so reloading the page does not show the same failure again.
pub fn take_auth_error() -> Option<String> {
    let window = web_sys::window()?;
    let location = window.location();
    let search = match location.search() {
        Ok(search) => search,
        Err(cause) => {
            warn!("could not read the address: {}", describe(&cause));
            return None;
        }
    };
    let code = account::auth_error_code(&search)?;
    let cleared = location.pathname().and_then(|path| {
        window
            .history()?
            .replace_state_with_url(&JsValue::NULL, "", Some(&path))
    });
    if let Err(cause) = cleared {
        warn!(
            "could not clear the sign-in error from the address: {}",
            describe(&cause)
        );
    }
    Some(code)
}

/// Keeps the Escape listener registered. Dropping it removes the listener.
pub struct KeyGuard {
    window: Window,
    closure: Closure<dyn FnMut(KeyboardEvent)>,
}

impl Drop for KeyGuard {
    fn drop(&mut self) {
        if let Err(cause) = self
            .window
            .remove_event_listener_with_callback("keydown", self.closure.as_ref().unchecked_ref())
        {
            warn!("could not remove the Escape listener: {}", describe(&cause));
        }
    }
}

/// Calls `handler` on every Escape press anywhere on the page. A listener on the window, not on a
/// component's root, because the focused element is often removed mid-use (a timer button
/// replaced by Pause) and key events then target the body.
pub fn on_escape(mut handler: impl FnMut() + 'static) -> Result<KeyGuard, String> {
    let window = web_sys::window().ok_or_else(|| "there is no window".to_owned())?;
    let closure = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        // Escape during IME composition cancels the composition, not the screen.
        if event.key() == "Escape" && !event.is_composing() {
            handler();
        }
    });
    window
        .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
        .map_err(|cause| format!("could not listen for key presses: {}", describe(&cause)))?;
    Ok(KeyGuard { window, closure })
}

/// What the page's fragment asks for. Read once, at startup.
pub fn address() -> Address {
    let hash = web_sys::window()
        .and_then(|window| window.location().hash().ok())
        .unwrap_or_default();
    read_address(&hash)
}

/// `https://host`, for links back to this page.
pub fn origin() -> Option<String> {
    web_sys::window()?.location().origin().ok()
}

/// A fresh room code or device key from the browser's cryptographic source. `None` only where the
/// browser has none, which every browser Tivan supports does.
pub fn random_token() -> Option<Token> {
    let crypto = web_sys::window()?.crypto().ok()?;
    let mut bytes = [0u8; TOKEN_BYTES];
    match crypto.get_random_values_with_u8_array(&mut bytes) {
        Ok(_) => Some(Token::from_bytes(bytes)),
        Err(cause) => {
            warn!("could not make a random code: {}", describe(&cause));
            None
        }
    }
}

/// A short buzz on phones that have one. iOS ignores it.
pub fn buzz() {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().vibrate_with_duration(40);
    }
}

/// What happened on a [`Socket`], in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocketEvent {
    Opened,
    Received(String),
    Closed { code: u16 },
}

/// A WebSocket to this page's own origin, reporting into a channel. Dropping it closes the
/// connection and stops the reports.
pub struct Socket {
    ws: WebSocket,
    _on_open: Closure<dyn FnMut()>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

impl Socket {
    /// `path` starts with `/`. Every failure to connect also arrives as `Closed`, because the
    /// browser always closes after an error.
    pub fn open(path: &str, events: UnboundedSender<SocketEvent>) -> Result<Socket, String> {
        let location = web_sys::window()
            .ok_or_else(|| "there is no window".to_owned())?
            .location();
        let secure = location.protocol().map_err(|cause| describe(&cause))? == "https:";
        let host = location.host().map_err(|cause| describe(&cause))?;
        let scheme = if secure { "wss" } else { "ws" };
        let ws = WebSocket::new(&format!("{scheme}://{host}{path}")).map_err(|cause| {
            format!(
                "could not open the remote's connection: {}",
                describe(&cause)
            )
        })?;

        let opened = events.clone();
        let on_open = Closure::<dyn FnMut()>::new(move || {
            let _ = opened.unbounded_send(SocketEvent::Opened);
        });
        let received = events.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let _ = received.unbounded_send(SocketEvent::Received(text));
            }
        });
        let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
            let _ = events.unbounded_send(SocketEvent::Closed { code: event.code() });
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        Ok(Socket {
            ws,
            _on_open: on_open,
            _on_message: on_message,
            _on_close: on_close,
        })
    }

    /// Sends now, or not at all: a message for a connection that is not open is dropped, because
    /// what it said is stale by the time the connection comes back.
    pub fn send(&self, text: &str) {
        if self.ws.ready_state() != WebSocket::OPEN {
            return;
        }
        if let Err(cause) = self.ws.send_with_str(text) {
            warn!("could not send to the remote: {}", describe(&cause));
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        if let Err(cause) = self.ws.close() {
            warn!(
                "could not close the remote's connection: {}",
                describe(&cause)
            );
        }
    }
}

/// Whether the phone can read its motion sensor, and what it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionAccess {
    /// Readings arrive without asking.
    Granted,
    /// The teacher has to allow it, from a tap.
    Ask,
    Unsupported,
}

pub fn motion_access() -> MotionAccess {
    match motion_access_js().as_str() {
        "granted" => MotionAccess::Granted,
        "ask" => MotionAccess::Ask,
        _ => MotionAccess::Unsupported,
    }
}

/// Asks the teacher to allow the motion sensor. Call from a tap.
pub async fn request_motion() -> bool {
    JsFuture::from(request_motion_js())
        .await
        .is_ok_and(|granted| granted.as_bool() == Some(true))
}

/// Keeps the motion listener registered. Dropping it stops the readings.
pub struct MotionGuard {
    stop: js_sys::Function,
    _closure: Closure<dyn FnMut(f64, f64, f64, f64, bool)>,
}

impl Drop for MotionGuard {
    fn drop(&mut self) {
        if let Err(cause) = self.stop.call0(&JsValue::NULL) {
            warn!("could not stop the motion listener: {}", describe(&cause));
        }
    }
}

/// Calls `handler` with every motion reading while the guard lives.
pub fn on_motion(mut handler: impl FnMut(Motion) + 'static) -> MotionGuard {
    let closure = Closure::<dyn FnMut(f64, f64, f64, f64, bool)>::new(
        move |at: f64, x: f64, y: f64, z: f64, with_gravity: bool| {
            handler(Motion {
                at: Millis(at.max(0.0) as u64),
                x,
                y,
                z,
                with_gravity,
            });
        },
    );
    let stop = listen_for_motion(&closure);
    MotionGuard {
        stop,
        _closure: closure,
    }
}

#[wasm_bindgen(module = "/src/remote_motion.js")]
extern "C" {
    #[wasm_bindgen(js_name = motionAccess)]
    fn motion_access_js() -> String;

    #[wasm_bindgen(js_name = requestMotion)]
    fn request_motion_js() -> js_sys::Promise;

    #[wasm_bindgen(js_name = listenForMotion)]
    fn listen_for_motion(
        on_motion: &Closure<dyn FnMut(f64, f64, f64, f64, bool)>,
    ) -> js_sys::Function;
}

#[wasm_bindgen(module = "/src/animate_shuffle.js")]
extern "C" {
    /// Makes the chips that change group when the DOM is next patched fly to their new places.
    /// Call it immediately before the state change that re-deals them.
    #[wasm_bindgen(js_name = animateShuffle)]
    pub fn animate_shuffle();

    /// Makes the group cards that move to another center when the DOM is next patched fly there.
    /// Call it immediately before the state change that rotates the groups.
    #[wasm_bindgen(js_name = animateRotation)]
    pub fn animate_rotation();

    /// Wobbles the group cards and throws every chip up across the wall before it drifts down
    /// into its new place, for the phone remote's shake. Call it immediately before the reshuffle.
    #[wasm_bindgen(js_name = animateBurst)]
    pub fn animate_burst();
}
