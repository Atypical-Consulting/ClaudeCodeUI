//! Port of `Components/Layout/RailSessionRow.razor` — spec 06 §4 RailSessionRow. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// A live session row (status dot, title/titling, branch, pill/state/cost).
#[component]
pub fn RailSessionRow(
    session: ccui_shared::session::SessionSummary,
) -> impl IntoView {
    // port: RailSessionRow.razor markup + behaviour (placeholder root only)
    view! { <a class="s"></a> }
}

/// A past session row: click opens the transcript (session_open) then navigates.
#[component]
pub fn RailPastRow(
    past: ccui_shared::api::PastSession,
) -> impl IntoView {
    // port: RailSessionRow.razor markup + behaviour (placeholder root only)
    view! { <a class="s"></a> }
}

/// `Dot(s)`: Waiting → wait, Running → run, Crashed → err, Exited → exited, else idle (reused by palette, Overview).
pub fn dot(status: SessionStatus) -> &'static str {
    todo!("port: RailSessionRow.Dot")
}

/// `Where(branch, repo)` text: non-empty of [branch unless "HEAD", repo] joined " · " (None when empty).
pub fn place(branch: Option<&str>, repo: Option<&str>) -> Option<String> {
    todo!("port: RailSessionRow.Where")
}
