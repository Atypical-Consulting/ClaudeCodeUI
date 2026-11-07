//! Port of `Components/Shared/Duck.razor (BlazorKawaii RubberDuck)` — spec 06 §7. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// The kawaii rubber duck SVG (viewBox 0 0 240 240, colour #FCCC0A). Reference renders:
/// .impeccable/mockups/inputs/kawaii/duck-*.html.
#[component]
pub fn Duck(
    mood: Mood,
    #[prop(default = 48)] size: u32,
) -> impl IntoView {
    // port: Duck.razor markup + behaviour (placeholder root only)
    view! { <svg viewBox="0 0 240 240"></svg> }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Happy,
    Excited,
    Blissful,
    Sad,
    Shocked,
    Ko,
    Sleepy,
    Dizzy,
}
