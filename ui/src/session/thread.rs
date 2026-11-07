//! Port of `Components/Session/Thread.razor` — spec 07 §33, 02 §7. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// The log: `session.with_items(|it| thread_blocks::of(it, hide_thoughts))` memo (index blocks; rows read
/// `session.tool_at(i)` / `row(i)`), timestamps / day dividers (`fmt::long_date`, flag),
/// bookmarks, fork from here, rewind; StreamingTail; CrashBanner; dock (jump + StatusLine); RewindDialog.
#[component]
pub fn Thread(
    session: SessionHandle,
) -> impl IntoView {
    // port: Thread.razor markup + behaviour (placeholder root only)
    view! { <div class="thread"></div> }
}
