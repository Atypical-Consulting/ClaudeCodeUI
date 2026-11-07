//! Port of `Components/Layout/Mascot.razor` — spec 06 §4 Mascot. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// The duck and one status line, priority: decisions, background needs, running, crashed, turn done
/// (5 min), calm.
#[component]
pub fn Mascot() -> impl IntoView {
    // port: Mascot.razor markup + behaviour (placeholder root only)
    view! { <div class="mascot"></div> }
}
