//! Port of `Components/Session/TaskActions.razor` — spec 07 §5. Owner: ui-session-inspector.
//! A running Bash command or subagent: move it to the background (Ctrl B in the terminal), or stop a subagent.
//! A background shell is stopped from TasksPanel.

use crate::common::icon::Icon;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::model::*;
use ccui_shared::{t, tool_kinds};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::future::Future;

/// Background (Ctrl+B) / stop buttons for a running shell or agent tool.
#[component]
pub fn TaskActions(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)] tool: Signal<ToolItem>,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    fn run(busy: RwSignal<bool>, error: RwSignal<Option<String>>, f: impl Future<Output = ipc::IpcResult<()>> + 'static) {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            if let Err(e) = f.await {
                error.set(Some(t!("Composer.ActionFailed", e)));
            }
            busy.set(false);
        });
    }
    let background = move |_| {
        let (id, tid) = (session.id(), tool.with_untracked(|t| t.id.clone()));
        run(busy, error, async move { ipc::session_background(&id, &tid).await });
    };
    let stop = move |task: String| {
        let id = session.id();
        run(busy, error, async move { ipc::session_stop_task(&id, &task).await });
    };
    let state = Memo::new(move |_| {
        tool.with(|t| {
            let (shell, agent) = (tool_kinds::is_shell(&t.name), tool_kinds::is_agent(&t.name));
            (t.state == ToolState::Running && (shell || agent))
                .then(|| (!t.background, if agent { t.task_id.clone() } else { None }))
        })
    });
    move || {
        let (can_bg, task) = state.get()?;
        Some(view! {
            {can_bg.then(|| view! {
                <button type="button" class="btn ghost" disabled=busy title=t!("Task.BackgroundHint") on:click=background>
                    <Icon name="down" sm=true />{t!("Task.Background")}
                </button>
            })}
            {task.map(|id| view! {
                <button type="button" class="btn ghost" disabled=busy on:click=move |_| stop(id.clone())>
                    <Icon name="stop" sm=true />{t!("Bg.Stop")}
                </button>
            })}
            {move || error.get().map(|e| view! { <span class="c-err-fg" role="alert">{e}</span> })}
        })
    }
}
