//! Port of `Components/Session/ToolDetail.razor` — spec 07 §31. Owner: ui-session-inspector.
//! Selected tool: Diff (edits, and a Bash command's bashEditDiff) or Output, Input, Raw JSON, and a .kv footer.

use super::task_actions::TaskActions;
use crate::common::diff_view::DiffView;
use crate::common::file_view::{numbered, FileView};
use crate::common::icon::Icon;
use crate::common::markdown::Markdown;
use crate::ipc;
use crate::layout::command_palette::escape_data;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::edit_diff::{self, DiffKind, FileDiff};
use ccui_shared::model::*;
use ccui_shared::{ask_user, fmt, json, mcp, t, tool_kinds};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};

/// `ToolDetail.Pretty(json)`: indented JSON for display.
pub fn pretty(json: &serde_json::Value) -> String {
    // 2-space indent, non-ASCII and `<` kept as is: what .NET's relaxed-escaping indented writer produces.
    serde_json::to_string_pretty(json).unwrap_or_default()
}

/// The tool_use block and its result, as the CLI sent them (rebuilt: the raw lines are not kept).
pub fn raw(tool: &ToolItem) -> String {
    let mut o = serde_json::Map::new();
    o.insert(
        "tool_use".into(),
        json!({"type": "tool_use", "id": tool.id, "name": tool.name, "input": tool.input}),
    );
    if let Some(r) = &tool.result_text {
        let err = matches!(tool.state, ToolState::Error | ToolState::Denied | ToolState::NotRun);
        o.insert(
            "tool_result".into(),
            json!({"type": "tool_result", "tool_use_id": tool.id, "content": r, "is_error": err}),
        );
    }
    if let Some(s) = &tool.structured {
        o.insert("tool_use_result".into(), s.clone());
    }
    pretty(&Value::Object(o))
}

/// A Read result's line range, "first–last".
pub fn read_range(tool: &ToolItem) -> Option<String> {
    if tool.name != "Read" {
        return None;
    }
    let lines = numbered(tool.result_text.as_deref().unwrap_or(""));
    Some(format!("{}–{}", lines.first()?.0, lines.last()?.0))
}

