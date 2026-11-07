//! Port of `Components/Pages/Extensions.razor (Plugins tab)` — spec 06 §6.7, 05 §1. Owner: ui-extensions.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Installed / catalogue / marketplaces, act (install, enable…, accept-command flow), options inspector,
/// reload the session after a change.
#[component]
pub fn PluginsTab(
    #[prop(into)] session: Signal<Option<SessionHandle>>,
    #[prop(into)] cwd: Signal<String>,
) -> impl IntoView {
    // port: Extensions.razor markup + behaviour (placeholder root only)
    view! { <div class="ext-plugins"></div> }
}
