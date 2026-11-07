//! Port of `Components/Session/Inspector.razor` — spec 07 §10. Owner: ui-session-inspector.
//! The .pane.insp column: the pending permission (shown even when Ctrl I hid the pane: it blocks the turn), then the
//! background tasks if any, then the selected agent or tool, else the uncommitted changes (/diff) or the context.

use super::agent_panel::AgentPanel;
use super::context_panel::ContextPanel;
use super::diff_panel::DiffPanel;
use super::permission_card::PermissionCard;
use super::tasks_panel::TasksPanel;
use super::tool_detail::ToolDetail;
use super::workflow_panel::WorkflowPanel;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::{session_rules, t, tool_kinds, workflow};
use leptos::prelude::*;

/// Which panel a selected tool opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Workflow,
    Agent,
    Tool,
}

pub fn panel_for(name: &str) -> Panel {
    if name == workflow::TOOL {
        Panel::Workflow
    } else if tool_kinds::is_agent(name) {
        Panel::Agent
    } else {
        Panel::Tool
    }
}

/// `aside.pane.insp` when open: PermissionCard (first pending), TasksPanel, then WorkflowPanel | AgentPanel |
/// ToolDetail (selected) | DiffPanel | ContextPanel.
#[component]
pub fn Inspector(
    session: SessionHandle,
) -> impl IntoView {
    let ui = use_ui();
    // Keyed on the request id (`@key="p.RequestId"`).
    let pending_key = Memo::new(move |_| session.meta.with(|m| m.pending.first().map(|p| p.request_id.clone())));
    let has_tasks = Memo::new(move |_| session.meta.with(|m| !m.tasks.is_empty()));
    // `Session.Tool(Ui.SelectedToolId)`, keyed on the tool id.
    let selected = Memo::new(move |_| {
        let id = ui.selected_tool_id.get()?;
        session.with_items(|items| session_rules::tool(items, &id).map(|t| (t.id.clone(), panel_for(&t.name))))
    });
    let shown = move || ui.inspector_open.get() || pending_key.with(Option::is_some);

    let card = move || {
        pending_key.get()?;
        let p = session.meta.with_untracked(|m| m.pending.first().cloned())?;
        Some(view! { <PermissionCard session=session permission=p /> })
    };
    let body = move || {
        if pending_key.with(Option::is_some) {
            return None;
        }
        if let Some((id, panel)) = selected.get() {
            let opt = session.tool_by_id(id);
            let first = opt.get_untracked()?;
            // Keeps its last value if the row goes away before `selected` drops it.
            let tool: Signal<ToolItem> =
                Memo::new(move |prev: Option<&ToolItem>| opt.get().or_else(|| prev.cloned()).unwrap_or_else(|| first.clone()))
                    .into();
            return Some(match panel {
                Panel::Workflow => view! { <WorkflowPanel session=session tool=tool /> }.into_any(),
                Panel::Agent => view! { <AgentPanel session=session agent=tool /> }.into_any(),
                Panel::Tool => view! { <ToolDetail session=session tool=tool /> }.into_any(),
            });
        }
        if ui.diff_open.get() {
            return Some(view! { <DiffPanel session=session /> }.into_any());
        }
        // Without a process yet (a past session) the context is empty, but the memory files are still on disk.
        Some(view! { <ContextPanel session=session /> }.into_any())
    };

    view! {
        <Show when=shown>
            <aside class="pane insp" aria-label=t!("Inspector")>
                {card}
                {move || has_tasks.get().then(|| view! { <TasksPanel session=session /> })}
                {body}
            </aside>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels() {
        assert_eq!(panel_for("Workflow"), Panel::Workflow);
        assert_eq!(panel_for("Agent"), Panel::Agent);
        assert_eq!(panel_for("Bash"), Panel::Tool);
    }
}
