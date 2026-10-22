use dioxus::prelude::*;

use crate::app::use_app;
use crate::domain::{Avatar, Skin};

/// Static markup with no user data, so it is safe to inject. It must be in the page once and must
/// not sit in a `display: none` container, which would break its clip paths; its own zero size
/// and absolute position already hide it.
const SPRITE: &str = concat!(
    include_str!("../../assets/avatars.svg"),
    include_str!("../../assets/skins.svg")
);

/// The shared definitions every [`AvatarIcon`] points at.
#[component]
pub fn AvatarSprite() -> Element {
    rsx! {
        div { dangerous_inner_html: SPRITE }
    }
}

/// An avatar wearing whatever skin the teacher has switched on today. Decorative: the student's
/// name beside it is what assistive technology reads.
#[component]
pub fn AvatarIcon(avatar: Avatar) -> Element {
    let skin = use_app().active_skin();
    rsx! {
        SkinnedAvatar { avatar, skin }
    }
}

/// An avatar wearing its costume from a given skin, for a place that is not showing the active one.
///
/// The two are cut out as one sticker, so a hat gets the same edge as the face. The art is drawn a
/// little smaller than the box to leave room inside it for that edge and its shadow.
#[component]
pub fn SkinnedAvatar(avatar: Avatar, skin: Option<Skin>) -> Element {
    rsx! {
        svg {
            class: "avatar",
            view_box: "0 0 100 100",
            "aria-hidden": "true",
            "focusable": "false",
            g { filter: "url(#av-sticker)",
                g { transform: "translate(50 53) scale(.84) translate(-50 -56)",
                    r#use { href: "#avatar-{avatar.name()}" }
                    if let Some(skin) = skin {
                        r#use { href: "#costume-{skin.costume(avatar).name()}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Costume;

    #[test]
    fn the_sprite_has_one_symbol_for_every_avatar() {
        for avatar in Avatar::ALL {
            let symbol = format!("<symbol id=\"avatar-{}\"", avatar.name());
            assert!(SPRITE.contains(&symbol), "no art for {avatar:?}");
        }
        assert_eq!(
            SPRITE.matches("<symbol id=\"avatar-").count(),
            Avatar::ALL.len(),
            "art without an Avatar"
        );
    }

    #[test]
    fn the_sprite_has_one_symbol_for_every_costume() {
        for costume in Costume::ALL {
            let symbol = format!("<symbol id=\"costume-{}\"", costume.name());
            assert!(SPRITE.contains(&symbol), "no art for {costume:?}");
        }
        assert_eq!(
            SPRITE.matches("<symbol id=\"costume-").count(),
            Costume::ALL.len(),
            "art without a Costume"
        );
    }

    #[test]
    fn the_sprite_defines_the_sticker_every_avatar_is_cut_out_with() {
        assert_eq!(SPRITE.matches("<filter id=\"av-sticker\"").count(), 1);
    }
}
