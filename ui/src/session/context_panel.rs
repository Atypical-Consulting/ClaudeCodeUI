//! Port of `Components/Session/ContextPanel.razor` — spec 07 §32. Owner: ui-session-inspector.
//! get_context_usage, refreshed when the session goes idle after a turn, then the working folder, the additional
//! folders, bookmarks, the session's memory files and /status.

use super::extra_dirs::{ExtraDirs, LocalFut};
use super::memory_panel::MemoryPanel;
use super::status_panel::StatusPanel;
use crate::common::icon::Icon;
use crate::common::pick_folder::PickFolder;
use crate::ipc;
use crate::state::{use_app, SessionHandle};
use ccui_shared::api::{Bookmark, TrustPrompt};
use ccui_shared::model::*;
use ccui_shared::{fmt, i18n, json, session_rules, t, text};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

/// The CLI takes 100k to 1M.
const WINDOWS: [&str; 5] = ["auto", "200000", "400000", "600000", "1000000"];

/// One used category of the context bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    pub color: &'static str,
    pub tokens: i64,
    pub mcp: bool,
}

/// A JSON number truncated to i64, else 0 (`(long)n.GetDouble()`).
pub fn long(v: &Value, k: &str) -> i64 {
    json::prop(v, k).and_then(Value::as_f64).map_or(0, |d| d as i64)
}

/// `mcpTools[]` entries with isLoaded true.
pub fn loaded_mcp(ctx: &Value) -> Vec<&Value> {
    match json::prop(ctx, "mcpTools") {
        Some(Value::Array(a)) => a.iter().filter(|t| json::prop(t, "isLoaded") == Some(&Value::Bool(true))).collect(),
        _ => Vec::new(),
    }
}

/// Used categories only ("deferred" tools are not in the context, "buffer" and "free" count as free).
pub fn rows(ctx: &Value) -> Vec<Row> {
    let mcp = loaded_mcp(ctx).len();
    let Some(Value::Array(cats)) = json::prop(ctx, "categories") else { return Vec::new() };
    cats.iter()
        .filter(|k| json::str(k, "kind") == Some("used") && long(k, "tokens") > 0)
        .map(|k| {
            let tokens = long(k, "tokens");
            let name = json::string(k, "name");
            let row = |label: String, color, mcp| Row { label, color, tokens, mcp };
            match name.as_str() {
                "System prompt" => row(t!("Ctx.Cat.SystemPrompt"), "var(--fg-4)", false),
                "System tools" => row(t!("Ctx.Cat.SystemTools"), "var(--syn-fn)", false),
                n if n.starts_with("MCP") => {
                    row(if mcp > 0 { t!("Ctx.Cat.McpN", mcp) } else { t!("Ctx.Cat.Mcp") }, "var(--syn-kw)", true)
                }
                "Memory files" => row(t!("Ctx.Cat.Memory"), "var(--syn-attr)", false),
                "Custom agents" => row(t!("Ctx.Cat.Agents"), "var(--syn-type)", false),
                "Skills" => row(t!("Ctx.Cat.Skills"), "var(--syn-num)", false),
                "Messages" => row(t!("Ctx.Cat.Messages"), "var(--syn-str)", false),
                _ => row(name.clone(), "var(--fg-3)", false),
            }
        })
        .collect()
}

