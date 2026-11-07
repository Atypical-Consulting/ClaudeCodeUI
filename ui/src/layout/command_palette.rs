//! Port of `Components/Layout/CommandPalette.razor` — spec 06 §5. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Ctrl K modal combobox: sessions, actions, message hits (search_transcripts, 250 ms debounce,
/// 3+ chars, cancellable). Rendered only while `palette_open`.
#[component]
pub fn CommandPalette() -> impl IntoView {
    // port: CommandPalette.razor markup + behaviour (placeholder root only)
    view! { <div class="palette" role="dialog" aria-modal="true" aria-label=t!("Rail.Search")></div> }
}

/// `Mark(s)`: (before, match, after) of the first case-insensitive match of the trimmed query (whitespace-collapsed
/// fallback), rendered as text + `<mark>` + text.
pub fn mark(s: &str, q: &str) -> (String, Option<String>, String) {
    todo!("port: CommandPalette.Mark")
}
