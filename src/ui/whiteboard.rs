use dioxus::prelude::*;

use crate::app::{Mode, use_app};

#[component]
pub fn WhiteboardMode() -> Element {
    let mut state = use_app();
    rsx! {
        button {
            class: "btn",
            r#type: "button",
            onclick: move |_| state.set_mode(Mode::Build),
            "Back to Build"
        }
    }
}
