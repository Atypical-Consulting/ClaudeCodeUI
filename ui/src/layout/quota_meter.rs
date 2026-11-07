//! Port of `Components/Layout/QuotaMeter.razor` — spec 06 §4 QuotaMeter. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// 5 h / 7 d windows, per-model rows, credits, warning, weekly breakdown (refresh_usage(behaviors)).
/// Only when the summary has limits.
#[component]
pub fn QuotaMeter() -> impl IntoView {
    // port: QuotaMeter.razor markup + behaviour (placeholder root only)
    view! { <div class="quota"></div> }
}

/// `W(f)` = round(clamp(f, 0, 1) * 100) + "%".
pub fn width(f: f64) -> String {
    todo!("port: QuotaMeter.W")
}
