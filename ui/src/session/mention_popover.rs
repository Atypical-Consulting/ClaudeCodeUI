//! Port of `Components/Session/MentionPopover.razor` — spec 07 §23, 04 §3. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// @ mentions: file_index_scan once per cwd, `file_index::filter` per query change, agents first.
#[component]
pub fn MentionPopover(
    session: SessionHandle,
    #[prop(into)] query: Signal<Option<String>>,
    on_pick: Callback<String>,
    ctl: super::PopoverCtl,
) -> impl IntoView {
    // port: MentionPopover.razor markup + behaviour (placeholder root only)
    view! { <div class="pop slash"></div> }
}
