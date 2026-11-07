//! Port of `Components/Session/RewindDialog.razor` — spec 07 §18. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Native modal: preview (session_preview_rewind), rewind conversation [+ files]. Opens on `target`.
#[component]
pub fn RewindDialog(
    session: SessionHandle,
    #[prop(into)] target: Signal<Option<UserItem>>,
) -> impl IntoView {
    // port: RewindDialog.razor markup + behaviour (placeholder root only)
    view! { <dialog class="modal"></dialog> }
}
