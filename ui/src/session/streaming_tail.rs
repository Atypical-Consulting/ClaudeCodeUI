//! Port of `Components/Session/StreamingTail.razor` — spec 07 §11. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Streaming text split on blank lines (finished paragraphs untouched) + caret; scroll_end after each render.
#[component]
pub fn StreamingTail(
    session: SessionHandle,
) -> impl IntoView {
    // port: StreamingTail.razor markup + behaviour (placeholder root only)
    view! { <div class="prose md" aria-hidden="true"></div> }
}
