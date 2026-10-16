use dioxus::prelude::*;

use crate::app::{Mode, use_app, use_escape};
use crate::domain::{Avatar, Skin, SkinChoice};
use crate::ui::NoticeBanner;
use crate::ui::avatar::SkinnedAvatar;

/// How many avatars each option shows wearing its skin.
const PREVIEW_AVATARS: usize = 3;

const AUTO_HINT_ID: &str = "skin-auto-hint";

#[component]
pub fn SettingsMode() -> Element {
    let mut state = use_app();
    use_escape(move || state.set_mode(Mode::Build));
    rsx! {
        header { class: "topbar",
            button {
                class: "btn btn-small",
                r#type: "button",
                onclick: move |_| state.set_mode(Mode::Build),
                "Back to Build"
            }
            h1 { class: "page-title", "Settings" }
        }
        NoticeBanner {}
        main { class: "build settings",
            SkinSettings {}
        }
    }
}

/// Every season, then what automatic shows now.
fn auto_hint(showing: Option<Skin>) -> String {
    let seasons: Vec<String> = Skin::ALL
        .into_iter()
        .map(|skin| format!("{} ({})", skin.label(), skin.season()))
        .collect();
    let showing = showing.map_or("no skin", Skin::label);
    format!("{}. Today: {showing}.", seasons.join(", "))
}

#[component]
fn SkinSettings() -> Element {
    let mut state = use_app();
    let chosen = state.with_doc(|doc| doc.settings().skin());
    let today = state.today();
    let hint = auto_hint(SkinChoice::Auto.resolve(today));
    rsx! {
        section { class: "panel",
            fieldset { class: "skin-options",
                legend { "Avatar skins" }
                for option in SkinChoice::options() {
                    label { key: "{option.label()}", class: "skin-option",
                        input {
                            r#type: "radio",
                            name: "skin",
                            checked: option == chosen,
                            aria_describedby: (option == SkinChoice::Auto).then_some(AUTO_HINT_ID),
                            onchange: move |_| state.edit(|doc, _| doc.set_skin_choice(option)),
                        }
                        span { class: "skin-text",
                            span { class: "skin-name", "{option.label()}" }
                            if option == SkinChoice::Auto {
                                span { id: AUTO_HINT_ID, class: "hint", "{hint}" }
                            }
                        }
                        span { class: "skin-preview",
                            for avatar in Avatar::ALL.into_iter().take(PREVIEW_AVATARS) {
                                SkinnedAvatar {
                                    key: "{avatar.name()}",
                                    avatar,
                                    skin: option.resolve(today),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_automatic_hint_names_every_season_and_what_shows_today() {
        assert_eq!(
            auto_hint(Some(Skin::Halloween)),
            "Halloween (Oct 1 to Oct 31), Christmas (Dec 1 to Dec 25). Today: Halloween."
        );
    }

    #[test]
    fn the_automatic_hint_says_so_when_nothing_is_in_season() {
        assert!(auto_hint(None).ends_with("Today: no skin."));
    }
}
