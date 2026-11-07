//! Port of `Components/Session/TasksPanel.razor` — spec 07 §25. Owner: ui-session-inspector.
//! Background shell and Monitor tasks (task_type local_bash): command, status, elapsed and the end of their output,
//! read with get_task_output every 2 s while one runs (once more after it ends), and Stop (stop_task). Other
//! background tasks (an MCP call, an MCP or WebSocket monitor, a teammate) show their type and Stop only:
//! get_task_output reads shells and Monitors alone. Running tasks first, then the last few that ended, which "Clear"
//! drops.

use crate::common::icon::Icon;
use crate::common::ticker::Ticker;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::model::*;
use ccui_shared::{fmt, i18n, json, session_rules, t};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Running tasks first, then the last 5 that ended.
pub fn shown(tasks: &[BgTask]) -> Vec<&BgTask> {
    let ended: Vec<&BgTask> = tasks.iter().filter(|t| !t.running()).collect();
    tasks.iter().filter(|t| t.running()).chain(ended[ended.len().saturating_sub(5)..].iter().copied()).collect()
}

/// The ended task with the latest ended_at (`MaxBy`: the first one wins a tie).
pub fn last_ended(tasks: &[BgTask]) -> Option<&BgTask> {
    tasks.iter().filter(|t| !t.running()).fold(None, |best: Option<&BgTask>, t| match best {
        Some(b) if b.ended_at >= t.ended_at => Some(b),
        _ => Some(t),
    })
}

/// A stop the user asked for is not a failure: neutral dashed dot, red only for "failed" and unknown statuses.
pub fn dot(t: &BgTask) -> &'static str {
    if t.running() {
        return "run";
    }
    match t.status.as_str() {
        "completed" => "idle",
        "killed" | "stopped" => "exited",
        _ => "err",
    }
}

/// Key of the status word.
pub fn label_key(status: &str) -> &'static str {
    match status {
        "running" | "pending" => "Bg.Running",
        "paused" => "Bg.Paused",
        "completed" => "Bg.Completed",
        "failed" => "Bg.Failed",
        "killed" | "stopped" => "Bg.Stopped",
        _ => "Bg.Ended", // a status this build does not know: it is not Running either
    }
}

/// What one `session_task_output` answer does: (new output to store, mark the task final).
pub fn poll_step(out: &TaskOutput, running: bool) -> (Option<&str>, bool) {
    match out {
        TaskOutput::Text(o) => (Some(o), !running),
        // Gone with its output file: nothing left to read.
        TaskOutput::RequestFailed(_) => (None, !running),
        // The CLI is busy: next tick. No process: nothing asked.
        TaskOutput::Timeout | TaskOutput::NoProcess => (None, false),
    }
}

