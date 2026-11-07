//! Port of `Components/Session/SlashPopover.razor` — spec 07 §12. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Slash command list (initialize_info.commands ∪ init slash_commands) filtered by the query; keyboard via ctl.
#[component]
pub fn SlashPopover(
    session: SessionHandle,
    #[prop(into)] query: Signal<Option<String>>,
    on_pick: Callback<String>,
    ctl: super::PopoverCtl,
) -> impl IntoView {
    // port: SlashPopover.razor markup + behaviour (placeholder root only)
    view! { <div class="pop slash"></div> }
}
