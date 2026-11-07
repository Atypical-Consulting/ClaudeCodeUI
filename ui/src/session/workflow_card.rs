//! Port of `Components/Session/WorkflowCard.razor` — spec 07 §13, 05 §4. Owner: ui-session-thread-composer.

use crate::ipc;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::workflow::{WfState, WorkflowRun, TOOL};
use ccui_shared::{fmt, t};
use leptos::prelude::*;
use std::time::Duration;

/// Ultracode Workflow run card (workflow_run polled 1 s while live via Ticker).
/// The run's phases with their state and done/total agents, as the CLI shows them. The header or a phase selects the
/// call (WorkflowPanel). Re-read when the call changes, and once a second while the run is live (the backend only
/// stats the files and reads what was appended).
#[component]
pub fn WorkflowCard(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)]
    tool: Signal<ToolItem>,
) -> impl IntoView {
    let ui = use_ui();
    let run = poll_run(session, tool);
    let sel = Memo::new(move |_| ui.selected_tool_id.with(|s| tool.with(|t| s.as_deref() == Some(t.id.as_str()))));
    let select = move |phase: Option<String>| {
        ui.selected_phase.set(phase);
        ui.selected_tool_id.set(Some(tool.with_untracked(|t| t.id.clone())));
        ui.inspector_open.set(true); // as ToolRow: a closed inspector would swallow the click
    };
    move || {
        run.get().map(|r| {
            let state = r.state;
            let (tool_state, head_text) = (tool.with(|t| t.state), head(&r));
            let lbl = run_label(&r, tool_state);
            let head_sel = sel.get() && ui.selected_phase.with(Option::is_none);
            let phases = r
                .phases
                .iter()
                .map(|p| {
                    let title = p.title.clone();
                    let on = sel.get() && ui.selected_phase.with(|s| s.as_deref() == Some(p.title.as_str()));
                    let pick = title.clone();
                    view! {
                        <button type="button" class=if on { "wfp sel" } else { "wfp" } aria-pressed=if on { "true" } else { "false" }
                            on:click=move |_| select(Some(pick.clone()))>
                            <span class=format!("dot {}", dot(p.state(), None)) aria-hidden="true"></span>
                            <span class="nm">{title}<span class="sr-only">{format!(" · {}", label(p.state(), None))}</span></span>
                            <span class="m">{format!("{}/{}", p.done, p.total)}</span>
                        </button>
                    }
                })
                .collect_view();
            let tool_now = tool.get();
            view! {
                <div class="wf wfr">
                    <button type="button" class=if head_sel { "wh sel" } else { "wh" } aria-pressed=if sel.get() { "true" } else { "false" }
                        on:click=move |_| select(None)>
                        <span class=format!("dot {}", dot(state, Some(&tool_now))) aria-hidden="true"></span>
                        <b>{format!("{} · {}", TOOL.to_lowercase(), r.name)}</b>
                        <span class="r">{head_text}
                            {(!matches!(state, WfState::Failed | WfState::Unknown))
                                .then(|| view! { <span class="sr-only">{format!(" · {lbl}")}</span> })}
                        </span>
                    </button>
                    {phases}
                </div>
            }
        })
    }
}

/// `WorkflowRuns.Of(tool)` through IPC: read on mount and whenever the call changes, then once a second while live.
/// Also used by WorkflowPanel's owner if it wants the same polling.
pub fn poll_run(session: SessionHandle, tool: Signal<ToolItem>) -> ReadSignal<Option<WorkflowRun>> {
    let (run, set_run) = signal(None::<WorkflowRun>);
    let busy = StoredValue::new(false);
    let fetch = move || {
        if busy.get_value() {
            return;
        }
        let tool_id = tool.with_untracked(|t| t.id.clone());
        if tool_id.is_empty() {
            return;
        }
        busy.set_value(true);
        let id = session.id();
        leptos::task::spawn_local(async move {
            match ipc::workflow_run(&id, &tool_id).await {
                Ok(r) => {
                    if run.try_with_untracked(|cur| *cur != r) == Some(true) {
                        let _ = set_run.try_set(r);
                    }
                }
                Err(e) => leptos::logging::error!("workflow_run: {e}"),
            }
            busy.try_set_value(false);
        });
    };
    // The call changed (state, progress, end): re-read.
    Effect::new(move |_| {
        tool.track();
        fetch();
    });
    let live = Memo::new(move |_| run.with(|r| r.as_ref().is_some_and(|r| r.live)));
    let timer: StoredValue<Option<IntervalHandle>> = StoredValue::new(None);
    let stop = move || {
        if let Some(h) = timer.try_update_value(Option::take).flatten() {
            h.clear();
        }
    };
    Effect::new(move |_| {
        let on = live.get();
        stop();
        if on {
            if let Ok(h) = set_interval_with_handle(fetch, Duration::from_secs(1)) {
                timer.set_value(Some(h));
            }
        }
    });
    on_cleanup(stop);
    run
}

/// `Head(r)`: `Wf.AgentsDone(done, agents)` · elapsed, then ` · Label` when Failed / Unknown.
pub fn head(r: &WorkflowRun) -> String {
    let mut s = format!("{} · {}", t!("Wf.AgentsDone", r.done(), r.agents.len()), fmt::dur(r.elapsed_ms));
    if matches!(r.state, WfState::Failed | WfState::Unknown) {
        s.push_str(" · ");
        s.push_str(&label(r.state, r.raw_status.as_deref()));
    }
    s
}

/// `Label(run, tool)`: the call waiting → Tool.Waiting, denied → Tool.Denied, else the run's label.
pub fn run_label(r: &WorkflowRun, tool: ToolState) -> String {
    match tool {
        ToolState::Waiting => t!("Tool.Waiting"),
        ToolState::Denied => t!("Tool.Denied"),
        _ => label(r.state, r.raw_status.as_deref()),
    }
}

/// `WorkflowCard.Dot(s, t)`: Done "done", Running "run", Failed "err", Unknown "exited", else (not started)
/// "wait" when the call waits for a permission, "idle" otherwise. Also used by WorkflowPanel.
pub fn dot(state: WfState, tool: Option<&ToolItem>) -> &'static str {
    match state {
        WfState::Done => "done",
        WfState::Running => "run",
        WfState::Failed => "err",
        WfState::Unknown => "exited",
        WfState::Waiting => {
            if tool.is_some_and(|t| t.state == ToolState::Waiting) {
                "wait"
            } else {
                "idle"
            }
        }
    }
}

/// `WorkflowCard.Label(s, raw)`: raw ?? Tool.Done | Status.Running | Tool.Error | Wf.State.Unknown | Wf.State.Waiting.
/// Also used by WorkflowPanel.
pub fn label(state: WfState, raw: Option<&str>) -> String {
    if let Some(r) = raw {
        return r.to_string();
    }
    t!(match state {
        WfState::Done => "Tool.Done",
        WfState::Running => "Status.Running",
        WfState::Failed => "Tool.Error",
        WfState::Unknown => "Wf.State.Unknown",
        WfState::Waiting => "Wf.State.Waiting",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_and_labels() {
        assert_eq!(dot(WfState::Unknown, None), "exited");
        assert_eq!(dot(WfState::Waiting, None), "idle");
        assert_eq!(label(WfState::Failed, Some("cancelled")), "cancelled");
        assert_eq!(label(WfState::Done, None), t!("Tool.Done"));
    }
}
