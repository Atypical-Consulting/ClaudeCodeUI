//! Port of `Components/Session/HookActivity.razor` — spec 07 §7. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Consecutive shown hooks (`<details>` open when one failed/blocked).
#[component]
pub fn HookActivity(
    hooks: Vec<HookItem>,
) -> impl IntoView {
    // port: HookActivity.razor markup + behaviour (placeholder root only)
    view! { <details class="hooks"></details> }
}
