//! Port of `Components/Session/Thread.razor` — spec 07 §33, 02 §7. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// The log: `thread_blocks::of(items)` memo (thoughts filtered when hidden), timestamps/day dividers (flag),
/// bookmarks, fork from here, rewind; StreamingTail; CrashBanner; dock (jump + StatusLine); RewindDialog.
#[component]
pub fn Thread(
    session: SessionHandle,
) -> impl IntoView {
    // port: Thread.razor markup + behaviour (placeholder root only)
    view! { <div class="thread"></div> }
}