/// Rows grouped by label (first color, summed tokens), in first-seen order (`GroupBy`).
pub fn legend(rows: &[Row]) -> Vec<(String, &'static str, i64)> {
    let mut out: Vec<(String, &'static str, i64)> = Vec::new();
    for r in rows {
        match out.iter_mut().find(|g| g.0 == r.label) {
            Some(g) => g.2 += r.tokens,
            None => out.push((r.label.clone(), r.color, r.tokens)),
        }
    }
    out
}

/// `(tokens * 100.0 / max).ToString("0.##", Invariant)`.
pub fn width(tokens: i64, max: i64) -> String {
    let s = format!("{:.2}", tokens as f64 * 100.0 / max as f64);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Default inspector: context usage, cost, extra dirs, memory, status, bookmarks, artifacts.
#[component]
pub fn ContextPanel(
    session: SessionHandle,
) -> impl IntoView {
    let app = use_app();
    let home_sep = StoredValue::new(app.boot.with_value(|b| (b.home.clone(), b.sep.clone())));
    let short = move |p: &str| home_sep.with_value(|(home, sep)| text::short(p, home, sep));
    let compacting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let trust = RwSignal::new(None::<TrustPrompt>);
    let cwd_note = RwSignal::new(None::<String>);
    let moving = RwSignal::new(false);
    let marks = RwSignal::new(Vec::<Bookmark>::new());
    let status = Memo::new(move |_| session.meta.with(|m| m.status));
    let busy_status = move || matches!(status.get(), SessionStatus::Starting | SessionStatus::Running | SessionStatus::Waiting);
    let idle = move || matches!(status.get(), SessionStatus::Idle | SessionStatus::Exited);
    let cwd = Memo::new(move |_| session.meta.with(|m| m.cwd.clone()));

    // get_context_usage once per finished turn, while idle with a process.
    let refreshed_for = StoredValue::new(None::<Option<Time>>);
    Effect::new(move |_| {
        let (ready, at) = session.meta.with(|m| (m.has_process && m.status == SessionStatus::Idle, m.last_result_at));
        if !ready || refreshed_for.get_value() == Some(at) {
            return;
        }
        refreshed_for.set_value(Some(at));
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_refresh_context(&id).await {
                leptos::logging::error!("[{id}] get_context_usage : {e}");
            }
        });
    });
    // A reply bookmarked in the thread shows up here at once.
    Effect::new(move |_| {
        app.bookmarks.track();
        let id = session.id();
        spawn_local(async move {
            if let Ok(b) = ipc::bookmarks_of(&id).await {
                marks.set(b);
            }
        });
    });

    // Plain user text: slash commands run locally in the CLI. Send awaits the flag settings before the turn starts,
    // so the status stays Idle meanwhile: refuse a second click here.
    let send = move |text: String, fail: &'static str| {
        if compacting.get_untracked() {
            return;
        }
        compacting.set(true);
        error.set(None);
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_send(&id, &text, &[]).await {
                error.set(Some(t!(fail, e)));
            }
            compacting.set(false);
        });
    };
    let move_to = move |path: String, trusted: Option<String>| {
        moving.set(true);
        cwd_note.set(None);
        let id = session.id();
        spawn_local(async move {
            match ipc::session_set_cwd(&id, &path, trusted.as_deref()).await {
                Ok(t) => {
                    let none = t.is_none();
                    trust.set(t);
                    if none {
                        let now = session.meta.with_untracked(|m| m.cwd.clone());
                        cwd_note.set(Some(t!("Cwd.Moved", short(&now))));
                    }
                }
                Err(e) => {
                    trust.set(None);
                    cwd_note.set(Some(t!("Session.ActionFailed", e.trim_end())));
                }
            }
            moving.set(false);
        });
    };

    let numbers = Memo::new(move |_| {
        session.meta.with(|m| {
            session.heavy.with(|h| {
                let c = h.context.as_ref();
                let max = c.map_or(m.context_window.unwrap_or(0), |c| long(c, "maxTokens"));
                let total = c.map_or(m.context_tokens, |c| long(c, "totalTokens"));
                (max, total)
            })
        })
    });
    let has_process = Memo::new(move |_| session.meta.with(|m| m.has_process));
    let context = Memo::new(move |_| session.heavy.with(|h| h.context.clone()));
    let sub = move || {
        let (max, total) = numbers.get();
        (max > 0).then(|| {
            view! { <span class="sub">{format!("{} / {} · {}", fmt::tokens(total), fmt::tokens(max), fmt::pct(total as f64 / max as f64))}</span> }
        })
    };

    let usage = move || {
        let (max, total) = numbers.get();
        let Some(ctx) = context.get().filter(|_| max > 0) else {
            let k = if has_process.get() { "Ctx.Unknown" } else { "Ctx.NoProcess" };
            return view! { <div class="section"><p>{t!(k)}</p></div> }.into_any();
        };
        let rows = rows(&ctx);
        let mcp = loaded_mcp(&ctx);
        let mcp_tokens: i64 = rows.iter().filter(|r| r.mcp).map(|r| r.tokens).sum();
        let bar = rows
            .iter()
            .map(|r| view! { <i style=format!("width:{}%;background:{}", width(r.tokens, max), r.color)></i> })
            .collect_view();
        let legend = legend(&rows)
            .into_iter()
            .map(|(label, color, tokens)| view! { <i style=format!("background:{color}")></i><span>{label}</span><span>{fmt::tokens(tokens)}</span> })
            .collect_view();
        let threshold = long(&ctx, "autoCompactThreshold");
        let auto = (json::prop(&ctx, "isAutoCompactEnabled") == Some(&Value::Bool(true)) && threshold > 0).then(|| {
            let opts = WINDOWS
                .iter()
                .map(|w| {
                    let label = if *w == "auto" { t!("Ctx.WindowAuto") } else { fmt::tokens(w.parse().unwrap_or(0)) };
                    view! { <option value=*w>{label}</option> }
                })
                .collect_view();
            view! {
                <span class="why" title=t!("Ctx.AutoTitle")>{t!("Ctx.Auto", fmt::pct(threshold as f64 / max as f64))}</span>
                // /autocompact <tokens|auto>: the CLI saves the window for the model (modelSettings) and answers in the thread.
                <span class="picks"><select aria-label=t!("Ctx.Window") title=t!("Ctx.WindowHint") disabled=move || compacting.get() || !idle()
                    prop:value="" on:change=move |e| {
                        let w = event_target_value(&e);
                        // Always back to the placeholder: the pick is an action, not a setting shown here.
                        if let Some(sel) = e.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlSelectElement>(t).ok()) {
                            sel.set_value("");
                        }
                        if !w.is_empty() {
                            send(format!("/autocompact {w}"), "Ctx.WindowFailed");
                        }
                    }>
                    <option value="">{t!("Ctx.Window")}</option>
                    {opts}
                </select></span>
            }
        });
        let mcp_sec = (!mcp.is_empty()).then(|| {
            let n = mcp.len();
            let mut servers: Vec<&str> = mcp.iter().map(|t| json::str(t, "serverName").unwrap_or("")).collect();
            servers.sort_unstable();
            servers.dedup();
            let k = if n == 1 { "Ctx.Loaded1" } else { "Ctx.LoadedN" };
            let line = format!(
                "{} {}",
                t!(k, i18n::plural(n as i64, "Tools"), i18n::plural(servers.len() as i64, "Ctx.Servers")),
                t!("Ctx.DisableHint")
            );
            view! {
                <div class="section">
                    <span class="cap">{t!("Ctx.McpWeight", fmt::tokens(mcp_tokens))}</span>
                    <p>{line}</p>
                    <div><a class="btn" href=format!("extensions?session={}", session.id())>{t!("Ctx.ManageMcp")}</a></div>
                </div>
            }
        });
        view! {
            <div class="section">
                <div class="ctx">
                    <div class="ctxbar" aria-hidden="true">{bar}</div>
                    <div class="legend">
                        {legend}
                        <i style="background:var(--seam)"></i><span>{t!("Ctx.Free")}</span><span>{fmt::tokens((max - total).max(0))}</span>
                    </div>
                </div>
                <div class="acts">
                    <button class="btn" disabled=move || compacting.get() || !idle() on:click=move |_| send("/compact".into(), "Ctx.CompactFailed")>
                        <Icon name="compress" sm=true />{t!("Ctx.CompactNow")}
                    </button>
                    {auto}
                </div>
                {move || error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> })}
            </div>
            {mcp_sec}
        }
        .into_any()
    };

    // The last API call's usage: what of its prompt came from the prompt cache, and what it (re)wrote there.
    let cache_line = Memo::new(move |_| {
        session.meta.with(|m| {
            (m.context_tokens > 0 && m.cache_read + m.cache_write > 0)
                .then(|| t!("Ctx.CacheLine", fmt::pct(m.cache_read as f64 / m.context_tokens as f64), fmt::tokens(m.cache_write)))
        })
    });
    let cache = move || {
        cache_line.get().map(|line| {
                view! {
                    <div class="section">
                        <span class="cap">{t!("Ctx.Cache")}</span>
                        <p title=t!("Ctx.CacheHint")>{line}</p>
                    </div>
                }
        })
    };

    let trust_view = move || {
        trust.get().map(|tp| {
            let msg = match &tp.root {
                Some(root) => t!("Cwd.TrustRoot", tp.directory, root),
                None => t!("Cwd.Trust", tp.directory),
            };
            let dir = tp.directory.clone();
            view! {
                <p role="alert">{msg}</p>
                <div class="acts">
                    <button type="button" class="btn primary" disabled=moving on:click=move |_| move_to(dir.clone(), Some(dir.clone()))>{t!("New.Trust")}</button>
                    <button type="button" class="btn ghost" on:click=move |_| trust.set(None)>{t!("Action.Cancel")}</button>
                </div>
            }
        })
    };

    let add_dir = Callback::new(move |d: String| -> LocalFut<Result<(), String>> {
        let id = session.id();
        Box::pin(async move { ipc::session_add_dir(&id, &d).await })
    });
    // Rebuilt only when the confirm step changes (a risky mode edits additional folders unasked).
    let confirm_key = Memo::new(move |_| session.meta.with(|m| session_rules::risky(&m.mode).then(|| m.mode.clone())));
    // Idle, or no process (the folder then goes into --add-dir at the next launch): the only states the probe covers.
    let dirs_view = move || {
        let dirs = Signal::derive(move || session.meta.with(|m| m.add_dirs.clone()));
        let disabled = Signal::derive(busy_status);
        match confirm_key.get() {
            Some(mode) => {
                let c = t!("Dirs.ConfirmRisky", i18n::name("Mode.Name.", Some(&mode)));
                view! { <ExtraDirs id="ctx-add-dir" dirs=dirs cwd=cwd on_add=add_dir confirm=c disabled=disabled /> }.into_any()
            }
            None => view! { <ExtraDirs id="ctx-add-dir" dirs=dirs cwd=cwd on_add=add_dir disabled=disabled /> }.into_any(),
        }
    };

    // Bookmarked replies (Thread): a link scrolls the thread to the reply (its id is m-<uuid>).
    let bookmarks = move || {
        let ms = marks.get();
        (!ms.is_empty()).then(|| {
            let id = session.id();
            let items = ms
                .into_iter()
                .map(|b| view! { <li><a href=format!("session/{id}#m-{}", b.uuid)>{b.gist}</a></li> })
                .collect_view();
            view! {
                <div class="section">
                    <span class="cap" id="bm-cap">{t!("Bookmark.Title")}</span>
                    <ul class="bms" aria-labelledby="bm-cap">{items}</ul>
                </div>
            }
        })
    };

    view! {
        <div class="ph"><h2 class="ttl">{t!("Ctx.Title")}</h2>{sub}</div>
        <div class="scroll">
            {usage}
            {cache}
            <div class="section">
                // /cd: session_set_cwd. A folder the CLI never trusted asks first, as the terminal does.
                <span class="cap" id="cwd-cap">{t!("Cwd.Label")}</span>
                <p class="cwd-now" title=move || cwd.get()>
                    <PickFolder start=Signal::derive(move || cwd.get()) on_pick=Callback::new(move |p| move_to(p, None))
                        on_error=Callback::new(move |m| cwd_note.set(Some(m))) disabled=Signal::derive(move || moving.get() || busy_status()) />
                    " "{move || short(&cwd.get())}
                </p>
                {trust_view}
                {move || cwd_note.get().map(|n| view! { <p role="status">{n}</p> })}
            </div>
            <div class="section">
                <label class="cap" for="ctx-add-dir">{t!("Dirs.Label")}</label>
                <p>{t!("Dirs.LiveHint")}</p>
                {dirs_view}
            </div>
            {bookmarks}
            <MemoryPanel session=session />
            <StatusPanel session=session />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_rows() {
        i18n::set_thread_lang(Some(i18n::Lang::En));
        let ctx = json!({
            "categories": [
                {"name": "System prompt", "kind": "used", "tokens": 3000},
                {"name": "MCP tools", "kind": "used", "tokens": 1500.0},
                {"name": "MCP tools (deferred)", "kind": "deferred", "tokens": 900},
                {"name": "Messages", "kind": "used", "tokens": 0},
                {"name": "Odd", "kind": "used", "tokens": 10},
                {"name": "Free space", "kind": "free", "tokens": 100000}
            ],
            "mcpTools": [{"serverName": "gh", "isLoaded": true}, {"serverName": "gh", "isLoaded": false}]
        });
        let r = rows(&ctx);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], Row { label: "system prompt".into(), color: "var(--fg-4)", tokens: 3000, mcp: false });
        assert_eq!(r[1].label, "MCP tools · 1");
        assert!(r[1].mcp);
        assert_eq!(r[2].color, "var(--fg-3)");
        assert_eq!(loaded_mcp(&ctx).len(), 1);
        let g = legend(&[r[0].clone(), r[0].clone(), r[2].clone()]);
        assert_eq!(g, [("system prompt".to_string(), "var(--fg-4)", 6000), ("Odd".to_string(), "var(--fg-3)", 10)]);
        i18n::set_thread_lang(None);
    }

    #[test]
    fn widths() {
        assert_eq!(width(1, 3), "33.33");
        assert_eq!(width(1, 2), "50");
        assert_eq!(width(1, 8), "12.5");
        assert_eq!(width(0, 8), "0");
    }
}
