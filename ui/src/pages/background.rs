//! Port of `Components/Pages/Background.razor` — spec 06 §6.5. Owner: ui-pages-a.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// `claude --bg` agents table (bg_list every 15 s, stable order), logs/stop/respawn/rm with armed confirm.
#[component]
pub fn Background() -> impl IntoView {
    // port: Background.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
