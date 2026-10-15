//! Everything that touches a browser global, so the rest of the crate runs natively under test.

use std::num::NonZeroUsize;

use js_sys::{Date, Math};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::Storage;

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
