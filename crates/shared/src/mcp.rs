//! Port of `Core/Mcp.cs` — spec 05 §2. Owner: core-extensions.

use crate::json;
use crate::session::SessionHeavy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub read_only: bool,
    pub destructive: bool,
    pub open_world: bool,
}

/// Tools of one mcp_status server object (annotations true only if literally true).
pub fn tools(server: &Value) -> Vec<ToolInfo> {
    let Some(Value::Array(a)) = json::prop(server, "tools") else { return Vec::new() };
    a.iter()
        .map(|t| {
            let an = json::get(t, "annotations");
            ToolInfo {
                name: json::string(t, "name"),
                description: json::str(t, "description").map(str::to_string),
                read_only: json::bool(an, "readOnly"),
                destructive: json::bool(an, "destructive"),
                open_world: json::bool(an, "openWorld"),
            }
        })
        .collect()
}

/// The CLI's server prefix in tool names: chars outside `[A-Za-z0-9_-]` → `_`; for `claude.ai …` servers runs of `_`
/// collapse and leading/trailing `_` are trimmed. `plugin:github:github` → `plugin_github_github`.
pub fn prefix(server: &str) -> String {
    // One '_' per UTF-16 unit, like the .NET/JS regexes (an astral char is two).
    let mut r = String::with_capacity(server.len());
    for c in server.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            r.push(c);
        } else {
            (0..c.len_utf16()).for_each(|_| r.push('_'));
        }
    }
    if !server.starts_with("claude.ai ") {
        return r;
    }
    let mut out = String::with_capacity(r.len());
    for c in r.chars() {
        if !(c == '_' && out.ends_with('_')) {
            out.push(c);
        }
    }
    out.trim_matches('_').to_string()
}

/// The first server whose `mcp__{prefix}__` prefixes the tool name; None for non-MCP tools.
pub fn server_of(tool: &str, servers: &[String]) -> Option<String> {
    if !tool.starts_with("mcp__") {
        return None;
    }
    servers.iter().find(|s| tool.starts_with(&format!("mcp__{}__", prefix(s)))).cloned()
}

/// `(?i)\b401\b|insufficient_scope|Unauthorized|rejected the (session )?credential|re-?authenticat`; None → false.
pub fn auth_error(text: Option<&str>) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    text.is_some_and(|t| {
        RE.get_or_init(|| {
            Regex::new(r"(?i)\b401\b|insufficient_scope|Unauthorized|rejected the (session )?credential|re-?authenticat").expect("valid regex")
        })
        .is_match(t)
    })
}

/// mcp_status `mcpServers[].name` when read, else init `mcp[].name`.
/// Contract change: takes the session's `SessionHeavy` (where mcp_status and init live), not `SessionMeta`.
pub fn names(heavy: &SessionHeavy) -> Vec<String> {
    match heavy.mcp_status.as_ref().and_then(|st| json::prop(st, "mcpServers")) {
        Some(Value::Array(a)) => a.iter().map(|m| json::string(m, "name")).collect(),
        _ => heavy.init.as_ref().map(|i| i.mcp.iter().map(|m| m.name.clone()).collect()).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_list() {
        // claude 2.1.296 mcp_status for the probe's fake server.
        let srv: Value = serde_json::from_str(r#"{"name":"probe","status":"connected","tools":[{"name":"ask","annotations":{"readOnly":true}},{"name":"link","description":"Visit","annotations":{"destructive":true,"openWorld":true}},{"name":"bare"}]}"#).unwrap();
        let t = tools(&srv);
        assert_eq!(t.len(), 3);
        assert!(t[0].name == "ask" && t[0].read_only && !t[0].destructive && t[0].description.is_none());
        assert!(t[1].name == "link" && t[1].description.as_deref() == Some("Visit") && t[1].destructive && t[1].open_world);
        assert!(t[2].name == "bare" && !t[2].read_only);
    }

    #[test]
    fn server_of_a_tool() {
        let names: Vec<String> = ["plugin:github:github", "claude.ai Gmail", "probe"].map(String::from).to_vec();
        assert_eq!(server_of("mcp__plugin_github_github__create_issue", &names).as_deref(), Some("plugin:github:github"));
        assert_eq!(server_of("mcp__claude_ai_Gmail__send", &names).as_deref(), Some("claude.ai Gmail"));
        assert_eq!(server_of("mcp__probe__ask", &names).as_deref(), Some("probe"));
        assert_eq!(server_of("mcp__other__x", &names), None);
        assert_eq!(server_of("Bash", &names), None);
        assert_eq!(prefix("claude.ai  My--Srv!"), "claude_ai_My--Srv");
    }

    #[test]
    fn auth_error_texts() {
        assert!(auth_error(Some("Tool call returned 403 insufficient_scope (scope: repo)")));
        assert!(auth_error(Some("Tool call returned 401 Unauthorized - token may have expired")));
        assert!(auth_error(Some("MCP server \"x\" rejected the session credential (HTTP 403)")));
        assert!(!auth_error(Some("ENOENT: no such file")));
        assert!(!auth_error(None));
    }

    #[test]
    fn server_names() {
        let mut h = SessionHeavy::default();
        assert!(names(&h).is_empty());
        h.mcp_status = Some(serde_json::json!({"mcpServers":[{"name":"a"},{"status":"x"}]}));
        assert_eq!(names(&h), ["a", ""]);
    }
}
