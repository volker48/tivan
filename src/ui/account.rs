//! The Build top bar's account controls and the Classroom sync panel. Signing in is a plain link
//! to the server, which sends the teacher to Google. Everything after that is a handler that
//! calls the API and then one domain function.

use dioxus::prelude::*;

use crate::account::Session;
use crate::api::ApiError;
use crate::app::{AppState, SharedOutbox, save_remotely_now, use_app};
use crate::domain::{CourseStatus, SyncPlan, SyncReport};
use crate::notice::{Notice, count, sync_summary};
use crate::platform;

/// The sync panel's progress. It lives in memory only, so leaving Build closes it.
#[derive(Clone, Debug)]
pub enum SyncState {
    Closed,
    /// Waiting for Classroom, which reads every roster and can take a few seconds.
    Loading,
    /// The courses are in and the teacher is choosing which new ones to add.
    Picking(SyncPlan),
    Done(SyncReport),
    /// Holds what to tell the teacher.
    Failed(String),
}

/// "Sign in with Google" for a visitor, or the teacher's name with Sync Classroom and Sign out.
/// Nothing while the page is still finding out who is signed in.
#[component]
pub fn AccountBar(sync: Signal<SyncState>) -> Element {
    let mut state = use_app();
    let outbox = use_context::<SharedOutbox>();
    let mut signing_out = use_signal(|| false);
    let session = state.session();
    let chip = session.short_name().unwrap_or_default().to_owned();
    match session {
        Session::Checking => rsx! {},
        Session::SignedOut => rsx! {
            div { class: "account",
                a { class: "btn btn-small", href: "/auth/login", "Sign in with Google" }
            }
        },
        Session::SignedIn { email, .. } => rsx! {
            div { class: "account",
                span { class: "account-chip", title: "{email}", "{chip}" }
                button {
                    class: "btn btn-small",
                    r#type: "button",
                    disabled: matches!(sync(), SyncState::Loading),
                    onclick: {
                        let outbox = outbox.clone();
                        move |_| look_up_courses(state, sync, outbox.clone())
                    },
                    "Sync Classroom"
                }
                button {
                    class: "btn btn-small",
                    r#type: "button",
                    disabled: signing_out(),
                    onclick: move |_| {
                        signing_out.set(true);
                        let outbox = outbox.clone();
                        spawn(async move {
                            save_remotely_now(state, outbox.clone()).await;
                            if outbox.borrow().has_unsent() {
                                signing_out.set(false);
                                return;
                            }
                            match platform::api().logout().await {
                                Ok(()) | Err(ApiError::Unauthorized) => platform::reload(),
                                Err(error) => {
                                    signing_out.set(false);
                                    state.set_notice(Notice::SignOutFailed(error.to_string()));
                                }
                            }
                        });
                    },
                    "Sign out"
                }
            }
        },
    }
}

fn look_up_courses(mut state: AppState, mut sync: Signal<SyncState>, outbox: SharedOutbox) {
    sync.set(SyncState::Loading);
    spawn(async move {
        match platform::api().courses().await {
            Ok(courses) => {
                let plan = state.with_doc(|doc| SyncPlan::new(doc, courses));
                sync.set(SyncState::Picking(plan));
            }
            Err(ApiError::Unauthorized) => {
                sync.set(SyncState::Closed);
                state.session_ended();
            }
            Err(ApiError::Reauthorize) => {
                // Leaving the page cancels a save still waiting or on the wire.
                save_remotely_now(state, outbox.clone()).await;
                if outbox.borrow().has_unsent() {
                    sync.set(SyncState::Failed(
                        "Your latest changes are not saved yet, so Tivan did not leave for Google. \
                         Try again in a moment."
                            .to_owned(),
                    ));
                } else {
                    platform::go_to_login();
                }
            }
            Err(ApiError::Failed(cause)) => sync.set(SyncState::Failed(format!(
                "Google Classroom could not be reached ({cause}). Try again in a moment."
            ))),
        }
    });
}

/// Lists the teacher's Classroom classes. New ones start checked, ones already in Tivan are
/// marked and always refreshed, and Apply brings the checked ones in.
#[component]
pub fn SyncPanel(sync: Signal<SyncState>) -> Element {
    let mut state = use_app();
    if !matches!(state.session(), Session::SignedIn { .. }) {
        return rsx! {};
    }
    let close = move |_| sync.set(SyncState::Closed);
    match sync() {
        SyncState::Closed => rsx! {},
        SyncState::Loading => rsx! {
            section { class: "panel sync-panel", role: "status",
                p { "Looking up your Classroom classes\u{2026}" }
            }
        },
        SyncState::Failed(message) => rsx! {
            section { class: "panel sync-panel",
                p { class: "field-error", role: "alert", "{message}" }
                div { class: "form-actions",
                    button { class: "btn", r#type: "button", onclick: close, "Close" }
                }
            }
        },
        SyncState::Done(report) => rsx! {
            section { class: "panel sync-panel",
                p { class: "sync-result", role: "status", "{sync_summary(&report)}" }
                div { class: "form-actions",
                    button { class: "btn btn-primary", r#type: "button", onclick: close, "Done" }
                }
            }
        },
        SyncState::Picking(plan) => {
            let entries = plan.entries();
            rsx! {
            section { class: "panel sync-panel",
                h2 { "Sync Classroom" }
                if entries.is_empty() {
                    p { class: "hint",
                        "No active Classroom classes found. Only classes you teach are listed."
                    }
                }
                ul { class: "sync-list",
                    for entry in entries {
                        li { key: "{entry.id.as_str()}", class: "sync-item",
                            match entry.status {
                                CourseStatus::New { chosen } => rsx! {
                                    label { class: "sync-choice",
                                        input {
                                            r#type: "checkbox",
                                            checked: chosen,
                                            onchange: {
                                                let id = entry.id.clone();
                                                move |event: FormEvent| {
                                                    let checked = event.checked();
                                                    sync.with_mut(|current| {
                                                        if let SyncState::Picking(plan) = current {
                                                            plan.set_chosen(&id, checked);
                                                        }
                                                    });
                                                }
                                            },
                                        }
                                        span { class: "sync-name", "{entry.name}" }
                                        span { class: "meta", "{count(entry.students, \"student\")}" }
                                    }
                                },
                                CourseStatus::Linked => rsx! {
                                    div { class: "sync-choice",
                                        span { class: "sync-name", "{entry.name}" }
                                        span { class: "meta",
                                            "{count(entry.students, \"student\")}, already in Tivan, refreshed"
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
                div { class: "form-actions",
                    button {
                        class: "btn btn-primary",
                        r#type: "button",
                        onclick: move |_| {
                            let current = sync.peek().clone();
                            if let SyncState::Picking(plan) = current {
                                let report = state.edit(|doc, rng| plan.apply(doc, rng));
                                sync.set(SyncState::Done(report));
                            }
                        },
                        "Apply"
                    }
                    button { class: "btn", r#type: "button", onclick: close, "Cancel" }
                }
            }
            }
        }
    }
}
