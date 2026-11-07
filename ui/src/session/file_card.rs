//! Port of `Components/Session/FileCard.razor` — spec 07 §9. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// SendUserFile card: files (reveal_path), caption.
#[component]
pub fn FileCard(
    session: SessionHandle,
    tool: ToolItem,
) -> impl IntoView {
    // port: FileCard.razor markup + behaviour (placeholder root only)
    view! { <div class="filecard"></div> }
}
