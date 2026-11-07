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
}
