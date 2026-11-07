//! Port of `Components/Session/AgentPanel.razor` — spec 07 §24. Owner: ui-session-inspector.
//! Selected subagent (Agent tool): its latest text, a .kv, its full transcript (read-only) and the map of every
//! subagent of the session. No per-agent cost from the CLI, so no cost row.

use super::task_actions::TaskActions;
use super::tool_detail::pretty;
use super::tool_row::{self, ToolMetaView};
use super::workflow as wf;
use crate::common::icon::Icon;
use crate::common::markdown::Markdown;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::{fmt, i18n, json, session_rules, t, tool_kinds};
use leptos::prelude::*;
use std::borrow::Borrow;

/// The agent's own messages (parent_tool_use_id), else its handback report once done, else its result.
pub fn last_text<I: Borrow<Item>>(children: &[I], agent: &ToolItem) -> Option<String> {
    children
        .iter()
        .rev()
        .find_map(|i| match i.borrow() {
            Item::Text(x) => Some(x.markdown.clone()),
            _ => None,
        })
        .or_else(|| {
            let h = json::prop(agent.structured.as_ref()?, "handbackReport")?;
            json::str(h, "text").map(str::to_string)
        })
        .or_else(|| agent.result_text.clone())
}

/// Depth-first from the top-level Agent calls; `workflow::MAX_DEPTH` bounds a malformed parent chain.
/// Returns `(item index, depth)`.
pub fn agent_map<I: Borrow<Item>>(items: &[I]) -> Vec<(usize, usize)> {
    let children = session_rules::children(items);
    let mut list = Vec::new();
    fn walk<I: Borrow<Item>>(
        items: &[I],
        idx: &mut dyn Iterator<Item = usize>,
        children: &std::collections::HashMap<String, Vec<usize>>,
        depth: usize,
        list: &mut Vec<(usize, usize)>,
    ) {
        if depth > wf::MAX_DEPTH {
            return;
        }
        for i in idx {
            let Some(a) = items[i].borrow().as_tool().filter(|t| tool_kinds::is_agent(&t.name)) else { continue };
            list.push((i, depth));
            if let Some(c) = children.get(&a.id) {
                walk(items, &mut c.iter().copied(), children, depth + 1, list);
            }
        }
    }
    let top = (0..items.len()).filter(|&i| items[i].borrow().as_tool().is_some_and(|t| t.parent_tool_use_id.is_none()));
    walk(items, &mut top.collect::<Vec<_>>().into_iter(), &children, 0, &mut list);
    list
}

/// A subagent's tool call, read-only: label, target and meta, opening onto its input and output (AgentPanel's
/// transcript, WorkflowPanel's agent tools). The `<details>` stays put while the call updates.
#[component]
pub fn SubTool(
    #[prop(into)] tool: Signal<ToolItem>,
    #[prop(into)] cwd: Signal<String>,
) -> impl IntoView {
    let head = Memo::new(move |_| tool.with(|x| (x.name.clone(), x.input.clone())));
    let meta = Memo::new(move |_| tool.with(tool_row::meta));
    let result = move || tool.with(|x| x.result_text.clone().filter(|r| !r.is_empty()));
    move || {
        let (name, input) = head.get();
        view! {
            <details class="sl-d">
                <summary class="sl">
                    <span class="k" style=format!("color:{}", tool_kinds::color(&name)) title=name.clone()>{tool_kinds::label(&name)}</span>
                    <span class="p">{move || tool_kinds::target(&name, &input, &cwd.get())}</span>
                    <ToolMetaView meta=meta />
                </summary>
                <pre class="cmdlog wfres" aria-label=t!("Tool.Input")>{pretty(&head.get_untracked().1)}</pre>
                {move || result().map(|r| view! { <pre class="cmdlog out wfres" aria-label=t!("Tool.Output")>{r}</pre> })}
            </details>
        }
    }
}

