//! Port of `Components/Session/QuestionForm.razor` — spec 07 §27, 02 §8. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// AskUserQuestion form (options + Other, multi), answers via session_answer with `ask_user::with_answers`.
#[component]
pub fn QuestionForm(
    session: SessionHandle,
    permission: PendingPermission,
) -> impl IntoView {
    // port: QuestionForm.razor markup + behaviour (placeholder root only)
    view! { <form class="ask"></form> }
}
