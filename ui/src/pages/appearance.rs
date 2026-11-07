//! Port of `Components/Pages/Appearance.razor` — spec 06 §6.4. Owner: ui-pages-a.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Language, theme cards, code size, notifications, thinking summaries, timestamps, version + update check.
#[component]
pub fn Appearance() -> impl IntoView {
    // port: Appearance.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
