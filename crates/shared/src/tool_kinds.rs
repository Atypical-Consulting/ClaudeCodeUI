//! Port of `Core/ToolKinds.cs` — spec 02 §6. Owner: shared-pure-a. Used by the UI everywhere (rows, cards, queue).

use crate::{ask_user, elicit, i18n, json, todo_list};
use serde_json::Value;

/// SendUserFile tool (input: `files`, `caption`).
pub const SEND_FILE: &str = "SendUserFile";

/// CSS var for the tool name (`style="color:…"`): Read/NotebookRead var(--read), Grep/Glob var(--grep),
/// Edit/MultiEdit/NotebookEdit var(--edit), Bash/PowerShell/BashOutput/KillShell var(--bash), Write var(--write),
/// WebFetch/WebSearch var(--web), else var(--fg-3).
pub fn color(name: &str) -> &'static str {
    match name {
        "Read" | "NotebookRead" => "var(--read)",
        "Grep" | "Glob" => "var(--grep)",
        "Edit" | "MultiEdit" | "NotebookEdit" => "var(--edit)",
        "Bash" | "PowerShell" | "BashOutput" | "KillShell" => "var(--bash)",
        "Write" => "var(--write)",
        "WebFetch" | "WebSearch" => "var(--web)",
        _ => "var(--fg-3)",
    }
}

/// Bash | PowerShell.
pub fn is_shell(name: &str) -> bool {
    matches!(name, "Bash" | "PowerShell")
}

/// Agent | Task.
pub fn is_agent(name: &str) -> bool {
    matches!(name, "Agent" | "Task")
}

/// Edit | MultiEdit | Write.
pub fn is_edit(name: &str) -> bool {
    matches!(name, "Edit" | "MultiEdit" | "Write")
}

/// .NET `ReplaceLineEndings(" ")`: CRLF, CR, LF, NEL, FF, LS, PS → one space each.
fn one_line(s: &str) -> String {
    s.replace("\r\n", " ").replace(['\r', '\n', '\u{85}', '\u{0C}', '\u{2028}', '\u{2029}'], " ")
}

