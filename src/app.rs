//! Shell state. Everything here is a signal provided through context:
//!
//! - `doc` is the persisted domain, written only through [`AppState::edit`] and
//!   [`AppState::edit_class`], and saved by the one effect in [`App`]. Where it is saved follows
//!   from `session` and `remote`, through [`account::save_target`].
//! - `session` and `remote` say who is signed in and whether their classes on the server were
//!   read. The startup check sets them once, and a rejected session sets them again.
//! - `mode`, `notice`, `timer` and `now` are in memory. The timer lives here, not in the
//!   whiteboard, so it keeps running while the teacher is back in Build.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::logger::tracing::warn;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::account::{self, Booted, RemoteDoc, SaveTarget, Session};
use crate::api::ApiError;
use crate::domain::{Class, Doc, Millis, Timer};
use crate::notice::Notice;
use crate::outbox::{Edit, Outbox};
use crate::persist;
use crate::platform::{self, JsRng, LocalStorage};
use crate::store;
use crate::ui::avatar::AvatarSprite;
use crate::ui::build::BuildMode;
use crate::ui::whiteboard::WhiteboardMode;

const MAIN_CSS: Asset = asset!("/assets/main.css");

/// Display precision is whole seconds, and the timer stores a deadline, so a coarser tick would
/// only make the digits change late.
const TICK_MS: u32 = 250;

/// How long the doc must sit unchanged before it is sent to the server.
const REMOTE_SAVE_DEBOUNCE_MS: u32 = 800;

/// How often a teacher leaving the page checks whether the last request has returned.
const IN_FLIGHT_POLL_MS: u32 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Build,
    Whiteboard,
}

#[derive(Clone, Copy)]
pub struct AppState {
    doc: Signal<Doc>,
    mode: Signal<Mode>,
    notice: Signal<Option<Notice>>,
    timer: Signal<Timer>,
    now: Signal<Millis>,
    session: Signal<Session>,
    remote: Signal<RemoteDoc>,
}

impl AppState {
    /// Reactive read. Subscribes the calling component to the doc.
    pub fn with_doc<R>(&self, f: impl FnOnce(&Doc) -> R) -> R {
        f(&self.doc.read())
    }

    /// Reactive read of the selected class, `None` when no class exists.
    pub fn with_class<R>(&self, f: impl FnOnce(&Class) -> R) -> Option<R> {
        self.with_doc(|doc| doc.selected().map(f))
    }

    /// Never call this from inside `with_doc` or `with_class`: the read guard is still held.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Doc, &mut JsRng) -> R) -> R {
        f(&mut self.doc.write(), &mut JsRng)
    }

    /// `None`, and no write, when no class is selected.
    pub fn edit_class<R>(&mut self, f: impl FnOnce(&mut Class, &mut JsRng) -> R) -> Option<R> {
        self.doc.peek().selected()?;
        self.doc
            .write()
            .selected_mut()
            .map(|class| f(class, &mut JsRng))
    }

    pub fn mode(&self) -> Mode {
        *self.mode.read()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode.set(mode);
    }

    pub fn notice(&self) -> Option<Notice> {
        self.notice.read().clone()
    }

    /// Ignored when a notice the teacher cannot dismiss is showing and this one could be.
    pub fn set_notice(&mut self, notice: Notice) {
        let allowed = self
            .notice
            .peek()
            .as_ref()
            .is_none_or(|current| notice.may_replace(current));
        if allowed {
            self.notice.set(Some(notice));
        }
    }

    pub fn dismiss_notice(&mut self) {
        self.notice.set(None);
    }

    pub fn session(&self) -> Session {
        self.session.read().clone()
    }

    /// The server no longer knows this session. The page keeps its doc and saves it in this
    /// browser from now on, and says so.
    pub fn session_ended(&mut self) {
        self.session.set(Session::SignedOut);
        self.remote.set(RemoteDoc::Unread);
        self.save_succeeded();
        self.set_notice(Notice::SessionEnded);
    }

    fn save_target(&self, local_can_save: bool) -> SaveTarget {
        account::save_target(&self.session.read(), *self.remote.read(), local_can_save)
    }

    /// Takes what the startup check found. The doc comes first, so no save can run between the
    /// session turning signed in and the doc being the server's.
    fn apply_boot(&mut self, booted: Booted) {
        if booted.remote == RemoteDoc::Ready
            && self
                .notice
                .peek()
                .as_ref()
                .is_some_and(Notice::is_about_local_storage)
        {
            self.notice.set(None);
        }
        if let Some(doc) = booted.doc {
            self.doc.set(doc);
        }
        self.remote.set(booted.remote);
        self.session.set(booted.session);
        if let Some(notice) = booted.notice {
            self.set_notice(notice);
        }
    }

    /// A save that went through answers the banner about one that did not.
    fn save_succeeded(&mut self) {
        if matches!(*self.notice.peek(), Some(Notice::SaveFailed(_))) {
            self.notice.set(None);
        }
    }

    fn save_failed(&mut self, cause: String) {
        self.notice.set(Some(Notice::SaveFailed(cause)));
    }

    pub fn timer(&self) -> Timer {
        *self.timer.read()
    }

    /// Time left on the countdown. Only a running timer reads the clock, which changes four
    /// times a second, so a paused one does not redraw.
    pub fn remaining(&self) -> Millis {
        let timer = self.timer();
        let now = if timer.is_running() {
            *self.now.read()
        } else {
            Millis(0)
        };
        timer.remaining(now)
    }

    /// Applies a timer transition at a fresh clock reading, so a button press never acts on a
    /// `now` that is up to a tick old.
    pub fn update_timer(&mut self, f: impl FnOnce(Timer, Millis) -> Timer) {
        let now = platform::now();
        self.now.set(now);
        let next = f(*self.timer.peek(), now);
        self.timer.set(next);
    }

    pub fn cancel_timer(&mut self) {
        let next = self.timer.peek().cancel();
        self.timer.set(next);
    }

    fn tick(&mut self) {
        let now = platform::now();
        self.now.set(now);
        let timer = *self.timer.peek();
        let next = timer.tick(now);
        if next != timer {
            self.timer.set(next);
        }
    }
}

