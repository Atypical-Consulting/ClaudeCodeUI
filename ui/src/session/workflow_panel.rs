//! Port of `Components/Session/WorkflowPanel.razor` — spec 07 §29, 05 §4. Owner: ui-session-inspector.
//! Selected Workflow call: the run, its phases, the selected phase's agents, and the selected agent's last text,
//! result, task list and tool calls, each opening onto its input and output (agent-<id>.jsonl through
//! workflow_agent_log, new lines only; released when the agent is no longer shown). Refreshed once a second while the
//! run is live.

use super::agent_panel::SubTool;
use super::tool_detail::pretty;
use super::workflow_card;
use crate::common::icon::Icon;
use crate::common::markdown::Markdown;
use crate::ipc;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::api::WorkflowAgentLog;
use ccui_shared::model::*;
use ccui_shared::todo_list::{self, TodoStatus};
use ccui_shared::workflow::{self, WfAgent, WfState, WorkflowRun};
use ccui_shared::{fmt, t};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The latest ones: a long agent runs hundreds of calls.
const MAX_TOOLS: usize = 40;

/// `Key(a)`: agent_id ?? "{phase}/{label}".
pub fn agent_key(a: &WfAgent) -> String {
    a.agent_id.clone().unwrap_or_else(|| format!("{}/{}", a.phase, a.label))
}

/// The phase shown: the selected one, else the first running, else the last that has agents, else the first.
pub fn pick_phase(run: &WorkflowRun, selected: Option<&str>) -> Option<usize> {
    let ps = &run.phases;
    selected
        .and_then(|s| ps.iter().position(|p| p.title == s))
        .or_else(|| ps.iter().position(|p| p.state() == WfState::Running))
        .or_else(|| ps.iter().rposition(|p| p.total > 0))
        .or(if ps.is_empty() { None } else { Some(0) })
}

fn state_label(a: &WfAgent) -> String {
    if a.state == WfState::Waiting {
        t!("Wf.Queued")
    } else {
        workflow_card::label(a.state, a.raw_state.as_deref())
    }
}

fn meta(a: &WfAgent) -> String {
    let mut parts = Vec::new();
    if a.tokens > 0 {
        parts.push(format!("{}{}", if a.approx { "≈" } else { "" }, t!("Workflow.Tokens", fmt::tokens(a.tokens))));
    }
    if let Some(d) = a.duration_ms {
        parts.push(fmt::dur(d));
    }
    if !matches!(a.state, WfState::Done | WfState::Running) {
        parts.push(state_label(a)); // the dot says done or running
    }
    parts.join(" · ")
}

