//! Port of `Components/Session/Composer.razor` — spec 07 §36. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Prompt box: draft (session_set_draft debounced), images (images::from in wasm), queued chips, menus/popovers,
/// slash commands, history recall (PromptHistory + past_prompts), Invisible strip notice, send / stop, meta row.
#[component]
pub fn Composer(
    session: SessionHandle,
) -> impl IntoView {
    // port: Composer.razor markup + behaviour (placeholder root only)
    view! { <section class="composer rich" aria-label=t!("Composer.Label")></section> }
}
