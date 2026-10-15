//! Components hold no rules. Each handler is one call into the domain through `AppState`.

use dioxus::prelude::*;

use crate::app::use_app;

pub mod build;
pub mod whiteboard;

#[component]
pub fn NoticeBanner() -> Element {
    let mut state = use_app();
    let Some(notice) = state.notice() else {
        return rsx! {};
    };
    let role = if notice.is_dismissible() {
        "status"
    } else {
        "alert"
    };
    rsx! {
        div { class: "notice", role,
            p { "{notice.message()}" }
            if notice.is_dismissible() {
                button {
                    class: "btn btn-small",
                    r#type: "button",
                    onclick: move |_| state.dismiss_notice(),
                    "Dismiss"
                }
            }
        }
    }
}
