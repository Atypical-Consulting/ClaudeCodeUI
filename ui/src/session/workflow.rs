//! Port of `Components/Session/Workflow.razor` — spec 07 §21. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::session::tool_row::{meta, ToolMetaView};
use crate::state::{use_ui, SessionHandle, UiState};
use ccui_shared::model::*;
use ccui_shared::{fmt, i18n, json, session_rules, t, tool_kinds};
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};

/// `session.Children`: parent_tool_use_id → item indexes, shared by every Workflow block of a Thread (one pass over
/// the items per patch, not one per block). Provided by the Thread; a Workflow without it builds its own.
#[derive(Clone, Copy)]
pub struct ChildrenMap(pub Memo<HashMap<String, Vec<usize>>>);

impl ChildrenMap {
    pub fn new(session: SessionHandle) -> ChildrenMap {
        ChildrenMap(Memo::new(move |_| session.with_items(|it| session_rules::children(it))))
    }
}

/// Consecutive Agent calls as a tree (MaxDepth 5) with dots and states; Ticker while any runs.
/// Their own tools carry parent_tool_use_id = the Agent call id, and so do the agents they spawn, whose rows open the
/// same way. No phases, workflow name or per-agent cost: the CLI does not provide them. The clock moves only the
/// printed durations (rows keep their focus while it ticks).
#[component]
pub fn Workflow(
    session: SessionHandle,
    /// Item indexes of the consecutive Agent calls; rows read `session.tool_at(i)` (the `open` set survives).
    #[prop(into)]
    agents: Signal<Vec<usize>>,
) -> impl IntoView {
    let ui = use_ui();
    let children = use_context::<ChildrenMap>().unwrap_or_else(|| ChildrenMap::new(session)).0;
    let open = RwSignal::new(HashSet::<String>::new());
    let list = Memo::new(move |_| {
        agents.with(|ix| {
            ix.iter().filter_map(|&i| session.row(i)).filter_map(|r| r.with(|it| it.as_tool().cloned())).collect::<Vec<_>>()
        })
    });
    let now = super::clock(Signal::derive(move || list.with(|a| a.iter().any(|a| a.ended_at.is_none()))));
    let tree = Tree { session, ui, children, open, now, cwd: super::cwd(session) };
    view! {
        <div class="wf">
            <div class="wh"><Icon name="agents"/><b>{t!("Workflow.Subagents")}</b>
                <span class="tag">{t!("Workflow.Tag")}</span><span class="r">{move || list.with(|a| head(a, now.get()))}</span></div>
            <For each=move || agents.get() key=|i| *i let:i>
                {
                    let a = session.tool_at(i);
                    let id = Memo::new(move |_| a.with(|a| a.id.clone()));
                    let is_open = Memo::new(move |_| id.with(|id| open.with(|o| o.contains(id))));
                    let sel = Memo::new(move |_| id.with(|id| ui.selected_tool_id.with(|s| s.as_deref() == Some(id.as_str()))));
                    let kids = move || {
                        let id = id.get();
                        (is_open.get() && !tree.kids(&id).is_empty()).then(|| tree.render(id, 1))
                    };
                    view! {
                        <button type="button" class=move || if sel.get() { "agent sel" } else { "agent" }
                            aria-expanded=move || if is_open.get() { "true" } else { "false" }
                            on:click=move |_| tree.click(&id.get_untracked())>
                            {move || view! { <Icon name=if is_open.get() { "down" } else { "right" } sm=true class="chev"/> }}
                            <span class=move || format!("dot {}", a.with(dot))></span>
                            <span class="nm">
                                {move || a.with(|a| json::str(&a.input, "description").map_or_else(|| t!("Agent.Title"), str::to_string))}" "
                                <small>{move || a.with(|a| json::str(&a.input, "subagent_type").map(str::to_string))}</small>
                            </span>
                            <span class="m">{move || a.with(|a| state(a, now.get()))}</span>
                        </button>
                        {kids}
                    }
                }
            </For>
        </div>
    }
}

#[derive(Clone, Copy)]
struct Tree {
    session: SessionHandle,
    ui: UiState,
    children: Memo<HashMap<String, Vec<usize>>>,
    open: RwSignal<HashSet<String>>,
    now: Signal<Time>,
    cwd: Memo<String>,
}

impl Tree {
    fn click(&self, id: &str) {
        self.open.update(|o| {
            if !o.remove(id) {
                o.insert(id.to_string());
            }
        });
        self.ui.selected_tool_id.set(Some(id.to_string())); // the inspector is not forced open (unlike ToolRow)
    }

    /// An agent's tool calls, SubagentHandback left out.
    fn kids(&self, id: &str) -> Vec<ToolItem> {
        let ix = self.children.with(|c| c.get(id).cloned().unwrap_or_default());
        ix.into_iter()
            .filter_map(|i| self.session.row(i))
            .filter_map(|r| r.with(|it| it.as_tool().filter(|t| t.name != "SubagentHandback").cloned()))
            .collect()
    }

