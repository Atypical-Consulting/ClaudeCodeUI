//! Port of `Components/Session/ConfigMenu.razor` — spec 07 §16. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// `/config key=value` menu from `session_rules::config_keys`.
#[component]
pub fn ConfigMenu(
    session: SessionHandle,
    on_close: Callback<()>,
) -> impl IntoView {
    // port: ConfigMenu.razor markup + behaviour (placeholder root only)
    view! { <div class="pop slash menu" role="dialog" tabindex="-1"></div> }
}
