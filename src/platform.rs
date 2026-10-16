//! Everything that touches a browser global, so the rest of the crate runs natively under test.

use std::num::NonZeroUsize;

use dioxus::logger::tracing::warn;
use js_sys::{Date, Math};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortSignal, Headers, KeyboardEvent, RequestCache, RequestInit, Response, Storage, Window,
};

use crate::account;
use crate::api::{Api, Http, HttpReply, HttpRequest, TransportError, timeout_ms};
use crate::domain::{Millis, Rng};
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
        let window = web_sys::window().ok_or_else(|| TransportError::new("there is no window"))?;
        let init = RequestInit::new();
        init.set_method(request.method.as_str());
        // A stale answer to a read would later be written back over newer data.
        init.set_cache(RequestCache::NoStore);
        init.set_signal(Some(&AbortSignal::timeout_with_u32(timeout_ms(
            request.path,
        ))));
        if let Some(body) = request.body {
            let headers = Headers::new().map_err(|cause| TransportError::new(describe(&cause)))?;
            headers
                .set("Content-Type", "application/json")
                .map_err(|cause| TransportError::new(describe(&cause)))?;
            init.set_headers(&headers);
            init.set_body(&JsValue::from_str(body));
        }
        let response = JsFuture::from(window.fetch_with_str_and_init(request.path, &init))
            .await
            .map_err(|cause| TransportError::new(describe(&cause)))?;
        let response: Response = response
            .dyn_into()
            .map_err(|_| TransportError::new("fetch did not return a response"))?;
        let text = response
            .text()
            .map_err(|cause| TransportError::new(describe(&cause)))?;
        let body = JsFuture::from(text)
            .await
            .map_err(|cause| TransportError::new(describe(&cause)))?
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
    let outcome = web_sys::window()
        .ok_or_else(|| JsValue::from_str("there is no window"))
        .and_then(|window| window.location().set_href("/auth/login"));
    if let Err(cause) = outcome {
        warn!("could not go to the sign-in page: {}", describe(&cause));
    }
}

pub fn reload() {
    let outcome = web_sys::window()
        .ok_or_else(|| JsValue::from_str("there is no window"))
        .and_then(|window| window.location().reload());
    if let Err(cause) = outcome {
        warn!("could not reload the page: {}", describe(&cause));
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
}