/// One Agent call: description, prompt, model, children items (`session_rules::children`), result.
#[component]
pub fn AgentPanel(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_by_id`): state, tokens and children update in place.
    #[prop(into)] agent: Signal<ToolItem>,
) -> impl IntoView {
    let ui = use_ui();
    let agent_id = agent.with_untracked(|a| a.id.clone());
    let cwd = Signal::derive(move || session.meta.with(|m| m.cwd.clone()));
    let children = Memo::new({
        let agent_id = agent_id.clone();
        move |_| session.with_items(|items| session_rules::children(items).remove(&agent_id).unwrap_or_default())
    });

    let writes = move || {
        let a = agent.get();
        let (text, child_tools) = session.with_items(|items| {
            let kids: Vec<&Item> = children.with(|c| c.iter().filter_map(|&i| items.get(i).copied()).collect());
            (last_text(&kids, &a), kids.iter().filter(|i| i.as_tool().is_some()).count())
        });
        let text = match text.filter(|t| !t.is_empty()) {
            Some(t) => view! { <Markdown text=t /> }.into_any(),
            None => {
                let k = if a.state == ToolState::Running { "Agent.NothingYet" } else { "Agent.NoText" };
                view! { <p>{t!(k)}</p> }.into_any()
            }
        };
        let s = |k: &str| json::str(&a.input, k).map(str::to_string);
        // The Agent call's own model / effort override, else the model its messages came from.
        let model = s("model").or_else(|| a.model.clone()).unwrap_or_else(|| "—".into());
        let effort = s("effort").map(|e| view! { <span>{t!("Kv.Effort")}</span><span>{i18n::name("Effort.", Some(&e))}</span> });
        let tokens = if a.tokens > 0 { fmt::tokens(a.tokens) } else { "—".into() };
        view! {
            {text}
            <div class="kv">
                <span>{t!("Kv.Type")}</span><span>{s("subagent_type").unwrap_or_else(|| "—".into())}</span>
                <span>{t!("Kv.Model")}</span><span>{model}</span>
                {effort}
                <span>{t!("Kv.Tools")}</span><span>{(a.sub_tool_uses.max(0) as usize).max(child_tools)}</span>
                <span>{t!("Kv.Tokens")}</span><span>{tokens}</span>
            </div>
        }
    };

    let child = move |i: usize| {
        let Some(row) = session.row(i) else { return ().into_any() };
        let kind = row.with_untracked(|it| match it {
            Item::Text(_) => 0,
            Item::Tool(x) if x.name == "SubagentHandback" => 1,
            Item::Tool(x) if tool_kinds::is_agent(&x.name) => 2,
            Item::Tool(_) => 3,
            _ => 1,
        });
        match kind {
            0 => {
                let row = row.clone();
                (move || row.with(|it| match it {
                    Item::Text(x) => view! { <Markdown text=x.markdown.clone() /> }.into_any(),
                    _ => ().into_any(),
                }))
                .into_any()
            }
            1 => ().into_any(),
            _ => {
                let tool: Signal<ToolItem> = Memo::new({
                    let row = row.clone();
                    move |prev: Option<&ToolItem>| row.with(|it| it.as_tool().cloned()).or_else(|| prev.cloned()).unwrap_or_else(|| empty_like())
                })
                .into();
                if kind == 3 {
                    return view! { <SubTool tool=tool cwd=cwd /> }.into_any();
                }
                let meta = Memo::new(move |_| tool.with(tool_row::meta));
                (move || {
                    let x = tool.get();
                    let id = x.id.clone();
                    view! {
                        <button type="button" class="sl" on:click=move |_| ui.selected_tool_id.set(Some(id.clone()))>
                            <span class="k" style=format!("color:{}", tool_kinds::color(&x.name))>{tool_kinds::label(&x.name)}</span>
                            <span class="p">{json::string(&x.input, "description")}</span>
                            <ToolMetaView meta=meta />
                        </button>
                    }
                })
                .into_any()
            }
        }
    };

    let map = Memo::new(move |_| {
        let now = chrono::Utc::now();
        session.with_items(|items| {
            agent_map(items)
                .into_iter()
                .filter_map(|(i, depth)| {
                    let a = items[i].as_tool()?;
                    Some((
                        a.id.clone(),
                        depth,
                        wf::dot(a),
                        json::str(&a.input, "description").map(str::to_string).unwrap_or_else(|| t!("Agent.Title")),
                        json::string(&a.input, "subagent_type"),
                        wf::state(a, now),
                    ))
                })
                .collect::<Vec<_>>()
        })
    });
    let map_view = move || {
        let n = map.with(Vec::len);
        (n > 1).then(|| {
            let current = agent_id.clone();
            // The agent map: every subagent of the session, nested under the one that spawned it.
            view! {
                <nav class="section last" aria-labelledby="agent-map-h">
                    <h3 id="agent-map-h" class="cap">{format!("{} · {n}", t!("Agent.Map"))}</h3>
                    <div class="wf">
                        <For each=move || map.get() key=|r| r.clone() let:r>
                            {
                                let (id, depth, dot, name, kind, state) = r;
                                let sel = id == current;
                                view! {
                                    <button type="button" class=if sel { "agent sel" } else { "agent" } style=format!("padding-left:{}px", 14 + 16 * depth)
                                        aria-current=sel.then_some("true") on:click=move |_| ui.selected_tool_id.set(Some(id.clone()))>
                                        <span></span><span class=format!("dot {dot}") aria-hidden="true"></span>
                                        <span class="nm">{name}" "<small>{kind}</small></span>
                                        <span class="m">{state}</span>
                                    </button>
                                }
                            }
                        </For>
                    </div>
                </nav>
            }
        })
    };

    view! {
        <div class="ph"><h2 class="ttl">{t!("Agent.Title")}</h2><span class="sub">{move || agent.with(|a| json::string(&a.input, "description"))}</span>
            <span class="end"><TaskActions session=session tool=agent />
                <button class="btn ghost" title=t!("Action.Close") aria-label=t!("Action.Close") on:click=move |_| ui.selected_tool_id.set(None)><Icon name="x" sm=true /></button>
            </span>
        </div>
        <div class="scroll">
            <div class="section">
                <span class="cap">{t!("Agent.Writes")}</span>
                {writes}
            </div>
            <Show when=move || children.with(|c| !c.is_empty())>
                <details class="section fold-sec">
                    <summary class="cap"><Icon name="right" sm=true class="chev" />{move || format!("{} · {}", t!("Agent.Transcript"), children.with(Vec::len))}</summary>
                    <div class="subrun wfa">
                        <For each=move || children.get() key=|i| *i children=child />
                    </div>
                </details>
            </Show>
            {map_view}
        </div>
    }
}

/// A row read before it exists (never shown: rows are keyed on existing indexes).
fn empty_like() -> ToolItem {
    ToolItem {
        id: String::new(),
        name: String::new(),
        input: serde_json::Value::Null,
        parent_tool_use_id: None,
        state: ToolState::Running,
        result_text: None,
        structured: None,
        started_at: Default::default(),
        ended_at: None,
        task_id: None,
        task_status: None,
        progress: None,
        tokens: 0,
        sub_tool_uses: 0,
        background: false,
        model: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(id: &str, name: &str, parent: Option<&str>) -> Item {
        let mut t = empty_like();
        t.id = id.into();
        t.name = name.into();
        t.parent_tool_use_id = parent.map(str::to_string);
        Item::Tool(t)
    }

    fn text(md: &str, parent: &str) -> Item {
        Item::Text(TextItem { markdown: md.into(), parent_tool_use_id: Some(parent.into()), uuid: None, at: None })
    }

    #[test]
    fn map_is_depth_first() {
        let items = vec![
            tool("a", "Agent", None),
            tool("b", "Bash", None),
            tool("a1", "Agent", Some("a")),
            tool("a11", "Task", Some("a1")),
            tool("c", "Agent", None),
            tool("x", "Read", Some("a")),
        ];
        assert_eq!(agent_map(&items), [(0, 0), (2, 1), (3, 2), (4, 0)]);
    }

    #[test]
    fn last_text_falls_back() {
        let mut a = empty_like();
        a.result_text = Some("result".into());
        let kids = vec![text("one", "a"), tool("t", "Bash", Some("a")), text("two", "a")];
        assert_eq!(last_text(&kids, &a).as_deref(), Some("two"));
        let none: Vec<Item> = vec![];
        assert_eq!(last_text(&none, &a).as_deref(), Some("result"));
        a.structured = Some(json!({"handbackReport": {"text": "report"}}));
        assert_eq!(last_text(&none, &a).as_deref(), Some("report"));
    }
}
