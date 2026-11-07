//! Port of `Components/Session/ToolRow.razor` — spec 07 §19. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::{ask_user, fmt, i18n, json, tool_kinds};
use leptos::prelude::*;
use serde_json::Value;

/// One tool call row (label, target, state, meta); selects it in the inspector.
/// A row only changes with its tool, its selection or the session cwd (fine-grained: no ShouldRender needed).
#[component]
pub fn ToolRow(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)]
    tool: Signal<ToolItem>,
) -> impl IntoView {
    let ui = use_ui();
    let selected = Memo::new(move |_| ui.selected_tool_id.with(|s| tool.with(|t| s.as_deref() == Some(t.id.as_str()))));
    let state = Memo::new(move |_| tool.with(|t| t.state));
    let class = move || match row_class(state.get(), selected.get()) {
        Some(c) => format!("t {c}"),
        None => "t".to_string(),
    };
    let select = move |_| {
        ui.selected_tool_id.set(Some(tool.with_untracked(|t| t.id.clone())));
        ui.inspector_open.set(true); // the row's only effect is the Inspector: a closed one would swallow the click
    };
    let name = Memo::new(move |_| tool.with(|t| t.name.clone()));
    let cwd = super::cwd(session);
    let target = move || cwd.with(|cwd| tool.with(|t| tool_kinds::target(&t.name, &t.input, cwd)));
    let m = Memo::new(move |_| tool.with(meta));
    view! {
        <button type="button" class=class aria-pressed=move || if selected.get() { "true" } else { "false" } on:click=select>
            {move || view! { <Icon name=if selected.get() { "down" } else { "right" } sm=true class="chev"/> }}
            <span class="kind" style=move || format!("color:{}", tool_kinds::color(&name.read())) title=move || name.get()>
                {move || tool_kinds::label(&name.read())}
            </span>
            <span class="p">{target}</span>
            <ToolMetaView meta=m/>
        </button>
    }
}

/// `ToolRow.Class`: Waiting "wait", Denied / NotRun "void", else "sel" when selected.
pub fn row_class(state: ToolState, selected: bool) -> Option<&'static str> {
    match state {
        ToolState::Waiting => Some("wait"),
        ToolState::Denied | ToolState::NotRun => Some("void"),
        _ => selected.then_some("sel"),
    }
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
    let badge = |class, key: &str, title: Option<String>| Some(ToolMeta::Badge { class, text: i18n::t(key), title });
    match tool.state {
        ToolState::Waiting => Some(ToolMeta::Plain(i18n::t("Tool.Waiting"))),
        ToolState::Denied => badge("no", "Tool.Denied", None),
        ToolState::Error => badge("no", "Tool.Error", None),
        ToolState::NotRun => badge("dim", "Tool.NotRun", Some(i18n::t("Tool.NotRunWhy"))),
        ToolState::Running => Some(ToolMeta::Plain("…".into())),
        ToolState::Done => {
            if tool.name == "Write" && tool.structured.as_ref().and_then(|s| json::str(s, "type")) == Some("create") {
                return badge("ok", "Tool.Created", None);
            }
            Some(done(tool)).filter(|m| !m.is_empty()).map(ToolMeta::Plain)
        }
    }
}

/// `(int)` of a JSON number property.
fn num(v: &Value, name: &str) -> Option<i64> {
    json::prop(v, name).and_then(Value::as_f64).map(|d| d as i64)
}

