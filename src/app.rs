//! Shell state. Everything here is a signal provided through context:
//!
//! - `doc` is the persisted domain, written only through [`AppState::edit`] and
//!   [`AppState::edit_class`], and saved by the one effect in [`App`].
//! - `mode`, `notice`, `timer`, `now` and `today` are in memory. The timer lives here, not in the
//!   whiteboard, so it keeps running while the teacher is back in Build.

use std::rc::Rc;

use dioxus::logger::tracing::warn;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::domain::{CalendarDay, Class, Doc, Millis, Skin, Timer};
use crate::notice::Notice;
use crate::platform::{self, JsRng, LocalStorage};
use crate::store;
use crate::ui::avatar::AvatarSprite;
use crate::ui::build::BuildMode;
use crate::ui::settings::SettingsMode;
use crate::ui::whiteboard::WhiteboardMode;

const MAIN_CSS: Asset = asset!("/assets/main.css");

/// Display precision is whole seconds, and the timer stores a deadline, so a coarser tick would
/// only make the digits change late.
const TICK_MS: u32 = 250;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Build,
    Whiteboard,
    Settings,
}

#[derive(Clone, Copy)]
pub struct AppState {
    doc: Signal<Doc>,
    mode: Signal<Mode>,
    notice: Signal<Option<Notice>>,
    timer: Signal<Timer>,
    now: Signal<Millis>,
    today: Signal<CalendarDay>,
    skin: Memo<Option<Skin>>,
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

    /// Reactive read of the skin to draw on every avatar. A memo, so avatars wake only when the
    /// skin itself changes and not on every document edit.
    pub fn active_skin(&self) -> Option<Skin> {
        (self.skin)()
    }

    /// Reactive read. Changes at midnight, so a tab left open across a season's edge follows it.
    pub fn today(&self) -> CalendarDay {
        *self.today.read()
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
        let today = platform::today();
        if today != *self.today.peek() {
            self.today.set(today);
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
    let initial = use_hook(|| Rc::new(store::load(&LocalStorage, &mut JsRng)));
    let doc = use_signal(|| initial.doc.clone());
    let mut notice = use_signal(|| initial.notice.clone());
    let today = use_signal(platform::today);
    let skin = use_memo(move || doc.read().settings().active_skin(*today.read()));
    let state = use_context_provider(|| AppState {
        doc,
        mode: Signal::new(Mode::Build),
        notice,
        timer: Signal::new(Timer::default()),
        now: Signal::new(platform::now()),
        today,
        skin,
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
        AvatarSprite {}
        div { class: "app",
            match state.mode() {
                Mode::Build => rsx! { BuildMode {} },
                Mode::Whiteboard => rsx! { WhiteboardMode {} },
                Mode::Settings => rsx! { SettingsMode {} },
            }
        }
    }
}
