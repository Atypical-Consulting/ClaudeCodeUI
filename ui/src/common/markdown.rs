//! Port of `Components/Shared/Markdown.razor` — spec 06 §7, 05 §5. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// `<div class="prose md">` with `md::render(text)` set once via inner_html, cached per (language, text).
#[component]
pub fn Markdown(
    text: String,
) -> impl IntoView {
    // port: Markdown.razor markup + behaviour (placeholder root only)
    view! { <div class="prose md"></div> }
}
