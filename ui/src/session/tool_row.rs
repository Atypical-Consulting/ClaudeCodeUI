//! Port of `Components/Session/ToolRow.razor` — spec 07 §19. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// One tool call row (label, target, state, meta); selects it in the inspector.
#[component]
pub fn ToolRow(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)] tool: Signal<ToolItem>,
) -> impl IntoView {
    // port: ToolRow.razor markup + behaviour (placeholder root only)
    view! { <button type="button" class="t"></button> }
}

/// The right-hand slot of a tool row (also the subagent rows of Workflow / AgentPanel).
#[derive(Clone, Debug, PartialEq)]
pub enum ToolMeta {
    /// `<span class="m">{0}</span>` (Waiting label, "…" while Running, the finished meta).
    Plain(String),
    /// `<span class="badge {class}" title={title}>{text}</span>`: class "no" (Denied, Error), "dim" (NotRun, title
    /// `Tool.NotRunWhy`), "ok" (Write that created the file).
    Badge { class: &'static str, text: String, title: Option<String> },
}

/// `ToolRow.Meta(tool)` by state; the finished meta (lines / results / +n −m / answers…) is computed once per
/// (tool, ended_at) — cache it (a Memo per row keyed on state + ended_at). None = empty slot.
pub fn meta(tool: &ToolItem) -> Option<ToolMeta> {
    todo!("port: ToolRow.Meta")
}

/// Render a `ToolMeta` (nothing for None).
#[component]
pub fn ToolMetaView(#[prop(into)] meta: Signal<Option<ToolMeta>>) -> impl IntoView {
    // port: ToolRow.Meta markup (placeholder)
    view! { <span class="m"></span> }
}