pub fn use_app() -> AppState {
    use_context::<AppState>()
}

/// The write queue for the teacher's account, shared by the save effect and whatever leaves the
/// page.
pub type SharedOutbox = Rc<RefCell<Outbox>>;

/// Sends the newest doc to the server once `edit` has settled.
async fn save_remotely(state: AppState, outbox: SharedOutbox, edit: Edit) {
    TimeoutFuture::new(REMOTE_SAVE_DEBOUNCE_MS).await;
    let first = outbox.borrow_mut().settled(edit);
    send_all(state, &outbox, first).await;
}

/// Sends the newest doc now and returns once no request is on the wire, for a teacher who is
/// about to sign out. Whether it all arrived is for [`Outbox::has_unsent`] to say.
pub async fn save_remotely_now(state: AppState, outbox: SharedOutbox) {
    let first = outbox.borrow_mut().settle_now();
    send_all(state, &outbox, first).await;
    while outbox.borrow().in_flight() {
        TimeoutFuture::new(IN_FLIGHT_POLL_MS).await;
    }
}

/// Sends `next`, then whatever edits settled while each request flew, one request at a time.
async fn send_all(mut state: AppState, outbox: &SharedOutbox, mut next: Option<String>) {
    while let Some(envelope) = next {
        next = match platform::api().put_doc(&envelope).await {
            Ok(()) => {
                state.save_succeeded();
                outbox.borrow_mut().finished(true)
            }
            Err(ApiError::Unauthorized) => {
                outbox.borrow_mut().halt();
                state.session_ended();
                None
            }
            Err(error) => {
                state.save_failed(error.to_string());
                outbox.borrow_mut().finished(false)
            }
        };
    }
}

/// Runs `handler` on every Escape press while the calling component is mounted. The first
/// render's closure is the one that stays, so it may capture only signals, which are `Copy`
/// handles to the current value.
pub fn use_escape(handler: impl FnMut() + 'static) {
    use_hook(|| match platform::on_escape(handler) {
        Ok(guard) => Some(Rc::new(guard)),
        Err(error) => {
            warn!("the Escape shortcut is unavailable: {error}");
            None
        }
    });
}

#[component]
pub fn App() -> Element {
    let initial = use_hook(|| Rc::new(store::load(&LocalStorage, &mut JsRng)));
    let doc = use_signal(|| initial.doc.clone());
    let notice = use_signal(|| initial.notice.clone());
    let state = use_context_provider(|| AppState {
        doc,
        mode: Signal::new(Mode::Build),
        notice,
        timer: Signal::new(Timer::default()),
        now: Signal::new(platform::now()),
        session: Signal::new(Session::Checking),
        remote: Signal::new(RemoteDoc::Unread),
    });
    use_hook(|| {
        if let Some(code) = platform::take_auth_error() {
            let mut state = state;
            state.set_notice(Notice::SignInFailed(code));
        }
    });

    let outbox = use_context_provider(|| Rc::new(RefCell::new(Outbox::default())));
    let local_can_save = initial.can_save;
    let saver = outbox.clone();
    use_effect(move || {
        let mut state = state;
        match state.save_target(local_can_save) {
            SaveTarget::Nowhere => {}
            SaveTarget::Local => match store::save(&LocalStorage, &doc.read()) {
                Ok(()) => state.save_succeeded(),
                Err(error) => state.save_failed(error.to_string()),
            },
            SaveTarget::Remote => {
                let edit = saver.borrow_mut().edited(persist::encode(&doc.read()));
                spawn(save_remotely(state, saver.clone(), edit));
            }
        }
    });

    let booter = outbox.clone();
    use_future(move || {
        let outbox = booter.clone();
        async move {
            let mut state = state;
            let local = doc.peek().clone();
            let mut booted = account::boot(&platform::api(), &local, &mut JsRng).await;
            outbox.borrow_mut().mark_synced(booted.synced.take());
            state.apply_boot(booted);
        }
    });

    use_future(move || async move {
        let mut state = state;
        loop {
            TimeoutFuture::new(TICK_MS).await;
            state.tick();
        }
    });

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        AvatarSprite {}
        div { class: "app",
            match state.mode() {
                Mode::Build => rsx! { BuildMode {} },
                Mode::Whiteboard => rsx! { WhiteboardMode {} },
            }
        }
    }
}
