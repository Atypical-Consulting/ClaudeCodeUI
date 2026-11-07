//! Port of `Components/Session/FileCard.razor` — spec 07 §9. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::ipc;
use crate::session::tool_row::{meta, ToolMetaView};
use crate::state::{use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::{json, t, tool_kinds};
use leptos::prelude::*;
use serde_json::Value;

/// SendUserFile card: files (reveal_path), caption. Each file has "Show in folder" (the OS file manager selects it;
/// a file the model picked is never opened for you). The header selects the call.
#[component]
pub fn FileCard(
    session: SessionHandle,
    /// Reactive (`SessionHandle::tool_at` / `tool_by_id`): Running → Done, live tokens re-render in place.
    #[prop(into)]
    tool: Signal<ToolItem>,
) -> impl IntoView {
    let ui = use_ui();
    let error = RwSignal::new(None::<String>);
    let selected = Memo::new(move |_| ui.selected_tool_id.with(|s| tool.with(|t| s.as_deref() == Some(t.id.as_str()))));
    let select = move |_| {
        ui.selected_tool_id.set(Some(tool.with_untracked(|t| t.id.clone())));
        ui.inspector_open.set(true);
    };
    let reveal = move |path: String| {
        error.set(None);
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::reveal_path(&path).await {
                let _ = error.try_set(Some(t!("File.RevealFailed", e)));
            }
        });
    };
    let cwd = super::cwd(session);
    let files = move || {
        let cwd = cwd.get();
        tool.with(|t| files(&t.input, &cwd))
            .into_iter()
            .map(|f| {
                let name = file_name(&f).to_string();
                let rel = tool_kinds::relative(&f, &cwd);
                let path = f.clone();
                view! {
                    <li><code title=f>{rel}</code>
                        <button type="button" class="btn ghost" aria-label=t!("File.RevealOf", name)
                            on:click=move |_| reveal(path.clone())><Icon name="folder" sm=true/>{t!("File.Reveal")}</button></li>
                }
            })
            .collect_view()
    };
    let caption = move || tool.with(|t| json::str(&t.input, "caption").filter(|c| !c.is_empty()).map(str::to_string));
    view! {
        <div class="filecard">
            <button type="button" class=move || if selected.get() { "fh sel" } else { "fh" }
                aria-pressed=move || if selected.get() { "true" } else { "false" } on:click=select>
                <Icon name="paperclip" sm=true/><b>{t!("File.Sent")}</b><ToolMetaView meta=Signal::derive(move || tool.with(meta))/>
            </button>
            <ul>{files}</ul>
            {move || caption().map(|c| view! { <p class="prose">{c}</p> })}
            {move || error.get().map(|e| view! { <p class="errbox" role="alert">{e}</p> })}
        </div>
    }
}

/// `Path.GetFileName`.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// `Path.IsPathFullyQualified`: `/x`, `\\server\x`, `C:\x`, `C:/x` (not `C:x` nor `\x`).
fn fully_qualified(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/') && !p.starts_with("/\\")
        || p.starts_with("\\\\")
        || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
}

/// `Path.GetFullPath(Path.Combine(cwd, p))` for a relative path (`.` and `..` folded), the path itself when absolute.
pub fn full_path(p: &str, cwd: &str) -> String {
    if fully_qualified(p) {
        return p.to_string();
    }
    let sep = if cwd.contains('\\') && !cwd.contains('/') { '\\' } else { '/' };
    let mut parts: Vec<&str> = Vec::new();
    for seg in cwd.split(['/', '\\']).chain(p.split(['/', '\\'])) {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.len() > usize::from(cwd.as_bytes().get(1) == Some(&b':')) {
                    parts.pop();
                }
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join(&sep.to_string());
    if sep == '/' {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Paths as sent: `input.files` (non-empty strings), absolute or relative to the session's folder.
pub fn files(input: &Value, cwd: &str) -> Vec<String> {
    match json::prop(input, "files") {
        Some(Value::Array(a)) => {
            a.iter().filter_map(Value::as_str).filter(|x| !x.is_empty()).map(|x| full_path(x, cwd)).collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths() {
        assert_eq!(full_path("/abs/a.png", "/w"), "/abs/a.png");
        assert_eq!(full_path("out/a.png", "/w/p"), "/w/p/out/a.png");
        assert_eq!(full_path("../a.png", "/w/p"), "/w/a.png");
        assert_eq!(full_path("./x/../a.png", "/w"), "/w/a.png");
        assert_eq!(full_path("a.png", "C:\\w"), "C:\\w\\a.png");
        assert_eq!(full_path("..\\..\\a", "C:\\w"), "C:\\a");
        assert_eq!(full_path("D:/x", "C:\\w"), "D:/x");
        assert_eq!(file_name("/w/out/a.png"), "a.png");
        assert_eq!(files(&json!({"files":["a", "", 3, "/b"]}), "/w"), ["/w/a", "/b"]);
    }
}
