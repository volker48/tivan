//! Shell state. Everything here is a signal provided through context:
//!
//! - `doc` is the persisted domain, written only through [`AppState::edit`] and
//!   [`AppState::edit_class`], and saved by the one effect in [`App`].
//! - `mode`, `notice`, `timer` and `now` are in memory. The timer lives here, not in the
//!   whiteboard, so it keeps running while the teacher is back in Build.

use std::rc::Rc;

use dioxus::logger::tracing::warn;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::domain::{Class, Doc, Millis, Timer};
use crate::notice::Notice;
use crate::platform::{self, JsRng, LocalStorage};
use crate::store;
use crate::ui::build::BuildMode;
use crate::ui::whiteboard::WhiteboardMode;

const MAIN_CSS: Asset = asset!("/assets/main.css");

/// Display precision is whole seconds, and the timer stores a deadline, so a coarser tick would
/// only make the digits change late.
const TICK_MS: u32 = 250;

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

    pub fn set_notice(&mut self, notice: Notice) {
        self.notice.set(Some(notice));
    }

    pub fn dismiss_notice(&mut self) {
        self.notice.set(None);
    }

    pub fn timer(&self) -> Timer {
        *self.timer.read()
    }

    /// Reactive, and changes four times a second. Read it only where a countdown is drawn.
    pub fn now(&self) -> Millis {
        *self.now.read()
    }

    /// Applies a timer transition at a fresh clock reading, so a button press never acts on a
    /// `now` that is up to a tick old.
    pub fn update_timer(&mut self, f: impl FnOnce(Timer, Millis) -> Timer) {
        let now = platform::now();
        self.now.set(now);
        let next = f(*self.timer.peek(), now);
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
    let initial = use_hook(|| Rc::new(store::load(&LocalStorage)));
    let doc = use_signal(|| initial.doc.clone());
    let mut notice = use_signal(|| initial.notice.clone());
    let state = use_context_provider(|| AppState {
        doc,
        mode: Signal::new(Mode::Build),
        notice,
        timer: Signal::new(Timer::default()),
        now: Signal::new(platform::now()),
    });

    let can_save = initial.can_save;
    use_effect(move || {
        if !can_save {
            return;
        }
        match store::save(&LocalStorage, &doc.read()) {
            Ok(()) => {
                if matches!(*notice.peek(), Some(Notice::SaveFailed(_))) {
                    notice.set(None);
                }
            }
            Err(error) => notice.set(Some(Notice::SaveFailed(error.to_string()))),
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
        div { class: "app",
            match state.mode() {
                Mode::Build => rsx! { BuildMode {} },
                Mode::Whiteboard => rsx! { WhiteboardMode {} },
            }
        }
    }
}
