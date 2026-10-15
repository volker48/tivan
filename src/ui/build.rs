use dioxus::prelude::*;

use crate::app::{Mode, use_app, use_escape};
use crate::domain::{
    GroupCount, Label, LabelError, MAX_LABEL_CHARS, ParsedRoster, Student, StudentId, Timer,
    format_mm_ss, group_sizes, parse_roster,
};
use crate::notice::{Notice, count};
use crate::platform;
use crate::ui::{
    GroupCard, NameInput, NoticeBanner, Selection, focus_on_mount, group_cards, hue,
    label_error_text, log_failed, selection,
};

/// Where the stepper starts for a class that has never been grouped.
const DEFAULT_GROUPS: usize = 3;

#[component]
pub fn BuildMode() -> Element {
    let mut state = use_app();
    let selected = state.with_class(|class| class.id());
    let mut creating = use_signal(|| false);
    rsx! {
        header { class: "topbar",
            h1 { class: "brand", "Tivan" }
            TimerChip {}
            button {
                class: "btn btn-hero",
                r#type: "button",
                disabled: selected.is_none(),
                onclick: move |_| state.set_mode(Mode::Whiteboard),
                "Whiteboard"
            }
        }
        NoticeBanner {}
        main { class: "build",
            match selected {
                None => rsx! {
                    section { class: "welcome",
                        h2 { "Welcome to Tivan" }
                        p { "Make a class, paste in your students, and split them into groups." }
                        NewClassForm { can_cancel: false, on_done: move |_| {} }
                    }
                },
                Some(id) => rsx! {
                    ClassBar { creating }
                    if creating() {
                        NewClassForm { can_cancel: true, on_done: move |_| creating.set(false) }
                    } else {
                        ClassPanel { key: "{id.get()}" }
                    }
                },
            }
        }
    }
}

/// Shown while a timer exists, so a teacher in Build can see it is still counting.
#[component]
fn TimerChip() -> Element {
    let mut state = use_app();
    let (digits, label) = match state.timer() {
        Timer::Idle => return rsx! {},
        Timer::Finished { .. } => {
            return rsx! {
                button {
                    class: "timer-chip is-done",
                    r#type: "button",
                    onclick: move |_| state.cancel_timer(),
                    span { class: "digits", "Time's up!" }
                    span { class: "label", "Dismiss" }
                }
            };
        }
        Timer::Running { .. } => (format_mm_ss(state.remaining()), "Timer"),
        Timer::Paused { .. } => (format_mm_ss(state.remaining()), "Paused"),
    };
    rsx! {
        button {
            class: "timer-chip",
            r#type: "button",
            title: "Open the timer on the whiteboard",
            onclick: move |_| state.set_mode(Mode::Whiteboard),
            span { class: "digits", "{digits}" }
            span { class: "label", "{label}" }
        }
    }
}

#[component]
fn ClassBar(creating: Signal<bool>) -> Element {
    let mut state = use_app();
    let (classes, selected) = state.with_doc(|doc| {
        let classes: Vec<_> = doc
            .classes()
            .iter()
            .map(|class| (class.id(), class.name().clone()))
            .collect();
        (classes, doc.selected().map(|class| class.id()))
    });
    // While the new-class form is open, no class pill reads as current.
    let current = selected.filter(|_| !creating());
    rsx! {
        nav { class: "class-bar", aria_label: "Classes",
            for (id , name) in classes {
                button {
                    key: "{id.get()}",
                    class: "pill",
                    r#type: "button",
                    aria_pressed: "{Some(id) == current}",
                    onclick: move |_| {
                        creating.set(false);
                        if Some(id) != selected {
                            state.edit(|doc, _| doc.select(id));
                        }
                    },
                    "{name}"
                }
            }
            button {
                class: "pill pill-new",
                r#type: "button",
                aria_pressed: "{creating()}",
                onclick: move |_| creating.set(true),
                "+ New class"
            }
        }
    }
}

/// The names in a pasted roster, plus the notice owed to the teacher if lines were dropped.
fn read_roster(text: &str) -> (Vec<Label>, Option<Notice>) {
    let ParsedRoster { names, too_long } = parse_roster(text);
    let notice = Notice::roster_skipped(names.len(), too_long);
    (names, notice)
}