/// A Workflow run: phases, agents, selected phase/agent log (workflow_agent_log), result.
#[component]
pub fn WorkflowPanel(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)] tool: Signal<ToolItem>,
) -> impl IntoView {
    let ui = use_ui();
    let tool_id = StoredValue::new(tool.with_untracked(|t| t.id.clone()));
    let run = RwSignal::new(None::<WorkflowRun>);
    let agent_sel = RwSignal::new(None::<String>);
    let log = RwSignal::new(WorkflowAgentLog::default());
    // The agent whose log the backend keeps for us.
    let held = StoredValue::new(None::<String>);
    let busy = Arc::new(AtomicBool::new(false));
    let cwd = Signal::derive(move || session.meta.with(|m| m.cwd.clone()));

    let release = move |agent: String| {
        let (id, tid) = (session.id(), tool_id.get_value());
        spawn_local(async move {
            let _ = ipc::workflow_agent_log_release(&id, &tid, &agent).await;
        });
    };

    // Run, then the selected agent's new log lines.
    let refresh = {
        let busy = busy.clone();
        move || {
            if busy.swap(true, Ordering::Relaxed) {
                return;
            }
            let (id, tid) = (session.id(), tool_id.get_value());
            let busy = busy.clone();
            spawn_local(async move {
                if let Ok(r) = ipc::workflow_run(&id, &tid).await {
                    run.set(r);
                }
                let want = held.try_get_value().flatten();
                if let Some(agent) = want {
                    if let Ok(l) = ipc::workflow_agent_log(&id, &tid, &agent).await {
                        // Still the agent shown (a click while the read ran asks again on the next tick).
                        if held.try_get_value().flatten().as_deref() == Some(agent.as_str()) {
                            log.set(l);
                        }
                    }
                }
                busy.store(false, Ordering::Relaxed);
            });
        }
    };
    let refresh = StoredValue::new(refresh);
    let refresh_now = move || refresh.with_value(|f| f());

    // The tool moved (progress, state): read again.
    Effect::new(move |_| {
        tool.track();
        refresh_now();
    });
    let live = Memo::new(move |_| run.with(|r| r.as_ref().is_some_and(|r| r.live)));
    let timer = StoredValue::new(None::<IntervalHandle>);
    Effect::new(move |_| {
        if let Some(h) = timer.try_update_value(Option::take).flatten() {
            h.clear();
        }
        if live.get() {
            if let Ok(h) = set_interval_with_handle(refresh_now, Duration::from_secs(1)) {
                timer.set_value(Some(h));
            }
        }
    });

    let phase_idx = Memo::new(move |_| run.with(|r| r.as_ref().and_then(|r| pick_phase(r, ui.selected_phase.get().as_deref()))));
    let phase_title = Memo::new(move |_| run.with(|r| Some(r.as_ref()?.phases.get(phase_idx.get()?)?.title.clone())));
    let agents = Memo::new(move |_| {
        let Some(title) = phase_title.get() else { return Vec::new() };
        run.with(|r| r.as_ref().map(|r| r.agents.iter().filter(|a| a.phase == title).cloned().collect()).unwrap_or_default())
    });
    let agent = Memo::new(move |_| {
        let k = agent_sel.get()?;
        agents.with(|a| a.iter().find(|a| agent_key(a) == k).cloned())
    });
    // Hold the log of the agent shown; release the one no longer shown (and on unmount).
    Effect::new(move |_| {
        let want = agent.with(|a| a.as_ref().and_then(|a| a.agent_id.clone()));
        let had = held.get_value();
        if had == want {
            return;
        }
        if let Some(h) = had {
            release(h);
        }
        held.set_value(want.clone());
        log.set(WorkflowAgentLog::default());
        if want.is_some() {
            refresh_now();
        }
    });
    on_cleanup(move || {
        if let Some(h) = timer.try_update_value(Option::take).flatten() {
            h.clear();
        }
        if let Some(a) = held.try_get_value().flatten() {
            release(a);
        }
    });

    let head = move || {
        run.with(|r| {
            let (name, label) = r.as_ref().map_or((String::new(), String::new()), |r| {
                (r.name.clone(), workflow_card::label(r.state, r.raw_status.as_deref()))
            });
            view! { <h2 class="ttl">{format!("{} · {name}", workflow::TOOL.to_lowercase())}</h2><span class="sub">{label}</span> }
        })
    };
    let summary = move || {
        run.get().map(|r| {
            let desc = r.description.clone().filter(|d| !d.is_empty()).map(|d| view! { <p>{d}</p> });
            let model = r.model.clone().map(|m| view! { <span>{t!("Kv.Model")}</span><span>{m}</span> });
            view! {
                {desc}
                <div class="kv">
                    <span>{t!("Kv.Status")}</span><span>{workflow_card::label(r.state, r.raw_status.as_deref())}</span>
                    <span>{t!("Kv.Agents")}</span><span>{format!("{}/{}", r.done(), r.agents.len())}</span>
                    <span>{t!("Kv.Elapsed")}</span><span>{fmt::dur(r.elapsed_ms)}</span>
                    <span>{t!("Kv.Tokens")}</span><span>{if r.tokens > 0 { fmt::tokens(r.tokens) } else { "—".into() }}</span>
                    <span>{t!("Kv.Tools")}</span><span>{r.tool_calls}</span>
                    {model}
                </div>
            }
        })
    };

    // Rows keyed on what they show: an unchanged row keeps its element (and the focus).
    let phases = Memo::new(move |_| {
        let sel = phase_idx.get();
        run.with(|r| {
            r.as_ref()
                .map(|r| {
                    r.phases
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (p.title.clone(), p.detail.clone(), p.state(), p.done, p.total, sel == Some(i)))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    });
    let phase_list = move || {
        if phases.with(Vec::is_empty) {
            return view! { <p>{t!("Wf.NoPhases")}</p> }.into_any();
        }
        view! {
            <div class="wf">
                <For each=move || phases.get() key=|p| format!("{p:?}") let:p>
                    {
                        let (title, detail, state, done, total, sel) = p;
                        let pick = title.clone();
                        view! {
                            <button type="button" class=if sel { "wfp sel" } else { "wfp" } aria-pressed=if sel { "true" } else { "false" }
                                on:click=move |_| ui.selected_phase.set(Some(pick.clone()))>
                                <span class=format!("dot {}", workflow_card::dot(state, None)) aria-hidden="true"></span>
                                <span class="nm">{title}{detail.filter(|d| !d.is_empty()).map(|d| view! { <small>{format!(" {d}")}</small> })}
                                    <span class="sr-only">{format!(" · {}", workflow_card::label(state, None))}</span></span>
                                <span class="m">{format!("{done}/{total}")}</span>
                            </button>
                        }
                    }
                </For>
            </div>
        }
        .into_any()
    };

    let agent_rows = Memo::new(move |_| {
        let open = agent_sel.get();
        agents.with(|a| {
            a.iter()
                .map(|a| {
                    let k = agent_key(a);
                    let is_open = open.as_deref() == Some(k.as_str());
                    (k, is_open, a.state, a.label.clone(), a.model.clone(), state_label(a), a.approx, meta(a))
                })
                .collect::<Vec<_>>()
        })
    });
    let agents_section = move || {
        let title = phase_title.get()?;
        let last = agent.with(Option::is_none);
        let list = if agent_rows.with(Vec::is_empty) {
            view! { <p>{t!("Wf.NoAgents")}</p> }.into_any()
        } else {
            view! {
                <div class="wf">
                    <For each=move || agent_rows.get() key=|r| format!("{r:?}") let:r>
                        {
                            let (k, is_open, state, label, model, st, approx, m) = r;
                            view! {
                                <button type="button" class=if is_open { "agent sel" } else { "agent" } aria-expanded=if is_open { "true" } else { "false" }
                                    aria-controls=is_open.then_some("wf-agent")
                                    on:click=move |_| agent_sel.update(|s| *s = if s.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) })>
                                    <Icon name=if is_open { "down" } else { "right" } sm=true class="chev" />
                                    <span class=format!("dot {}", workflow_card::dot(state, None)) aria-hidden="true"></span>
                                    <span class="nm">{label}" "<small>{model}</small><span class="sr-only">{format!(" · {st}")}</span></span>
                                    <span class="m" title=approx.then(|| t!("Wf.Approx"))>{m}</span>
                                </button>
                            }
                        }
                    </For>
                </div>
            }
            .into_any()
        };
        Some(view! {
            <section class=if last { "section last" } else { "section" } aria-labelledby="wf-agents">
                <h3 id="wf-agents" class="cap">{t!("Wf.PhaseAgents", title)}</h3>
                {list}
            </section>
        })
    };

    let tools_shown = Memo::new(move |_| log.with(|l| l.tools.iter().skip(l.tools.len().saturating_sub(MAX_TOOLS)).map(|x| x.id.clone()).collect::<Vec<_>>()));
    let agent_section = move || {
        let a = agent.get()?;
        let text = move || match log.with(|l| l.text.clone()).filter(|t| !t.is_empty()) {
            Some(t) => view! { <Markdown text=t /> }.into_any(),
            None => {
                let running = agent.with(|a| a.as_ref().is_some_and(|a| a.state == WfState::Running));
                view! { <p>{t!(if running { "Agent.NothingYet" } else { "Agent.NoText" })}</p> }.into_any()
            }
        };
        let result = a.result.clone().filter(|r| !r.is_empty()).map(|res| {
            let shown = workflow::json_result(&res).map(|j| pretty(&j)).unwrap_or(res);
            view! {
                <span class="cap">{t!("Wf.Result")}</span>
                <pre class="cmdlog out wfres" role="region" tabindex="0" aria-label=t!("Wf.Result")>{shown}</pre>
            }
        });
        let todos = move || {
            let items: Vec<Item> = log.with(|l| l.tools.iter().cloned().map(Item::Tool).collect());
            let todos = todo_list::from(&items);
            (!todos.is_empty()).then(|| {
                let progress = todo_list::progress(&todos);
                let rows = todos
                    .into_iter()
                    .map(|td| {
                        let class = match td.status {
                            TodoStatus::InProgress => Some("now"),
                            TodoStatus::Completed => Some("done"),
                            TodoStatus::Pending => None,
                        };
                        view! { <li class=class><span class="sr-only">{format!("{} · ", todo_list::status_label(td.status))}</span>{td.label().to_string()}</li> }
                    })
                    .collect_view();
                view! { <span class="cap">{format!("{} · {progress}", t!("Todo.Title"))}</span><ul class="wf-todos">{rows}</ul> }
            })
        };
        let tools = move || {
            let n = log.with(|l| l.tools.len());
            (n > 0).then(|| {
                let tool_of = move |id: String| -> Signal<ToolItem> {
                    let first = log.with_untracked(|l| l.tools.iter().find(|x| x.id == id).cloned());
                    Memo::new(move |prev: Option<&ToolItem>| {
                        log.with(|l| l.tools.iter().find(|x| x.id == id).cloned())
                            .or_else(|| prev.cloned())
                            .or_else(|| first.clone())
                            .expect("a listed tool")
                    })
                    .into()
                };
                view! {
                    <span class="cap">{format!("{} · {n}", t!("Wf.AgentTools"))}</span>
                    <div class="subrun wfa">
                        {(n > MAX_TOOLS).then(|| view! { <div class="sl more">{t!("Wf.EarlierTools", n - MAX_TOOLS)}</div> })}
                        <For each=move || tools_shown.get() key=|id| id.clone() let:id>
                            <SubTool tool=tool_of(id) cwd=cwd />
                        </For>
                    </div>
                }
            })
        };
        Some(view! {
            <section id="wf-agent" class="section last" aria-labelledby="wf-agent-h">
                <h3 id="wf-agent-h" class="cap">{t!("Agent.Writes")}</h3>
                {text}
                {result}
                {todos}
                {tools}
            </section>
        })
    };

    view! {
        <div class="ph">{head}
            <span class="end"><button class="btn ghost" title=t!("Action.Close") aria-label=t!("Action.Close") on:click=move |_| ui.selected_tool_id.set(None)><Icon name="x" sm=true /></button></span>
        </div>
        <div class="scroll">
            <section class="section">{summary}</section>
            <section class="section" aria-labelledby="wf-phases">
                <h3 id="wf-phases" class="cap">{t!("Wf.Phases")}</h3>
                {phase_list}
            </section>
            {agents_section}
            {agent_section}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccui_shared::workflow::WfPhase;

    fn phase(title: &str, total: i32, done: i32, running: i32) -> WfPhase {
        WfPhase { title: title.into(), detail: None, total, done, running, failed: 0 }
    }

    fn run(phases: Vec<WfPhase>) -> WorkflowRun {
        WorkflowRun {
            name: "w".into(),
            description: None,
            run_dir: None,
            phases,
            agents: vec![],
            tokens: 0,
            tool_calls: 0,
            elapsed_ms: 0,
            state: WfState::Running,
            raw_status: None,
            model: None,
            live: true,
        }
    }

    #[test]
    fn phase_choice() {
        let r = run(vec![phase("a", 2, 2, 0), phase("b", 2, 1, 1), phase("c", 0, 0, 0)]);
        assert_eq!(pick_phase(&r, None), Some(1));
        assert_eq!(pick_phase(&r, Some("c")), Some(2));
        assert_eq!(pick_phase(&r, Some("zz")), Some(1));
        let done = run(vec![phase("a", 2, 2, 0), phase("b", 1, 1, 0), phase("c", 0, 0, 0)]);
        assert_eq!(pick_phase(&done, None), Some(1));
        let empty = run(vec![phase("a", 0, 0, 0)]);
        assert_eq!(pick_phase(&empty, None), Some(0));
        assert_eq!(pick_phase(&run(vec![]), None), None);
    }

    #[test]
    fn keys() {
        let mut a = WfAgent {
            label: "lint".into(),
            phase: "check".into(),
            state: WfState::Done,
            model: None,
            tokens: 0,
            approx: false,
            tool_calls: 0,
            duration_ms: None,
            agent_id: None,
            result: None,
            raw_state: None,
        };
        assert_eq!(agent_key(&a), "check/lint");
        a.agent_id = Some("a1".into());
        assert_eq!(agent_key(&a), "a1");
    }
}
