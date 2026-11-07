//! Port of `Components/Session/ModeMenu.razor` — spec 07 §14. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Permission mode menu (MODES, risky confirm when not isolated).
#[component]
pub fn ModeMenu(
    session: SessionHandle,
    on_close: Callback<()>,
    on_escape: Callback<()>,
) -> impl IntoView {
    // port: ModeMenu.razor markup + behaviour (placeholder root only)
    view! { <div class="pop slash menu"></div> }
}
