//! Port of `Components/Session/ApiErrorCard.razor` — spec 07 §2, 02 §13. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Synthetic API error: title, hint, raw (copyable), retry when the session can.
#[component]
pub fn ApiErrorCard(
    error: ccui_shared::api_errors::ApiError,
    #[prop(optional)] session: Option<SessionHandle>,
) -> impl IntoView {
    // port: ApiErrorCard.razor markup + behaviour (placeholder root only)
    view! { <div class="crash" role="alert"></div> }
}
