//! Port of `Components/Pages/Extensions.razor (Hooks tab)` — spec 06 §6.7, 04 §8. Owner: ui-extensions.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Hook listing (session_hooks), add/edit/remove form via cli_edit_hook, post-write poll 500 ms × 20.
#[component]
pub fn HooksTab(
    session: SessionHandle,
) -> impl IntoView {
    // port: Extensions.razor markup + behaviour (placeholder root only)
    view! { <div class="ext-hooks"></div> }
}
