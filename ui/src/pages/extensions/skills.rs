//! Port of `Components/Pages/Extensions.razor (Skills tab)` — spec 06 §6.7. Owner: ui-extensions.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Skills table (init or reload_skills) with FindBox.
#[component]
pub fn SkillsTab(
    session: SessionHandle,
) -> impl IntoView {
    // port: Extensions.razor markup + behaviour (placeholder root only)
    view! { <table class="mcp"></table> }
}
