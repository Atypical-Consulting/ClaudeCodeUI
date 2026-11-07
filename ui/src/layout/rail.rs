//! Port of `Components/Layout/Rail.razor` — spec 06 §4. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Brand, actions (new, search, overview, worktrees tag, background pill, extensions pill, settings), live
/// sessions grouped by repo, recent (8, throttled 30 s / generation), QuotaMeter, Mascot. Starts `bg_watch` on mount.
#[component]
pub fn Rail() -> impl IntoView {
    // port: Rail.razor markup + behaviour (placeholder root only)
    view! { <nav class="pane" aria-label=t!("Rail.Label")></nav> }
}
