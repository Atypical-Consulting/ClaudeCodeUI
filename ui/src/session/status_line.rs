//! Port of `Components/Session/StatusLine.razor` — spec 07 §22. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::state::{use_app, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::session::SessionMeta;
use ccui_shared::{ask_user, elicit, fmt, i18n, session_rules, t, tool_kinds};
use leptos::prelude::*;

/// What the live status shows (first matching case of `StatusLine.razor`).
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// `<div class="status"><span class="spin">` text `<small aria-hidden?>` small.
    Spin { text: String, small: Option<String>, hidden: bool, wait: bool },
    /// `<div class="status wait"><span class="dot wait">` text.
    Dot { text: String, class: &'static str, dot: &'static str },
    Interrupted(String),
    Error { key: &'static str, subtype: String },
    TurnDone,
    None,
}

/// Facts read from the items (top-level agents of the current turn, last non-hook item a running tool).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemFacts {
    /// (done, total, any running) of the top-level Agent calls after the last UserItem.
    pub agents: (usize, usize, bool),
    /// Label of the last non-Hook item when it is a Running ToolItem.
    pub running_tool: Option<String>,
}

pub fn item_facts(items: &[&Item]) -> ItemFacts {
    let from = items.iter().rposition(|i| matches!(i, Item::User(_))).map_or(0, |i| i + 1);
    let agents: Vec<&ToolItem> = items[from..]
        .iter()
        .filter_map(|i| i.as_tool())
        .filter(|t| t.parent_tool_use_id.is_none() && tool_kinds::is_agent(&t.name))
        .collect();
    let running_tool = match items.iter().rev().find(|i| !matches!(i, Item::Hook(_))) {
        Some(Item::Tool(t)) if t.state == ToolState::Running => Some(tool_kinds::label(&t.name)),
        _ => None,
    };
    ItemFacts {
        agents: (
            agents.iter().filter(|a| a.state == ToolState::Done).count(),
            agents.len(),
            agents.iter().any(|a| a.state == ToolState::Running),
        ),
        running_tool,
    }
}

fn since(t: Time, now: Time) -> String {
    fmt::dur((now - t).num_milliseconds())
}

