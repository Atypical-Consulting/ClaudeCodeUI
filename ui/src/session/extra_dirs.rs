//! Port of `Components/Session/ExtraDirs.razor` — spec 07 §20. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// `--add-dir` list + input (validate with `session_rules::extra_dir`); used by NewSession and ContextPanel.
/// The parent reports an async add failure through `error`.
#[component]
pub fn ExtraDirs(
    #[prop(into)] id: String,
    #[prop(into)] dirs: Signal<Vec<String>>,
    #[prop(into)] cwd: Signal<String>,
    on_add: Callback<String>,
    #[prop(optional)] on_remove: Option<Callback<String>>,
    #[prop(optional, into)] disabled: Option<Signal<bool>>,
    #[prop(optional, into)] error: Option<Signal<Option<String>>>,
    #[prop(optional, into)] confirm: Option<String>,
) -> impl IntoView {
    // port: ExtraDirs.razor markup + behaviour (placeholder root only)
    view! { <ul class="dirs" aria-label=t!("Dirs.Label")></ul> }
}
