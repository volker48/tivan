//! Components hold no rules. Each handler is one call into the domain through `AppState`.

use dioxus::logger::tracing::warn;
use dioxus::prelude::*;

use crate::app::use_app;
use crate::domain::{
    CenterIdx, Class, GroupIdx, GroupView, Label, LabelError, MAX_LABEL_CHARS, MoveError, Student,
    StudentId,
};
use crate::platform;

pub mod avatar;
pub mod build;
pub mod whiteboard;

/// Group colors repeat after this many. The stylesheet defines `hue-0` to `hue-7`.
const HUES: usize = 8;

pub fn hue(idx: GroupIdx) -> usize {
    idx.get() % HUES
}

/// One group with its members resolved, owned so a component can keep it after the doc read ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupCard {
    pub idx: GroupIdx,
    pub name: Label,
    pub members: Vec<Student>,
}

fn group_card(view: GroupView<'_>) -> GroupCard {
    GroupCard {
        idx: view.idx,
        name: view.name.clone(),
        members: view.members.into_iter().cloned().collect(),
    }
}

pub fn group_cards(class: &Class) -> Vec<GroupCard> {
    class.group_views().into_iter().map(group_card).collect()
}

/// One center with the group card standing in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CenterCard {
    pub idx: CenterIdx,
    pub name: Label,
    pub card: GroupCard,
}

/// The centers in center order. `None` when the class does not use centers.
pub fn center_cards(class: &Class) -> Option<Vec<CenterCard>> {
    let views = class.center_views()?;
    let cards = views
        .into_iter()
        .map(|view| CenterCard {
            idx: view.idx,
            name: view.name.clone(),
            card: group_card(view.group),
        })
        .collect();
    Some(cards)
}

pub fn label_error_text(error: LabelError) -> String {
    match error {
        LabelError::Empty => "Type a name first.".to_owned(),
        LabelError::TooLong => format!("Keep names to {MAX_LABEL_CHARS} characters or fewer."),
    }
}

/// A move or rename can only fail on a stale id, which the UI never builds from the same render,
/// so a failure is logged rather than shown.
pub fn log_failed(action: &str, outcome: Option<Result<(), MoveError>>) {
    if let Some(Err(error)) = outcome {
        warn!("could not {action}: {error:?}");
    }
}

/// Puts the cursor in a field the moment it appears, so opening an editor never costs a second tap.
pub fn focus_on_mount(mounted: MountedEvent) {
    spawn(async move {
        if let Err(error) = mounted.set_focus(true).await {
            warn!("could not focus a new field: {error:?}");
        }
    });
}

/// "Round r of n" and the Rotate button, for a class that uses centers. Nothing otherwise. With
/// a single group there is nowhere to rotate to, so the button is off.
#[component]
pub fn RotateControls() -> Element {
    let mut state = use_app();
    let Some((round, total)) = state
        .with_class(|class| {
            let centers = class.grouping()?.centers()?;
            Some((centers.round(), centers.names().len()))
        })
        .flatten()
    else {
        return rsx! {};
    };
    rsx! {
        div { class: "rotate",
            span { class: "round", "Round {round} of {total}" }
            button {
                class: "btn btn-primary",
                r#type: "button",
                disabled: total < 2,
                onclick: move |_| {
                    platform::animate_rotation();
                    state.edit_class(|class, _| class.rotate_centers());
                },
                "Rotate"
            }
        }
    }
}

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

/// A text field for renaming. Enter or leaving the field saves, Escape cancels, and so does an
/// empty or unchanged name, because there is nothing to write.
#[component]
pub fn NameInput(
    initial: String,
    label: String,
    on_commit: EventHandler<Label>,
    on_cancel: EventHandler<()>,
) -> Element {
    let original = use_signal(|| initial.clone());
    let mut draft = use_signal(move || initial);
    let mut error = use_signal(|| None::<String>);
    // Removing a focused input can fire blur, which must not undo a cancel or commit twice.
    let mut finished = use_signal(|| false);
    let mut cancel = move || {
        finished.set(true);
        on_cancel.call(());
    };
    let mut commit = move || {
        if finished() {
            return;
        }
        match Label::parse(&draft.read()) {
            Ok(name) if name.as_str() != *original.read() => {
                finished.set(true);
                on_commit.call(name);
            }
            Ok(_) | Err(LabelError::Empty) => cancel(),
            Err(problem) => error.set(Some(label_error_text(problem))),
        }
    };
    rsx! {
        div { class: "name-edit", onclick: move |event| event.stop_propagation(),
            input {
                class: "name-input",
                r#type: "text",
                value: "{draft}",
                maxlength: "{MAX_LABEL_CHARS}",
                aria_label: "{label}",
                onmounted: focus_on_mount,
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event| match event.key() {
                    Key::Enter => commit(),
                    Key::Escape => cancel(),
                    _ => {}
                },
                onblur: move |_| commit(),
            }
            if let Some(message) = error() {
                p { class: "field-error", role: "alert", "{message}" }
            }
        }
    }
}

/// The student the teacher tapped to move, with the group they are in now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub student: Student,
    pub group: GroupIdx,
}

pub fn selection(class: &Class, id: StudentId) -> Option<Selection> {
    Some(Selection {
        student: class.student(id)?.clone(),
        group: class.grouping()?.group_of(id)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hue_has_a_stylesheet_rule() {
        let css = include_str!("../../assets/main.css");
        for n in 0..HUES {
            assert!(css.contains(&format!(".hue-{n} ")), "missing .hue-{n}");
        }
        assert!(!css.contains(&format!(".hue-{HUES} ")));
    }
}
