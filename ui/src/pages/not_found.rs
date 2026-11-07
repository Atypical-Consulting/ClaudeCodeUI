//! Port of `Components/Pages/NotFound.razor` — spec 06 §6.8. Owner: ui-shell.
//! Interactive navigation to an unknown route (the router fallback): no HTTP status re-execution any more.

use crate::common::status_screen::StatusScreen;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// Router fallback: StatusScreen 404 with the current path.
#[component]
pub fn NotFound() -> impl IntoView {
    let path = use_location().pathname.get_untracked();
    let path = format!("/{}", path.trim_start_matches('/'));
    view! { <StatusScreen code=404 path=path/> }
}
