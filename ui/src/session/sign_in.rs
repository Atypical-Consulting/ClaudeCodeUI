//! Port of `Components/Session/SignIn.razor` — spec 07 §8. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Claude.ai sign-in flow (session_start_sign_in → link → session_finish_sign_in).
#[component]
pub fn SignIn(
    session: SessionHandle,
    #[prop(optional)] primary: bool,
) -> impl IntoView {
    // port: SignIn.razor markup + behaviour (placeholder root only)
    view! { <p role="status"></p> }
}