    /// `Kids(a, depth)`: a child Agent is a row that opens onto its own calls, one level deeper.
    fn render(self, id: String, depth: usize) -> AnyView {
        let rows = move || {
            self.kids(&id)
                .into_iter()
                .map(|t| {
                    let color = format!("color:{}", tool_kinds::color(&t.name));
                    let label = tool_kinds::label(&t.name);
                    if tool_kinds::is_agent(&t.name) && depth < MAX_DEPTH {
                        let tid = StoredValue::new(t.id.clone());
                        let is_open = Memo::new(move |_| tid.with_value(|id| self.open.with(|o| o.contains(id))));
                        let sel = Memo::new(move |_| {
                            tid.with_value(|id| self.ui.selected_tool_id.with(|s| s.as_deref() == Some(id.as_str())))
                        });
                        let deeper = move || {
                            let id = tid.get_value();
                            (is_open.get() && !self.kids(&id).is_empty()).then(|| self.render(id, depth + 1))
                        };
                        let desc = json::string(&t.input, "description");
                        let kind = json::str(&t.input, "subagent_type").map(str::to_string);
                        let now = self.now;
                        view! {
                            <button type="button" class=move || if sel.get() { "sl sub sel" } else { "sl sub" }
                                aria-expanded=move || if is_open.get() { "true" } else { "false" }
                                on:click=move |_| self.click(&tid.get_value())>
                                <span class="k" style=color>{move || view! { <Icon name=if is_open.get() { "down" } else { "right" } sm=true/> }}{label}</span>
                                <span class="p">{desc}" "<small>{kind}</small></span>
                                <span class="m">{move || state(&t, now.get())}</span>
                            </button>
                            {deeper}
                        }
                        .into_any()
                    } else {
                        let m = meta(&t);
                        let target = self.cwd.with(|cwd| tool_kinds::target(&t.name, &t.input, cwd));
                        let name = t.name.clone();
                        view! {
                            <div class="sl"><span class="k" style=color title=name>{label}</span>
                                <span class="p">{target}</span><ToolMetaView meta=Signal::stored(m)/></div>
                        }
                        .into_any()
                    }
                })
                .collect_view()
        };
        view! { <div class="subrun">{rows}</div> }.into_any()
    }
}

pub const MAX_DEPTH: usize = 5;

fn elapsed_ms(a: &ToolItem, now: Time) -> i64 {
    (a.ended_at.unwrap_or(now) - a.started_at).num_milliseconds()
}

/// `Head`: `Agents` plural · Fmt.Dur(max(ended_at ?? now) − min(started_at)).
pub fn head(agents: &[ToolItem], now: Time) -> String {
    let n = i18n::plural(agents.len() as i64, "Agents");
    let end = agents.iter().map(|a| a.ended_at.unwrap_or(now)).max();
    let start = agents.iter().map(|a| a.started_at).min();
    let ms = match (end, start) {
        (Some(e), Some(s)) => (e - s).num_milliseconds(),
        _ => 0,
    };
    format!("{n} · {}", fmt::dur(ms))
}

/// `Workflow.Dot(a)`: Done → "done", Running → "run", Waiting → "wait", else "err" (also used by AgentPanel).
pub fn dot(agent: &ToolItem) -> &'static str {
    match agent.state {
        ToolState::Done => "done",
        ToolState::Running => "run",
        ToolState::Waiting => "wait",
        _ => "err",
    }
}

/// `Workflow.State(a)`: `Workflow.Tokens(Fmt.Tokens) · ` when tokens > 0, then the state word (+ ` · Fmt.Dur(elapsed)`
/// for Running/Done; elapsed = (ended_at ?? now) − started_at). Also used by AgentPanel.
pub fn state(agent: &ToolItem, now: Time) -> String {
    let tokens = if agent.tokens > 0 { format!("{} · ", t!("Workflow.Tokens", fmt::tokens(agent.tokens))) } else { String::new() };
    let word = match agent.state {
        ToolState::Running => format!("{} · {}", t!("Status.Running"), fmt::dur(elapsed_ms(agent, now))),
        ToolState::Done => format!("{} · {}", t!("Tool.Done"), fmt::dur(elapsed_ms(agent, now))),
        ToolState::Waiting => t!("Tool.Waiting"),
        ToolState::Denied => t!("Tool.Denied"),
        ToolState::NotRun => t!("Tool.NotRun"),
        ToolState::Error => t!("Tool.Error"),
    };
    format!("{tokens}{word}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agent(state: ToolState, tokens: i64, start: i64, end: Option<i64>) -> ToolItem {
        let at = |ms| Time::default() + chrono::Duration::milliseconds(ms);
        ToolItem {
            id: "a".into(),
            name: "Agent".into(),
            input: json!({}),
            parent_tool_use_id: None,
            state,
            result_text: None,
            structured: None,
            started_at: at(start),
            ended_at: end.map(at),
            task_id: None,
            task_status: None,
            progress: None,
            tokens,
            sub_tool_uses: 0,
            background: false,
            model: None,
        }
    }

    #[test]
    fn states_and_head() {
        let now = Time::default() + chrono::Duration::milliseconds(10_000);
        let done = agent(ToolState::Done, 1500, 0, Some(4500));
        assert_eq!(state(&done, now), format!("{} · {} · {}", t!("Workflow.Tokens", "2k"), t!("Tool.Done"), fmt::dur(4500)));
        assert_eq!(state(&agent(ToolState::Waiting, 0, 0, None), now), t!("Tool.Waiting"));
        assert_eq!(dot(&agent(ToolState::Denied, 0, 0, None)), "err");
        let running = agent(ToolState::Running, 0, 2000, None);
        assert_eq!(head(&[done, running], now), format!("{} · {}", i18n::plural(2, "Agents"), fmt::dur(10_000)));
    }
}