/// The diff a finished (or replayed) edit shows without asking the backend: the CLI's structuredPatch, else a
/// created file's content. None = from the input (`edit_diff_from_input`).
pub fn known_diff(structured: Option<&Value>) -> Option<Vec<edit_diff::DiffLine>> {
    let s = structured?;
    if let Some(Value::Array(p)) = json::prop(s, "structuredPatch") {
        if !p.is_empty() {
            return Some(edit_diff::from_patch(&Value::Array(p.clone())));
        }
    }
    (json::str(s, "type") == Some("create")).then(|| edit_diff::created(json::str(s, "content").unwrap_or("")))
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

const NOTE: &str = "margin:0;color:var(--fg-4);font-size:13px";

/// Selected tool: input, diff (from_patch / from_bash / edit_diff_from_input), output (FileView), MCP sign-in again.
#[component]
pub fn ToolDetail(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)] tool: Signal<ToolItem>,
) -> impl IntoView {
    let ui = use_ui();
    let tab = RwSignal::new(0u8);
    let auth_busy = RwSignal::new(false);
    let auth_url = RwSignal::new(None::<String>);
    let auth_note = RwSignal::new(None::<String>);
    let cwd = Memo::new(move |_| session.meta.with(|m| m.cwd.clone()));

    // What the panel depends on: re-rendered per (state, result, structured), not per token.
    let shape = Memo::new(move |_| tool.with(|t| (t.state, t.result_text.clone(), t.structured.clone())));
    let bash = Memo::new(move |_| shape.with(|s| s.2.as_ref().map(edit_diff::from_bash).unwrap_or_default()));
    let edit = Memo::new(move |_| tool.with(|t| tool_kinds::is_edit(&t.name)) || bash.with(|b| !b.is_empty()));
    let diffs = RwSignal::new(Vec::<FileDiff>::new());
    let diff_state = StoredValue::new(None::<ToolState>);
    Effect::new(move |_| {
        let state = shape.with(|s| s.0);
        if !edit.get() || diff_state.get_value() == Some(state) {
            return;
        }
        diff_state.set_value(Some(state));
        let b = bash.get_untracked();
        if !b.is_empty() {
            diffs.set(b);
            return;
        }
        // After the fact: the CLI's structuredPatch; a created file has none. Before (or replayed without it): from the input.
        let t = tool.get_untracked();
        let path = json::string(&t.input, "file_path");
        match known_diff(t.structured.as_ref()) {
            Some(lines) => diffs.set(vec![FileDiff { path, lines }]),
            None => spawn_local(async move {
                match ipc::edit_diff_from_input(&t.name, &t.input).await {
                    Ok(lines) => diffs.set(vec![FileDiff { path, lines }]),
                    Err(e) => leptos::logging::error!("edit_diff_from_input: {e}"),
                }
            }),
        }
    });

    // mcp_clear_auth then mcp_authenticate; the CLI reconnects the server once the browser is done.
    let sign_in_again = move |server: String| {
        auth_busy.set(true);
        auth_note.set(None);
        let id = session.id();
        spawn_local(async move {
            match ipc::session_sign_in_again(&id, &server).await {
                Ok(s) => match (s.url, s.callback_expected) {
                    (Some(u), true) => auth_url.set(Some(u)),
                    (None, _) => auth_note.set(Some(t!("Mcp.SignedIn", server))),
                    (Some(_), false) => auth_note.set(Some(t!("Ext.AuthTerminal", server))),
                },
                Err(e) => auth_note.set(Some(t!("Ext.ActionFailed", e.trim_end()))),
            }
            auth_busy.set(false);
        });
    };

    let sub = move || {
        tool.with(|t| match json::str(&t.input, "file_path") {
            Some(fp) => format!("{} · {}", t.name, file_name(fp)),
            None => format!("{} · {}", t.name, tool_kinds::target(&t.name, &t.input, &cwd.get())),
        })
    };
    let tabs = move || {
        let first = if tool.with(|t| tool_kinds::is_edit(&t.name)) { t!("Tool.Diff") } else { t!("Tool.Output") };
        [(0u8, first), (1, t!("Tool.Input")), (2, t!("Tool.RawJson"))]
            .into_iter()
            .map(|(i, label)| {
                let on = move || tab.get() == i;
                view! {
                    <button id=format!("td-tab-{i}") class=move || if on() { "tab on" } else { "tab" } role="tab"
                        aria-selected=move || if on() { "true" } else { "false" } aria-controls="td-panel"
                        tabindex=move || if on() { "0" } else { "-1" } on:click=move |_| tab.set(i)>{label}</button>
                }
            })
            .collect_view()
    };

    let mcp_block = move |t: &ToolItem| {
        if t.state != ToolState::Error {
            return None;
        }
        let server = session.heavy.with_untracked(|h| mcp::server_of(&t.name, &mcp::names(h)))?;
        // A failed MCP call: its server's page (status, Retry), and for a refused credential a fresh sign-in.
        // claude.ai connectors are managed by claude.ai: no re-auth from here.
        let reauth = mcp::auth_error(t.result_text.as_deref()) && !server.starts_with("claude.ai ");
        let href = format!("extensions?tab=mcp&session={}&server={}", session.id(), escape_data(&server));
        let s2 = server.clone();
        let auth = reauth.then(|| {
            view! {
                <p>{t!("Mcp.AuthFailed", server)}</p>
                {move || match (auth_url.get(), auth_note.get()) {
                    (Some(u), _) => Some(view! {
                        <p role="status">{t!("Mcp.AuthWaiting")}</p>
                        <div class="acts"><a class="btn primary" href=u target="_blank" rel="noopener noreferrer">{t!("Ext.OpenSignIn")}</a></div>
                    }.into_any()),
                    (None, Some(n)) => Some(view! { <p role="status">{n}</p> }.into_any()),
                    _ => None,
                }}
            }
        });
        let button = move || {
            (reauth && auth_url.with(Option::is_none)).then(|| {
                let s = s2.clone();
                view! {
                    <button class="btn primary" disabled=auth_busy on:click=move |_| sign_in_again(s.clone())>
                        <Icon name="plug" sm=true />{t!("Mcp.SignInAgain")}
                    </button>
                }
            })
        };
        Some(view! {
            {auth}
            <div class="acts">{button}<a class="btn" href=href>{t!("Mcp.Servers")}</a></div>
        })
    };

    let panel = move || {
        let which = tab.get();
        shape.track();
        let t = tool.get_untracked();
        match which {
            1 => {
                return view! { <div style="padding:16px 16px 0"><Markdown text=format!("```json\n{}\n```", pretty(&t.input)) /></div> }
                    .into_any()
            }
            2 => return view! { <div style="padding:16px 16px 0"><Markdown text=format!("```json\n{}\n```", raw(&t)) /></div> }.into_any(),
            _ => {}
        }
        let result = t.result_text.clone();
        if edit.get() && !matches!(t.state, ToolState::Denied | ToolState::Error | ToolState::NotRun) {
            // A Bash command that changed files: its output first, then what it changed.
            let out = (!tool_kinds::is_edit(&t.name)).then(|| {
                let r = result.clone().filter(|r| !r.is_empty()).unwrap_or_else(|| t!("Tool.NoOutput"));
                view! { <pre class="cmdlog" style="margin:0;overflow:auto">{r}</pre> }
            });
            let more = t
                .structured
                .as_ref()
                .and_then(|s| json::prop(s, "bashEditDiff"))
                .map(|b| json::long(b, "moreFiles"))
                .filter(|m| *m > 0)
                .map(|m| view! { <p style=NOTE>{t!("Diff.MoreFiles", m)}</p> });
            let views = move || {
                let ds = diffs.get();
                if ds.iter().any(|d| !d.lines.is_empty()) {
                    let c = cwd.get_untracked();
                    ds.into_iter()
                        .map(|d| view! { <DiffView path=tool_kinds::relative(&d.path, &c) lines=d.lines /> })
                        .collect_view()
                        .into_any()
                } else {
                    view! { <p style=NOTE>{t!("Tool.NoChanges")}</p> }.into_any()
                }
            };
            return view! { <div class="diffs" style="padding:16px">{out}{views}{more}</div> }.into_any();
        }
        match t.state {
            ToolState::NotRun => {
                return view! {
                    <div style="padding:16px"><p style="margin:0 0 12px;color:var(--fg-3);font-size:13px">{t!("Tool.NotRunWhy")}</p>
                        <pre class="cmdlog" style="margin:0;overflow:auto">{result}</pre></div>
                }
                .into_any()
            }
            ToolState::Error | ToolState::Denied => {
                return view! { <div style="padding:16px"><div class="errbox">{result}</div>{mcp_block(&t)}</div> }.into_any()
            }
            _ => {}
        }
        if t.name == ask_user::TOOL {
            let answered = ask_user::answered(t.structured.as_ref());
            if !answered.is_empty() {
                let rows = answered.into_iter().map(|(q, a)| view! { <dt>{q}</dt><dd>{a}</dd> }).collect_view();
                return view! { <div class="section last"><dl class="ask-done">{rows}</dl></div> }.into_any();
            }
        }
        if t.name == "Read" {
            if let Some(read) = result.clone() {
                return view! { <FileView path=json::string(&t.input, "file_path") text=read /> }.into_any();
            }
        }
        let r = match result.filter(|r| !r.is_empty()) {
            Some(r) => r,
            None if t.state == ToolState::Done => t!("Tool.NoOutput"),
            None => t!("Bg.NoOutput"),
        };
        view! { <div style="padding:16px"><pre class="cmdlog" style="margin:0;overflow:auto">{r}</pre></div> }.into_any()
    };

    let kv = move || {
        let t = tool.get();
        let c = cwd.get();
        let file = json::str(&t.input, "file_path").map(|_| {
            view! { <span>{t!("Tool.Kv.File")}</span><span>{tool_kinds::target(&t.name, &t.input, &c)}</span> }
        });
        let range = read_range(&t).map(|r| view! { <span>{t!("Tool.Kv.Lines")}</span><span>{r}</span> });
        let diff_kv = diffs.with(|ds| {
            let total: usize = ds.iter().map(|d| d.lines.len()).sum();
            (edit.get() && total > 0).then(|| {
                let count = |k| ds.iter().map(|d| d.lines.iter().filter(|l| l.kind == k).count()).sum::<usize>();
                let s = format!("+{} −{}", count(DiffKind::Add), count(DiffKind::Del));
                view! { <span>{t!("Tool.Kv.Diff")}</span><span>{s}</span> }
            })
        });
        let status = match t.state {
            ToolState::Done => {
                let k = if tool_kinds::is_edit(&t.name) { "Tool.Applied" } else { "Tool.Done" };
                view! { <span class="c-ok">{t!(k)}</span> }.into_any()
            }
            ToolState::Denied => view! { <span class="c-err">{t!("Tool.Denied")}</span> }.into_any(),
            ToolState::Error => view! { <span class="c-err">{t!("Tool.Error")}</span> }.into_any(),
            ToolState::NotRun => view! { <span>{t!("Tool.NotRun")}</span> }.into_any(),
            ToolState::Waiting => view! { <span class="c-wait">{t!("Tool.Waiting")}</span> }.into_any(),
            ToolState::Running => view! { <span>{t!("Status.Running")}</span> }.into_any(),
        };
        let dur = t.ended_at.map(|end| {
            view! { <span>{t!("Tool.Kv.Duration")}</span><span>{fmt::dur((end - t.started_at).num_milliseconds())}</span> }
        });
        view! {
            <span>{t!("Tool.Kv.Tool")}</span><span>{t.name.clone()}</span>
            {file}
            {range}
            {diff_kv}
            <span>{t!("Tool.Kv.Status")}</span>
            {status}
            {dur}
        }
    };

    view! {
        <div class="ph"><h2 class="ttl">{t!("Inspector")}</h2><span class="sub">{sub}</span>
            <span class="end"><TaskActions session=session tool=tool />
                <button class="btn ghost" title=t!("Action.Close") aria-label=t!("Action.Close") on:click=move |_| ui.selected_tool_id.set(None)><Icon name="x" sm=true /></button>
            </span>
        </div>
        <div class="tabs" role="tablist" aria-label=t!("Inspector")>{tabs}</div>
        <div id="td-panel" role="tabpanel" aria-labelledby=move || format!("td-tab-{}", tab.get()) tabindex="0" class="scroll">{panel}</div>
        <div class="section" style="margin-top:auto;border-top:1px solid var(--seam);border-bottom:0">
            <div class="kv">{kv}</div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, result: Option<&str>) -> ToolItem {
        ToolItem {
            id: "t1".into(),
            name: name.into(),
            input: json!({"file_path": "/w/a.rs"}),
            parent_tool_use_id: None,
            state: ToolState::Error,
            result_text: result.map(str::to_string),
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

    #[test]
    fn pretty_and_raw() {
        assert_eq!(pretty(&json!({"a": "<é>", "b": [1]})), "{\n  \"a\": \"<é>\",\n  \"b\": [\n    1\n  ]\n}");
        let r = raw(&tool("Read", Some("x")));
        let v: Value = serde_json::from_str(&r).unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["tool_use", "tool_result"]);
        assert_eq!(v["tool_result"]["is_error"], json!(true));
        assert!(!raw(&tool("Read", None)).contains("tool_result"));
    }

    #[test]
    fn read_lines_and_diffs() {
        assert_eq!(read_range(&tool("Read", Some("     3→a\n     4→b\n    12→c"))).as_deref(), Some("3–12"));
        assert_eq!(read_range(&tool("Read", Some("nothing"))), None);
        assert_eq!(read_range(&tool("Bash", Some("1→a"))), None);
        assert!(known_diff(None).is_none());
        assert!(known_diff(Some(&json!({"structuredPatch": []}))).is_none());
        assert_eq!(known_diff(Some(&json!({"type": "create", "content": "a\nb"}))).map(|d| d.len()), Some(edit_diff::created("a\nb").len()));
        let p = json!({"structuredPatch": [{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-a","+b"]}]});
        assert_eq!(known_diff(Some(&p)).map(|d| d.len()), Some(3));
    }
}
