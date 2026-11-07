//! Port of `Components/Session/PermissionCard.razor` — spec 07 §35. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// First pending request: QuestionForm | ElicitForm | plan | generic (allow / allow session + destination / deny
/// with message); armed keys (Enter / Shift+Enter / Delete).
#[component]
pub fn PermissionCard(
    session: SessionHandle,
    permission: PendingPermission,
) -> impl IntoView {
    // port: PermissionCard.razor markup + behaviour (placeholder root only)
    view! { <div class="ph"></div> }
}
