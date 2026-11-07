//! Port of `Components/Shared/PickFolder.razor` — spec 06 §7, 04 §2. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Folder button: busy while the native dialog is open (ipc::pick_folder); cancel changes nothing.
#[component]
pub fn PickFolder(
    #[prop(optional, into)] start: Option<Signal<String>>,
    #[prop(optional, into)] disabled: Option<Signal<bool>>,
    on_pick: Callback<String>,
    on_error: Callback<String>,
) -> impl IntoView {
    // port: PickFolder.razor markup + behaviour (placeholder root only)
    view! { <button type="button" class="pickdir"></button> }
}
