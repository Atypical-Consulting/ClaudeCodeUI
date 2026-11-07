//! Port of `Components/Session/CrashBanner.razor` — spec 07 §3. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Crashed / start failed banner with exit text and Restart.
#[component]
pub fn CrashBanner(
    session: SessionHandle,
) -> impl IntoView {
    // port: CrashBanner.razor markup + behaviour (placeholder root only)
    view! { <div class="crash" role="alert"></div> }
}
