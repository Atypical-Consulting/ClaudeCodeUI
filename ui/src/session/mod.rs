//! `Components/Session/*` — the session page's 36 components (spec 07). Two owners:
//! ui-session-thread-composer (thread, composer, header, menus/popovers, todo, status line) and
//! ui-session-inspector (inspector panels, permission forms, tasks, memory, extra dirs).
//! Every component takes `session: SessionHandle` (Copy store of signals, see state.rs) and reads it reactively.

pub mod agent_panel;
pub mod api_error_card;
pub mod composer;
pub mod config_menu;
pub mod context_panel;
pub mod crash_banner;
pub mod diff_panel;
pub mod elicit_form;
pub mod extra_dirs;
pub mod file_card;
pub mod hook_activity;
pub mod inspector;
pub mod ledger;
pub mod memory_panel;
pub mod mention_popover;
pub mod mode_menu;
pub mod model_menu;
pub mod permission_card;
pub mod question_form;
pub mod rewind_dialog;
pub mod session_header;
pub mod session_title;
pub mod sign_in;
pub mod slash_popover;
pub mod status_line;
pub mod status_panel;
pub mod streaming_tail;
pub mod task_actions;
pub mod tasks_panel;
pub mod thread;
pub mod todo_panel;
pub mod tool_detail;
pub mod tool_row;
pub mod workflow;
pub mod workflow_card;
pub mod workflow_panel;

use leptos::prelude::*;

/// The public surface the Composer drives on its popovers (`open`, `active_id`, `move(d)`, `pick()` in Razor):
/// the popover writes `open` / `active_id` (for aria-activedescendant), the Composer writes `key`
/// ("ArrowUp" / "ArrowDown" / "Home" / "End" / "Enter" / "Tab", with a sequence number).
/// Owner: ui-session-thread-composer.
#[derive(Clone, Copy)]
pub struct PopoverCtl {
    pub open: RwSignal<bool>,
    pub active_id: RwSignal<Option<String>>,
    pub key: RwSignal<Option<(u64, String)>>,
}

impl PopoverCtl {
    pub fn new() -> PopoverCtl {
        PopoverCtl { open: RwSignal::new(false), active_id: RwSignal::new(None), key: RwSignal::new(None) }
    }

    /// The Composer relays a key to the open popover (sequenced: the same key twice fires twice).
    pub fn press(&self, key: &str) {
        self.key.update(|k| {
            let n = k.as_ref().map_or(1, |(n, _)| n + 1);
            *k = Some((n, key.to_string()));
        });
    }

    /// Popover side: run `f` for every key pressed after this call (a key left from an earlier popover is skipped).
    pub(crate) fn on_key(&self, f: impl Fn(&str) + 'static) {
        let key = self.key;
        let seen = StoredValue::new(key.get_untracked().map_or(0, |(n, _)| n));
        Effect::new(move |_| {
            let Some((n, k)) = key.get() else { return };
            if n <= seen.get_value() {
                return;
            }
            seen.set_value(n);
            f(&k);
        });
    }

    /// Popover side: on unmount, nothing is open any more (no stale aria-activedescendant).
    pub(crate) fn reset_on_cleanup(&self) {
        let (open, active) = (self.open, self.active_id);
        on_cleanup(move || {
            let _ = open.try_set(false);
            let _ = active.try_set(None);
        });
    }
}

impl Default for PopoverCtl {
    fn default() -> Self {
        PopoverCtl::new()
    }
}

/// The current time, moved once a second while `active` (the Ticker's job, but read only where a duration is
/// printed, so the elements around it are not rebuilt every second: focus and live regions stay put).
pub(crate) fn clock(active: Signal<bool>) -> Signal<ccui_shared::model::Time> {
    let now = RwSignal::new(chrono::Utc::now());
    let timer: StoredValue<Option<IntervalHandle>> = StoredValue::new(None);
    let stop = move || {
        if let Some(h) = timer.try_update_value(Option::take).flatten() {
            h.clear();
        }
    };
    Effect::new(move |_| {
        let on = active.get();
        stop();
        now.set(chrono::Utc::now());
        if on {
            let tick = move || {
                let _ = now.try_set(chrono::Utc::now());
            };
            if let Ok(h) = set_interval_with_handle(tick, std::time::Duration::from_secs(1)) {
                timer.set_value(Some(h));
            }
        }
    });
    on_cleanup(stop);
    now.into()
}

/// The session cwd as a memo: the meta is replaced on every patch, and rows that print a path relative to the cwd
/// must not recompute it 20 times a second. Shared through the Thread's context; else a memo of its own.
#[derive(Clone, Copy)]
pub struct Cwd(pub Memo<String>);

pub(crate) fn cwd(session: crate::state::SessionHandle) -> Memo<String> {
    use_context::<Cwd>().map(|c| c.0).unwrap_or_else(|| Memo::new(move |_| session.meta.with(|m| m.cwd.clone())))
}
