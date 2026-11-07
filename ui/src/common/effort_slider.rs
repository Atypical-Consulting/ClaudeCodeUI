//! Port of `Components/Shared/EffortSlider.razor` — spec 06 §7. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Range input + tick buttons over `levels`; drag state; `on_change(level)` on release.
#[component]
pub fn EffortSlider(
    levels: Vec<String>,
    #[prop(into)] value: Signal<String>,
    on_change: Callback<String>,
    name: Callback<String, String>,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    // port: EffortSlider.razor markup + behaviour (placeholder root only)
    view! { <span class="effort-slider"></span> }
}
