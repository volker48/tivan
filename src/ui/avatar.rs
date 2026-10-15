use dioxus::prelude::*;

use crate::domain::Avatar;

/// Static markup with no user data, so it is safe to inject. It must be in the page once and must
/// not sit in a `display: none` container, which would break its clip paths; its own zero size
/// and absolute position already hide it.
const SPRITE: &str = include_str!("../../assets/avatars.svg");

/// The shared definitions every [`AvatarIcon`] points at.
#[component]
pub fn AvatarSprite() -> Element {
    rsx! {
        div { dangerous_inner_html: SPRITE }
    }
}

/// Decorative: the student's name beside it is what assistive technology reads.
#[component]
pub fn AvatarIcon(avatar: Avatar) -> Element {
    rsx! {
        svg {
            class: "avatar",
            view_box: "0 0 100 100",
            "aria-hidden": "true",
            "focusable": "false",
            r#use { href: "#avatar-{avatar.name()}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