/// `ToolRow.Compute`: the finished meta of a Done tool.
fn done(t: &ToolItem) -> String {
    let s = t.structured.as_ref().unwrap_or(&json::UNDEFINED);
    match t.name.as_str() {
        ask_user::TOOL => {
            let qa = ask_user::answered(t.structured.as_ref());
            if !qa.is_empty() {
                return qa.into_iter().map(|(_, a)| a).collect::<Vec<_>>().join(" · ");
            }
        }
        "Read" => {
            let n = json::prop(s, "file")
                .and_then(|f| num(f, "numLines"))
                .or_else(|| t.result_text.as_ref().map(|r| r.trim_end_matches('\n').split('\n').count() as i64))
                .unwrap_or(0);
            return i18n::plural(n, "Tool.Lines");
        }
        "Grep" | "Glob" => {
            let r = match json::str(s, "mode") {
                Some("content") => num(s, "numLines"),
                Some("count") => num(s, "numMatches"),
                _ => num(s, "numFiles"),
            }
            .or_else(|| t.result_text.as_ref().map(|r| r.split('\n').filter(|l| !l.is_empty()).count() as i64))
            .unwrap_or(0);
            return i18n::plural(r, "Tool.Results");
        }
        "Edit" | "MultiEdit" | "Write" => {
            if let Some(Value::Array(patch)) = json::prop(s, "structuredPatch") {
                let lines: Vec<&str> = patch
                    .iter()
                    .filter_map(|h| match json::prop(h, "lines") {
                        Some(Value::Array(l)) => Some(l),
                        _ => None,
                    })
                    .flatten()
                    .map(|l| l.as_str().unwrap_or(""))
                    .collect();
                let (add, del) = (lines.iter().filter(|l| l.starts_with('+')).count(), lines.iter().filter(|l| l.starts_with('-')).count());
                return format!("+{add} \u{2212}{del}");
            }
        }
        _ => {}
    }
    match t.ended_at.map(|e| (e - t.started_at).num_milliseconds()) {
        Some(ms) if ms >= 50 => fmt::dur(ms),
        _ => String::new(),
    }
}

/// Render a `ToolMeta` (nothing for None).
#[component]
pub fn ToolMetaView(#[prop(into)] meta: Signal<Option<ToolMeta>>) -> impl IntoView {
    move || {
        meta.get().map(|m| match m {
            ToolMeta::Plain(t) => view! { <span class="m">{t}</span> }.into_any(),
            ToolMeta::Badge { class, text, title } => {
                view! { <span class=format!("badge {class}") title=title>{text}</span> }.into_any()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccui_shared::i18n::{set_thread_lang, Lang};
    use serde_json::json;

    fn tool(name: &str, state: ToolState, structured: Option<Value>, result: Option<&str>) -> ToolItem {
        ToolItem {
            id: "t".into(),
            name: name.into(),
            input: json!({}),
            parent_tool_use_id: None,
            state,
            result_text: result.map(Into::into),
            structured,
            started_at: Time::default(),
            ended_at: Some(Time::default() + chrono::Duration::milliseconds(4500)),
            task_id: None,
            task_status: None,
            progress: None,
            tokens: 0,
            sub_tool_uses: 0,
            background: false,
            model: None,
        }
    }

    fn plain(t: &ToolItem) -> String {
        match meta(t) {
            Some(ToolMeta::Plain(s)) => s,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn metas() {
        set_thread_lang(Some(Lang::En));
        assert_eq!(plain(&tool("Read", ToolState::Done, Some(json!({"file":{"numLines":12.0}})), None)), i18n::plural(12, "Tool.Lines"));
        assert_eq!(plain(&tool("Read", ToolState::Done, None, Some("a\nb\n"))), i18n::plural(2, "Tool.Lines"));
        assert_eq!(plain(&tool("Grep", ToolState::Done, Some(json!({"mode":"count","numMatches":3})), None)), i18n::plural(3, "Tool.Results"));
        assert_eq!(plain(&tool("Glob", ToolState::Done, None, Some("a\n\nb"))), i18n::plural(2, "Tool.Results"));
        let edit = json!({"structuredPatch":[{"lines":[" a","-b","+c","+d"]},{"nolines":1}]});
        assert_eq!(plain(&tool("Edit", ToolState::Done, Some(edit), None)), "+2 \u{2212}1");
        assert_eq!(plain(&tool("Bash", ToolState::Done, None, None)), fmt::dur(4500));
        assert!(matches!(meta(&tool("Write", ToolState::Done, Some(json!({"type":"create"})), None)), Some(ToolMeta::Badge { class: "ok", .. })));
        assert!(matches!(meta(&tool("Bash", ToolState::NotRun, None, None)), Some(ToolMeta::Badge { class: "dim", title: Some(_), .. })));
        assert_eq!(plain(&tool("Bash", ToolState::Running, None, None)), "…");
        let mut quick = tool("Bash", ToolState::Done, None, None);
        quick.ended_at = Some(Time::default());
        assert_eq!(meta(&quick), None);
        set_thread_lang(None);
    }

    #[test]
    fn classes() {
        assert_eq!(row_class(ToolState::Waiting, true), Some("wait"));
        assert_eq!(row_class(ToolState::NotRun, true), Some("void"));
        assert_eq!(row_class(ToolState::Done, true), Some("sel"));
        assert_eq!(row_class(ToolState::Done, false), None);
    }
}