/// "529 · overloaded · in 4s": what failed and when the next attempt goes.
pub fn retry_text(r: &RetryEvt) -> String {
    [r.error_status.map(|s| s.to_string()), r.error.clone(), Some(t!("Status.RetryIn", fmt::dur(r.delay_ms as i64)))]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The first matching case of the Razor switch.
pub fn line(m: &SessionMeta, facts: &ItemFacts, elsewhere: bool, now: Time) -> Line {
    use SessionStatus::*;
    let spin = |text: String, small: Option<String>, hidden: bool| Line::Spin { text, small, hidden, wait: false };
    let elapsed = || m.turn_started_at.map(|t| since(t, now)).unwrap_or_default();
    match m.status {
        Starting => spin(t!("Status.StartingClaude"), None, false),
        Running | Waiting if m.compacting_since.is_some() => {
            spin(t!("Status.Compacting"), m.compacting_since.map(|c| since(c, now)), true)
        }
        Running if m.retry.is_some() => {
            let r = m.retry.as_ref().unwrap();
            Line::Spin { text: t!("Status.Retry", r.attempt, r.max_retries), small: Some(retry_text(r)), hidden: false, wait: true }
        }
        Running if !m.running_hooks.is_empty() => {
            let h = m.running_hooks.iter().min_by_key(|h| h.at).unwrap();
            let more = if m.running_hooks.len() > 1 {
                format!(" · {}", t!("Status.HooksMore", m.running_hooks.len() - 1))
            } else {
                String::new()
            };
            spin(t!("Status.HookRunning", h.name), Some(format!("{}{more}", since(h.at, now))), true)
        }
        Running if facts.agents.1 > 0 && facts.agents.2 => {
            spin(t!("Status.AgentsWorking"), Some(t!("OfN", facts.agents.0, facts.agents.1)), false)
        }
        Running if facts.running_tool.is_some() => {
            spin(t!("Status.ToolRunning", facts.running_tool.as_deref().unwrap_or_default()), Some(elapsed()), true)
        }
        Running => {
            let thinking = if m.thinking_tokens > 0 {
                format!(" · {}", t!("Status.Thinking", fmt::tokens(m.thinking_tokens as i64)))
            } else {
                String::new()
            };
            spin(t!("Status.Writing"), Some(format!("{}{thinking}", elapsed())), true)
        }
        Waiting => {
            let tool = m.pending.first().map(|p| p.tool.as_str());
            if matches!(tool, Some(ask_user::TOOL) | Some(elicit::TOOL)) {
                return Line::Dot { text: t!("Status.PausedQuestion"), class: "status wait", dot: "wait" };
            }
            let what = match tool {
                Some(n) if tool_kinds::is_edit(n) => t!("Status.WhatEdit"),
                Some(n) if tool_kinds::is_shell(n) => t!("Status.WhatCommand"),
                _ => t!("Status.WhatTool"),
            };
            Line::Dot { text: t!("Status.Paused", what), class: "status wait", dot: "wait" }
        }
        Exited if elsewhere => Line::Dot { text: t!("Status.ElsewhereLine"), class: "status wait", dot: "wait" },
        Exited => Line::Dot {
            text: t!("Status.HistoryLine", i18n::name("Mode.Name.", Some(&m.mode))),
            class: "status",
            dot: "exited",
        },
        Idle if m.last_result_subtype.as_deref() == Some(session_rules::INTERRUPTED) => {
            Line::Interrupted(m.last_turn_ms.map(fmt::dur).unwrap_or_default())
        }
        Idle if m.last_result_subtype.as_deref().is_some_and(|s| s.starts_with("error")) => {
            let subtype = m.last_result_subtype.clone().unwrap_or_default();
            let key = match subtype.as_str() {
                "error_max_turns" => "Status.Err.MaxTurns",
                "error_max_budget_usd" => "Status.Err.Budget",
                "error_during_execution" => "Status.Err.Execution",
                _ => "Status.TurnError",
            };
            Line::Error { key, subtype }
        }
        Idle if m.last_turn_ms.is_some() => Line::TurnDone,
        _ => Line::None,
    }
}

impl Line {
    /// The ticking part (a Spin's small text), kept apart so the line around it is not rebuilt every second.
    fn small(&self) -> Option<String> {
        match self {
            Line::Spin { small, .. } => small.clone(),
            _ => None,
        }
    }

    /// The line with its ticking text blanked (present / absent kept).
    fn shape(&self) -> Line {
        match self {
            Line::Spin { text, small, hidden, wait } => {
                Line::Spin { text: text.clone(), small: small.as_ref().map(|_| String::new()), hidden: *hidden, wait: *wait }
            }
            other => other.clone(),
        }
    }
}

/// Live status under the thread: running / waiting / compacting / retry / hooks / custom statusLine text.
/// Always-present live region: its text changes are announced. Ticking counters are aria-hidden so the reader is not
/// interrupted every second; only their text node moves with the clock (the Ticker's job, active while Running).
/// Without a visible .status it is taken out of the flow (app.css).
#[component]
pub fn StatusLine(session: SessionHandle) -> impl IntoView {
    let app = use_app();
    let facts = Memo::new(move |_| session.with_items(item_facts));
    let running = Memo::new(move |_| session.meta.with(|m| m.status == SessionStatus::Running));
    let now = super::clock(running.into());
    let id = session.id();
    let elsewhere = Memo::new(move |_| app.summary.with(|s| session_rules::running_elsewhere(s, &id)));
    let full = Memo::new(move |_| session.meta.with(|m| line(m, &facts.read(), elsewhere.get(), now.get())));
    let shape = Memo::new(move |_| full.with(Line::shape));
    let small = Memo::new(move |_| full.with(Line::small));
    let current = move || match shape.get() {
        Line::Spin { text, small: has_small, hidden, wait } => view! {
            <div class=if wait { "status wait" } else { "status" }><span class="spin"></span>{text}
                {has_small.map(|_| view! { " "<small aria-hidden=hidden.then_some("true")>{move || small.get()}</small> })}</div>
        }
        .into_any(),
        Line::Dot { text, class, dot } => view! { <div class=class><span class=format!("dot {dot}")></span>{text}</div> }.into_any(),
        Line::Interrupted(d) => view! {
            <div class="status"><Icon name="stop" sm=true/>{t!("Status.Interrupted")}" "<small>{d}</small></div>
        }
        .into_any(),
        Line::Error { key, subtype } => view! { <div class="status c-err-fg" title=subtype>{t!(key)}</div> }.into_any(),
        Line::TurnDone => view! { <span class="sr-only">{t!("Status.TurnDone")}</span> }.into_any(),
        Line::None => ().into_any(),
    };
    // The settings' statusLine command, run by the app: what it printed, as the terminal's footer shows it.
    let status_text = Memo::new(move |_| session.meta.with(|m| m.status_text.clone()));
    let custom = move || status_text.get().map(|c| view! { <div class="status custom" aria-hidden="true"><small>{c}</small></div> });
    // Plugins' $.ui.status lines (system/ui_status): the REPL's status row, one per plugin until it clears its own.
    let plugin_line = Memo::new(move |_| {
        let mut ps = session.meta.with(|m| m.plugin_status.clone());
        ps.sort_by(|a, b| a.0.cmp(&b.0));
        (!ps.is_empty()).then(|| ps.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(" · "))
    });
    let plugins = move || plugin_line.get().map(|s| view! { <div class="status"><small>{s}</small></div> });
    view! { <div role="status" class="status-live">{current}{custom}{plugins}</div> }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, state: ToolState, parent: Option<&str>) -> Item {
        Item::Tool(ToolItem {
            id: "t".into(),
            name: name.into(),
            input: json!({}),
            parent_tool_use_id: parent.map(Into::into),
            state,
            result_text: None,
            structured: None,
            started_at: Time::default(),
            ended_at: None,
            task_id: None,
            task_status: None,
            progress: None,
            tokens: 0,
            sub_tool_uses: 0,
            background: false,
            model: None,
        })
    }

    #[test]
    fn facts() {
        let user = Item::User(UserItem { text: "x".into(), at: Time::default(), ultracode: false, images: None, uuid: None });
        let hook = Item::Hook(HookItem { hook: HookEvt::default() });
        let items = [
            tool("Agent", ToolState::Done, None),
            user,
            tool("Agent", ToolState::Done, None),
            tool("Agent", ToolState::Running, None),
            tool("Read", ToolState::Running, Some("a")),
            hook,
        ];
        let refs: Vec<&Item> = items.iter().collect();
        let f = item_facts(&refs);
        assert_eq!(f.agents, (1, 2, true));
        assert_eq!(f.running_tool, Some(tool_kinds::label("Read")));
    }

    #[test]
    fn lines() {
        let now = Time::default() + chrono::Duration::seconds(10);
        let mut m = SessionMeta { status: SessionStatus::Running, turn_started_at: Some(Time::default()), ..Default::default() };
        let none = ItemFacts::default();
        assert_eq!(line(&m, &none, false, now), Line::Spin { text: t!("Status.Writing"), small: Some(fmt::dur(10_000)), hidden: true, wait: false });
        m.retry = Some(RetryEvt { attempt: 1, max_retries: 3, delay_ms: 4000, error_status: Some(529), error: Some("overloaded".into()) });
        assert!(matches!(line(&m, &none, false, now), Line::Spin { wait: true, .. }));
        assert_eq!(retry_text(m.retry.as_ref().unwrap()), format!("529 · overloaded · {}", t!("Status.RetryIn", fmt::dur(4000))));
        m.status = SessionStatus::Idle;
        m.last_result_subtype = Some("error_max_turns".into());
        assert_eq!(line(&m, &none, false, now), Line::Error { key: "Status.Err.MaxTurns", subtype: "error_max_turns".into() });
        m.status = SessionStatus::Exited;
        assert!(matches!(line(&m, &none, true, now), Line::Dot { dot: "wait", .. }));
    }
}