/// Background tasks of this process: output tail (session_task_output), stop, clear ended.
#[component]
pub fn TasksPanel(
    session: SessionHandle,
) -> impl IntoView {
    let output = RwSignal::new(HashMap::<String, String>::new());
    let stopping = RwSignal::new(HashSet::<String>::new());
    let error = RwSignal::new(None::<String>);
    // Ended tasks whose last output was read (or is gone).
    let fin: Rc<RefCell<HashSet<String>>> = Rc::default();
    let polling = Rc::new(Cell::new(false));
    let alive = Arc::new(AtomicBool::new(true));

    let poll = {
        let alive = alive.clone();
        move || {
            if polling.get() || !alive.load(Ordering::Relaxed) || !session.meta.with_untracked(|m| m.has_process) {
                return;
            }
            polling.set(true);
            let todo: Vec<(String, bool)> = session.meta.with_untracked(|m| {
                m.tasks
                    .iter()
                    .filter(|t| t.kind.is_none() && (t.running() || !fin.borrow().contains(&t.id)))
                    .map(|t| (t.id.clone(), t.running()))
                    .collect()
            });
            let (id, fin, polling, alive) = (session.id(), fin.clone(), polling.clone(), alive.clone());
            spawn_local(async move {
                for (task, running) in todo {
                    let Ok(out) = ipc::session_task_output(&id, &task).await else { continue };
                    if !alive.load(Ordering::Relaxed) {
                        break;
                    }
                    let (text, final_) = poll_step(&out, running);
                    if let Some(o) = text {
                        if output.with_untracked(|m| m.get(&task).map(String::as_str) != Some(o)) {
                            output.update(|m| {
                                m.insert(task.clone(), o.to_string());
                            });
                        }
                    }
                    if final_ {
                        fin.borrow_mut().insert(task);
                    }
                }
                polling.set(false);
            });
        }
    };
    poll();
    let timer = set_interval_with_handle(poll, Duration::from_millis(2000)).ok();
    on_cleanup(move || {
        alive.store(false, Ordering::Relaxed);
        if let Some(h) = timer {
            h.clear();
        }
    });

    // The new status arrives as task_updated ("killed"); the answer only says the request was taken. Any error (the
    // process exiting under the click) is shown.
    let stop = move |task: String| {
        if stopping.with_untracked(|s| s.contains(&task)) {
            return;
        }
        stopping.update(|s| {
            s.insert(task.clone());
        });
        error.set(None);
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_stop_task(&id, &task).await {
                error.set(Some(t!("Bg.StopFailed", e)));
            }
            stopping.update(|s| {
                s.remove(&task);
            });
        });
    };
    let clear = move |_| {
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_clear_ended_tasks(&id).await {
                leptos::logging::error!("session_clear_ended_tasks: {e}");
            }
        });
    };

    let ids = Memo::new(move |_| session.meta.with(|m| shown(&m.tasks).into_iter().map(|t| t.id.clone()).collect::<Vec<_>>()));
    let counts = Memo::new(move |_| {
        session.meta.with(|m| {
            let s = shown(&m.tasks);
            let running = s.iter().filter(|t| t.running()).count();
            (running, s.len() > running)
        })
    });
    let announce = move || {
        session.meta.with(|m| last_ended(&m.tasks).map(|t| format!("{} · {}", t.description, t!(label_key(&t.status)))))
    };

    let row = move |task_id: String| {
        let task = Memo::new({
            let task_id = task_id.clone();
            move |prev: Option<&Option<BgTask>>| {
                session
                    .meta
                    .with(|m| m.tasks.iter().find(|t| t.id == task_id).cloned())
                    .or_else(|| prev.cloned().flatten())
            }
        });
        let tool = Memo::new(move |_| {
            let tu = task.with(|t| t.as_ref().map(|t| t.tool_use_id.clone()))?;
            session.with_items(|items| {
                session_rules::tool(items, &tu).map(|x| (x.name.clone(), json::str(&x.input, "command").map(str::to_string)))
            })
        });
        let running = Memo::new(move |_| task.with(|t| t.as_ref().is_some_and(BgTask::running)));
        let has_process = Memo::new(move |_| session.meta.with(|m| m.has_process));
        move || {
            let t = task.get()?;
            let nm = match (&t.kind, tool.with(|x| x.as_ref().map(|x| x.0.clone()))) {
                (Some(k), _) => format!("{} · {}", i18n::name("Bg.Type.", Some(k)), t.description),
                (None, Some(name)) => format!("{name} · {}", t.description),
                (None, None) => t.description.clone(),
            };
            let cmd = tool.with(|x| x.as_ref().and_then(|x| x.1.clone()));
            let id = t.id.clone();
            let out = t.kind.is_none().then(|| {
                let id = id.clone();
                let body = move || {
                    output.with(|m| match m.get(&id) {
                        Some(o) if !o.is_empty() => BgTask::tail(o, 12),
                        _ => t!("Bg.NoOutput"),
                    })
                };
                view! { <pre class="cmdlog out" role="region" tabindex="0" aria-label=t!("Bg.Output", t.description)>{body}</pre> }
            });
            let elapsed = {
                let t = t.clone();
                move || {
                    let end = t.ended_at.unwrap_or_else(chrono::Utc::now);
                    format!("{} · {}", t!(label_key(&t.status)), fmt::dur((end - t.started_at).num_milliseconds()))
                }
            };
            let stop_btn = {
                let (id, desc) = (id.clone(), t.description.clone());
                move || {
                    (running.get() && has_process.get()).then(|| {
                        let (id, id2) = (id.clone(), id.clone());
                        view! {
                            <div><button class="btn" disabled=move || stopping.with(|s| s.contains(&id2)) aria-label=t!("Bg.StopLabel", desc)
                                on:click=move |_| stop(id.clone())><Icon name="stop" sm=true />{t!("Bg.Stop")}</button></div>
                        }
                    })
                }
            };
            Some(view! {
                <details class="bg-row" prop:open=move || running.get()>
                    <summary>
                        <span class=format!("dot {}", dot(&t)) aria-hidden="true"></span>
                        <span class="nm">{nm}</span>
                        <span class="m"><Ticker active=Signal::derive(move || running.get())>{elapsed.clone()}</Ticker></span>
                    </summary>
                    {cmd.map(|c| view! { <pre class="cmdlog" aria-label=t!("Bg.Command")><span class="c">"$ "</span>{c}</pre> })}
                    {out}
                    {stop_btn}
                </details>
            })
        }
    };

    view! {
        <section class="section bg-tasks" aria-labelledby="bg-tasks-h">
            <div class="hd">
                <h2 id="bg-tasks-h" class="cap">{move || format!("{} · {}", t!("Bg.Tasks"), t!("Bg.RunningN", counts.get().0))}</h2>
                {move || counts.get().1.then(|| view! { <button class="btn ghost" on:click=clear>{t!("Bg.Clear")}</button> })}
            </div>
            // Announces a task that stops, completes or fails; the rows themselves only change visually.
            <span class="sr-only" role="status">{announce}</span>
            <For each=move || ids.get() key=|id| id.clone() children=row />
            {move || error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> })}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: &str, ended: Option<i64>) -> BgTask {
        BgTask {
            id: id.into(),
            tool_use_id: "t".into(),
            description: "d".into(),
            started_at: Time::default(),
            kind: None,
            status: status.into(),
            ended_at: ended.map(|s| Time::default() + chrono::Duration::seconds(s)),
        }
    }

    #[test]
    fn order_and_labels() {
        let mut ts = vec![task("a", "completed", Some(1)), task("r", "running", None)];
        for i in 0..6 {
            ts.push(task(&format!("e{i}"), "killed", Some(10 + i)));
        }
        let ids: Vec<&str> = shown(&ts).iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["r", "e1", "e2", "e3", "e4", "e5"]);
        assert_eq!(last_ended(&ts).map(|t| t.id.as_str()), Some("e5"));
        assert_eq!(dot(&ts[1]), "run");
        assert_eq!(dot(&ts[0]), "idle");
        assert_eq!(dot(&ts[2]), "exited");
        assert_eq!(dot(&task("x", "failed", Some(1))), "err");
        assert_eq!(label_key("pending"), "Bg.Running");
        assert_eq!(label_key("weird"), "Bg.Ended");
    }

    #[test]
    fn task_output_answers() {
        assert_eq!(poll_step(&TaskOutput::Text("x".into()), true), (Some("x"), false));
        assert_eq!(poll_step(&TaskOutput::Text("x".into()), false), (Some("x"), true));
        assert_eq!(poll_step(&TaskOutput::RequestFailed("gone".into()), false), (None, true));
        assert_eq!(poll_step(&TaskOutput::RequestFailed("gone".into()), true), (None, false));
        assert_eq!(poll_step(&TaskOutput::Timeout, false), (None, false));
        assert_eq!(poll_step(&TaskOutput::NoProcess, false), (None, false));
    }
}
