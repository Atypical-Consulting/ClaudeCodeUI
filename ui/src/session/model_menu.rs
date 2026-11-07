//! Port of `Components/Session/ModelMenu.razor` — spec 07 §30. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Models (initialize_info.models), effort (capped), advisor, fast, thinking, make default.
#[component]
pub fn ModelMenu(
    session: SessionHandle,
    on_close: Callback<()>,
    on_escape: Callback<()>,
) -> impl IntoView {
    // port: ModelMenu.razor markup + behaviour (placeholder root only)
    view! { <div class="pop slash menu"></div> }
}

/// The current model entry of initialize_info.models (`ModelMenu.Current`): (value, displayName).
pub fn current(meta: &ccui_shared::session::SessionMeta) -> Option<(String, String)> {
    todo!("port: ModelMenu.Current")
}