/// One-line target of a tool call; first match wins (elicitation, file_path, command, pattern, AskUser headers,
/// TodoList target, description, url, query, first string property, "").
pub fn target(name: &str, input: &Value, cwd: &str) -> String {
    if name == elicit::TOOL {
        return one_line(&format!("{} · {}", json::string(input, "mcp_server_name"), json::string(input, "message")));
    }
    if let Some(f) = json::str(input, "file_path") {
        return relative(f, cwd);
    }
    if let Some(c) = json::str(input, "command") {
        return one_line(c); // one line: WebKit drops the ellipsis on a newline
    }
    if let Some(p) = json::str(input, "pattern") {
        return match json::str(input, "glob").or_else(|| json::str(input, "path")).map(|w| relative(w, cwd)) {
            Some(w) if w != "." => format!("{p} · {w}"),
            _ => p.to_string(),
        };
    }
    let qs = ask_user::parse(input);
    if !qs.is_empty() {
        return qs
            .iter()
            .map(|q| if q.header.is_empty() { q.text.as_str() } else { q.header.as_str() })
            .collect::<Vec<_>>()
            .join(" · ");
    }
    todo_list::target(name, input)
        .or_else(|| ["description", "url", "query"].iter().find_map(|k| json::str(input, k)).map(str::to_string))
        // MCP and other unknown tools: the first string argument is the best one-line target there is.
        .or_else(|| input.as_object()?.values().find_map(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// AskUserQuestion → `Tool.Label.Question`; elicitation → `Tool.Label.Elicit`; `mcp__s__t` → part after the last
/// `__` when its index > 3; else the name.
pub fn label(name: &str) -> String {
    if name == ask_user::TOOL {
        return i18n::t("Tool.Label.Question");
    }
    if name == elicit::TOOL {
        return i18n::t("Tool.Label.Elicit");
    }
    match name.rfind("__") {
        Some(i) if name.starts_with("mcp__") && i > 3 => name[i + 2..].to_string(),
        _ => name.to_string(),
    }
}

/// Path relative to cwd when it starts with cwd (case-insensitive plain prefix, `.` when equal), then `\` → `/`.
/// `/w/abc` vs cwd `/w/a` yields `../abc` (kept for parity).
pub fn relative(path: &str, cwd: &str) -> String {
    let prefixed = !cwd.is_empty()
        && path.len() >= cwd.len()
        && path.is_char_boundary(cwd.len())
        && path[..cwd.len()].to_lowercase() == cwd.to_lowercase();
    let out = if prefixed { relative_path(cwd, path) } else { path.to_string() };
    out.replace('\\', "/")
}

/// `Path.GetRelativePath(from, to)` over components (both separators; case-insensitive on Windows like .NET).
fn relative_path(from: &str, to: &str) -> String {
    let parts = |s: &str| s.split(['/', '\\']).filter(|p| !p.is_empty()).map(str::to_string).collect::<Vec<_>>();
    let (a, b) = (parts(from), parts(to));
    let same = |x: &String, y: &String| if cfg!(windows) { x.to_lowercase() == y.to_lowercase() } else { x == y };
    let common = a.iter().zip(&b).take_while(|(x, y)| same(x, y)).count();
    let rel: Vec<&str> =
        std::iter::repeat_n("..", a.len() - common).chain(b[common..].iter().map(String::as_str)).collect();
    if rel.is_empty() {
        ".".into()
    } else {
        rel.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set_thread_lang, Lang};
    use serde_json::json;

    #[test]
    fn kinds_and_colors() {
        assert_eq!(color("NotebookEdit"), "var(--edit)");
        assert_eq!(color("KillShell"), "var(--bash)");
        assert_eq!(color("mcp__x__y"), "var(--fg-3)");
        assert!(is_shell("PowerShell") && !is_shell("BashOutput"));
        assert!(is_agent("Task") && is_edit("Write") && !is_edit("NotebookEdit"));
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative("/w/a/b.rs", "/w/a"), "b.rs");
        assert_eq!(relative("/w/a", "/w/a"), ".");
        assert_eq!(relative("/w/abc", "/w/a"), "../abc");
        assert_eq!(relative("/W/A/x", "/w/a"), if cfg!(windows) { "x" } else { "../../W/A/x" });
        assert_eq!(relative("/other/x", "/w/a"), "/other/x");
        assert_eq!(relative("C:\\w\\x", ""), "C:/w/x");
    }

    #[test]
    fn targets() {
        assert_eq!(target("Read", &json!({"file_path":"/w/src/a.rs"}), "/w"), "src/a.rs");
        assert_eq!(target("Bash", &json!({"command":"a\r\nb\nc"}), "/w"), "a b c");
        assert_eq!(target("Grep", &json!({"pattern":"x","path":"/w/src"}), "/w"), "x · src");
        assert_eq!(target("Grep", &json!({"pattern":"x","path":"/w"}), "/w"), "x");
        assert_eq!(target(elicit::TOOL, &json!({"mcp_server_name":"probe","message":"a\nb"}), ""), "probe · a b");
        assert_eq!(
            target("AskUserQuestion", &json!({"questions":[{"question":"Q?","header":""},{"question":"R?","header":"H"}]}), ""),
            "Q? · H"
        );
        assert_eq!(target("WebFetch", &json!({"url":"https://x"}), ""), "https://x");
        assert_eq!(target("mcp__s__t", &json!({"n":1,"first":"f","second":"s"}), ""), "f");
        assert_eq!(target("mcp__s__t", &json!({}), ""), "");
        // TodoList.Check: look-alike inputs on another tool keep their usual target.
        assert_eq!(target("mcp__x__t", &json!({"subject":"s","description":"d"}), ""), "d");
    }

    #[test]
    fn labels() {
        set_thread_lang(Some(Lang::Fr));
        assert_eq!(label("AskUserQuestion"), "Question");
        assert_eq!(label(elicit::TOOL), "Saisie MCP");
        set_thread_lang(None);
        assert_eq!(label("mcp__roseline__search_symbols"), "search_symbols");
        assert_eq!(label("mcp__x"), "mcp__x");
        assert_eq!(label("Bash"), "Bash");
    }
}
