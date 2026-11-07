//! Port of `Components/Shared/Ticker.razor` — spec 06 §7, 07 §0.4.8. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Re-renders its children once a second while `active` (set_interval driving a tick signal the children
/// read; cleared when inactive or unmounted).
#[component]
pub fn Ticker(
    #[prop(into)] active: Signal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    // port: Ticker.razor markup + behaviour (placeholder root only)
    view! { <span></span> }
}