#[component]
fn NewClassForm(can_cancel: bool, on_done: EventHandler<()>) -> Element {
    let mut state = use_app();
    let mut name = use_signal(String::new);
    let paste = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        form {
            class: "panel new-class",
            onsubmit: move |event| {
                event.prevent_default();
                let label = match Label::parse(&name.read()) {
                    Ok(label) => label,
                    Err(problem) => {
                        error.set(Some(label_error_text(problem)));
                        return;
                    }
                };
                let (names, notice) = read_roster(&paste.read());
                state.edit(|doc, _| doc.create_class(label, names));
                if let Some(notice) = notice {
                    state.set_notice(notice);
                }
                on_done.call(());
            },
            h2 { "New class" }
            label { class: "field",
                span { "Class name" }
                input {
                    r#type: "text",
                    value: "{name}",
                    maxlength: "{MAX_LABEL_CHARS}",
                    placeholder: "Period 2 Math",
                    onmounted: focus_on_mount,
                    oninput: move |event| name.set(event.value()),
                }
            }
            label { class: "field",
                span { "Students (optional, you can add them later)" }
                PasteBox { text: paste }
            }
            if let Some(message) = error() {
                p { class: "field-error", role: "alert", "{message}" }
            }
            div { class: "form-actions",
                button { class: "btn btn-primary", r#type: "submit", "Create class" }
                if can_cancel {
                    button {
                        class: "btn",
                        r#type: "button",
                        onclick: move |_| on_done.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}

/// A box for a pasted column of names, with a live count so a bad paste shows before it lands.
#[component]
fn PasteBox(text: Signal<String>) -> Element {
    let ParsedRoster { names, too_long } = parse_roster(&text.read());
    rsx! {
        textarea {
            rows: "6",
            placeholder: "One name per line. A column copied from a spreadsheet works.",
            value: "{text}",
            oninput: move |event| text.set(event.value()),
        }
        if !names.is_empty() || too_long > 0 {
            p { class: "hint",
                "{count(names.len(), \"name\")} ready"
                if too_long > 0 {
                    ", {count(too_long, \"line\")} too long and will be skipped"
                }
            }
        }
    }
}

/// Everything below the class bar. The parent keys it by class id, so local state such as the
/// stepper and the open roster starts fresh for each class.
#[component]
fn ClassPanel() -> Element {
    let state = use_app();
    let (students, grouped) = state
        .with_class(|class| (class.students().len(), class.grouping().is_some()))
        .unwrap_or_default();
    rsx! {
        ClassHeader {}
        Roster {}
        if students > 0 {
            GroupControls {}
            if grouped {
                GroupBoard {}
            } else {
                div { class: "empty-state",
                    h3 { "No groups yet" }
                    p { "Choose how many groups you want, then press Make groups." }
                }
            }
        }
    }
}

#[component]
fn ClassHeader() -> Element {
    let mut state = use_app();
    let class =
        state.with_class(|class| (class.id(), class.name().clone(), class.students().len()));
    let mut renaming = use_signal(|| false);
    let mut confirming = use_signal(|| false);
    let Some((id, name, students)) = class else {
        return rsx! {};
    };
    rsx! {
        div { class: "class-head",
            if renaming() {
                NameInput {
                    initial: name.to_string(),
                    label: "Class name",
                    on_commit: move |label| {
                        state.edit_class(|class, _| class.rename(label));
                        renaming.set(false);
                    },
                    on_cancel: move |_| renaming.set(false),
                }
            } else {
                h2 { class: "class-title", "{name}" }
                span { class: "meta", "{count(students, \"student\")}" }
            }
            div { class: "class-actions",
                if confirming() {
                    span { class: "confirm-text", "Delete {name} for good?" }
                    button {
                        class: "btn btn-danger btn-small",
                        r#type: "button",
                        onclick: move |_| state.edit(|doc, _| doc.delete_class(id)),
                        "Yes, delete"
                    }
                    button {
                        class: "btn btn-small",
                        r#type: "button",
                        onclick: move |_| confirming.set(false),
                        "Keep it"
                    }
                } else {
                    if !renaming() {
                        button {
                            class: "btn btn-small",
                            r#type: "button",
                            onclick: move |_| renaming.set(true),
                            "Rename"
                        }
                    }
                    button {
                        class: "btn btn-small",
                        r#type: "button",
                        onclick: move |_| confirming.set(true),
                        "Delete"
                    }
                }
            }
        }
    }
}

#[component]
fn Roster() -> Element {
    let mut state = use_app();
    let (students, grouped) = state
        .with_class(|class| (class.students().to_vec(), class.grouping().is_some()))
        .unwrap_or_default();
    let total = students.len();
    let mut open = use_signal(|| !grouped);
    let mut pasting = use_signal(|| total == 0);
    let mut draft = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut paste = use_signal(String::new);
    let shown = open() || total == 0;
    rsx! {
        section { class: "panel",
            button {
                class: "panel-toggle",
                r#type: "button",
                aria_expanded: "{shown}",
                disabled: total == 0,
                onclick: move |_| open.set(!open()),
                span { "Students" }
                span { class: "badge", "{total}" }
            }
            if shown {
                if total == 0 {
                    p { class: "hint", "No students yet. Add a name below, or paste a whole list." }
                }
                form {
                    class: "add-row",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let parsed = Label::parse(&draft.read());
                        match parsed {
                            Ok(name) => {
                                state.edit_class(|class, _| class.add_student(name));
                                draft.set(String::new());
                                error.set(None);
                            }
                            Err(LabelError::Empty) => {}
                            Err(problem) => error.set(Some(label_error_text(problem))),
                        }
                    },
                    input {
                        r#type: "text",
                        value: "{draft}",
                        maxlength: "{MAX_LABEL_CHARS}",
                        placeholder: "Add a student",
                        aria_label: "Add a student",
                        oninput: move |event| draft.set(event.value()),
                    }
                    button { class: "btn btn-primary", r#type: "submit", "Add" }
                }
                if let Some(message) = error() {
                    p { class: "field-error", role: "alert", "{message}" }
                }
                ul { class: "roster-list",
                    for Student { id , name } in students {
                        li { key: "{id.get()}", class: "roster-item",
                            span { "{name}" }
                            button {
                                class: "icon-btn",
                                r#type: "button",
                                aria_label: "Remove {name}",
                                title: "Remove {name}",
                                onclick: move |_| {
                                    state.edit_class(|class, _| class.remove_student(id));
                                },
                                "\u{d7}"
                            }
                        }
                    }
                }
                if pasting() {
                    div { class: "paste-add",
                        PasteBox { text: paste }
                        div { class: "form-actions",
                            button {
                                class: "btn btn-primary",
                                r#type: "button",
                                onclick: move |_| {
                                    let (names, notice) = read_roster(&paste.read());
                                    state
                                        .edit_class(|class, _| {
                                            for name in names {
                                                class.add_student(name);
                                            }
                                        });
                                    if let Some(notice) = notice {
                                        state.set_notice(notice);
                                    }
                                    paste.set(String::new());
                                    pasting.set(false);
                                },
                                "Add these names"
                            }
                            button {
                                class: "btn",
                                r#type: "button",
                                onclick: move |_| pasting.set(false),
                                "Cancel"
                            }
                        }
                    }
                } else {
                    button {
                        class: "btn btn-small",
                        r#type: "button",
                        onclick: move |_| pasting.set(true),
                        "Paste a list"
                    }
                }
            }
        }
    }
}

#[component]
fn GroupControls() -> Element {
    let mut state = use_app();
    let (students, current) = state
        .with_class(|class| {
            (
                class.students().len(),
                class.grouping().map(|grouping| grouping.len()),
            )
        })
        .unwrap_or_default();
    let mut wanted = use_signal(|| current.unwrap_or(DEFAULT_GROUPS));
    // The roster can shrink after the teacher set the stepper, so the count is re-derived each render.
    let groups = GroupCount::nearest(wanted(), students).ok();
    let shown = groups.map_or(0, GroupCount::get);
    rsx! {
        section { class: "controls",
            div { class: "stepper", role: "group", aria_label: "Number of groups",
                button {
                    class: "btn stepper-btn",
                    r#type: "button",
                    aria_label: "Fewer groups",
                    disabled: shown <= 1,
                    onclick: move |_| wanted.set(shown - 1),
                    "\u{2212}"
                }
                div { class: "stepper-value",
                    span { class: "big", "{shown}" }
                    span { class: "unit",
                        if shown == 1 {
                            "group"
                        } else {
                            "groups"
                        }
                    }
                }
                button {
                    class: "btn stepper-btn",
                    r#type: "button",
                    aria_label: "More groups",
                    disabled: shown >= GroupCount::max_for(students),
                    onclick: move |_| wanted.set(shown + 1),
                    "+"
                }
            }
            if let Some(hint) = groups.and_then(|groups| size_hint(students, groups)) {
                span { class: "hint", "{hint}" }
            }
            button {
                class: "btn btn-primary",
                r#type: "button",
                disabled: groups.is_none(),
                onclick: move |_| {
                    if let Some(groups) = groups {
                        state.edit_class(|class, rng| class.regroup(groups, rng));
                    }
                },
                "Make groups"
            }
            if current.is_some() {
                button {
                    class: "btn",
                    r#type: "button",
                    onclick: move |_| {
                        platform::animate_shuffle();
                        state.edit_class(|class, rng| class.reshuffle(rng));
                    },
                    "Shuffle again"
                }
            }
        }
    }
}

/// "6 each" or "6 to 7 each", from the domain's own split so the hint matches what Make groups
/// will do.
fn size_hint(students: usize, groups: GroupCount) -> Option<String> {
    let sizes = group_sizes(students, groups);
    let (most, fewest) = (sizes.first()?, sizes.last()?);
    Some(if most == fewest {
        format!("{most} each")
    } else {
        format!("{fewest} to {most} each")
    })
}

#[component]
fn GroupBoard() -> Element {
    let state = use_app();
    let cards = use_memo(move || state.with_class(group_cards).unwrap_or_default());
    let mut moving = use_signal(|| None::<StudentId>);
    use_escape(move || {
        if moving.peek().is_some() {
            moving.set(None);
        }
    });
    // A selected student can be removed from the roster, which leaves `moving` pointing at no one.
    let selected = moving().and_then(|id| state.with_class(|class| selection(class, id)).flatten());
    rsx! {
        section { class: "board",
            if let Some(selected) = &selected {
                div { class: "moving-bar", role: "status",
                    span {
                        "Moving "
                        strong { "{selected.student.name}" }
                        ". Tap a group to place them."
                    }
                    button {
                        class: "btn btn-small",
                        r#type: "button",
                        onclick: move |_| moving.set(None),
                        "Cancel"
                    }
                }
            }
            div { class: "cards",
                for card in cards() {
                    GroupCardView {
                        key: "{card.idx.get()}",
                        card,
                        selected: selected.clone(),
                        moving,
                    }
                }
            }
        }
    }
}

#[component]
fn GroupCardView(
    card: GroupCard,
    selected: Option<Selection>,
    moving: Signal<Option<StudentId>>,
) -> Element {
    let mut state = use_app();
    let mut renaming = use_signal(|| false);
    let idx = card.idx;
    let selected_id = selected.as_ref().map(|selection| selection.student.id);
    // Only a student from another group can be moved here.
    let incoming = selected.as_ref().filter(|selection| selection.group != idx);
    rsx! {
        article {
            class: "group-card hue-{hue(idx)}",
            "data-target": "{incoming.is_some()}",
            onclick: move |_| {
                let Some(id) = selected_id else { return };
                log_failed("move a student", state.edit_class(|class, _| class.move_student(id, idx)));
                moving.set(None);
            },
            header { class: "card-head",
                if renaming() {
                    NameInput {
                        initial: card.name.to_string(),
                        label: "Group name",
                        on_commit: move |label| {
                            log_failed(
                                "rename a group",
                                state.edit_class(|class, _| class.rename_group(idx, label)),
                            );
                            renaming.set(false);
                        },
                        on_cancel: move |_| renaming.set(false),
                    }
                } else {
                    button {
                        class: "group-name",
                        r#type: "button",
                        title: "Click to rename",
                        onclick: move |event| {
                            // While a student is selected the whole card is a drop target, name included.
                            if selected_id.is_none() {
                                event.stop_propagation();
                                renaming.set(true);
                            }
                        },
                        "{card.name}"
                    }
                }
                button {
                    class: "icon-btn",
                    r#type: "button",
                    aria_label: "Pick a new name for {card.name}",
                    title: "Pick a new name",
                    onclick: move |event| {
                        event.stop_propagation();
                        log_failed(
                            "rename a group",
                            state.edit_class(|class, rng| class.reroll_group_name(idx, rng)),
                        );
                    },
                    DiceIcon {}
                }
            }
            ul { class: "chips",
                for (id , name) in card.members.iter().map(|student| (student.id, &student.name)) {
                    li { key: "{id.get()}",
                        button {
                            class: "chip",
                            r#type: "button",
                            "data-student": "{id.get()}",
                            aria_pressed: "{selected_id == Some(id)}",
                            onclick: move |event| {
                                event.stop_propagation();
                                moving.set(if selected_id == Some(id) { None } else { Some(id) });
                            },
                            "{name}"
                        }
                    }
                }
            }
            if card.members.is_empty() {
                p { class: "empty-card", "No one here yet" }
            }
            if let Some(selection) = incoming {
                button { class: "drop-hint", r#type: "button", "Move {selection.student.name} here" }
            }
        }
    }
}

#[component]
fn DiceIcon() -> Element {
    rsx! {
        svg {
            width: "24",
            height: "24",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            "aria-hidden": "true",
            rect {
                x: "3",
                y: "3",
                width: "18",
                height: "18",
                rx: "4",
            }
            for (cx , cy) in [("8.5", "8.5"), ("15.5", "8.5"), ("12", "12"), ("8.5", "15.5"), ("15.5", "15.5")] {
                circle {
                    cx,
                    cy,
                    r: "1.3",
                    fill: "currentColor",
                    stroke: "none",
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(students: usize, groups: usize) -> Option<String> {
        size_hint(students, GroupCount::new(groups, students).unwrap())
    }

    #[test]
    fn size_hint_names_one_size_when_the_split_is_even() {
        assert_eq!(hint(24, 4).as_deref(), Some("6 each"));
    }

    #[test]
    fn size_hint_gives_a_range_when_the_split_is_uneven() {
        assert_eq!(hint(25, 4).as_deref(), Some("6 to 7 each"));
    }
}
