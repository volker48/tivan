use dioxus::prelude::*;

use crate::app::{Mode, use_app};
use crate::ui::NoticeBanner;

#[component]
pub fn BuildMode() -> Element {
    let mut state = use_app();
    rsx! {
        NoticeBanner {}
        h1 { "Tivan" }
        button {
            class: "btn",
            r#type: "button",
            onclick: move |_| state.set_mode(Mode::Whiteboard),
            "Whiteboard"
        }
    }
}
