//! Port of `Core/Mcp.cs` — spec 05 §2. Owner: core-extensions.

use crate::session::SessionMeta;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    todo!("port: Mcp.Tools")
}

/// The CLI's server prefix in tool names: chars outside `[A-Za-z0-9_-]` → `_`; for `claude.ai …` servers runs of `_`
/// collapse and leading/trailing `_` are trimmed. `plugin:github:github` → `plugin_github_github`.
pub fn prefix(server: &str) -> String {
    todo!("port: Mcp.Prefix")
}

/// The first server whose `mcp__{prefix}__` prefixes the tool name; None for non-MCP tools.
pub fn server_of(tool: &str, servers: &[String]) -> Option<String> {
    todo!("port: Mcp.ServerOf")
}

/// `(?i)\b401\b|insufficient_scope|Unauthorized|rejected the (session )?credential|re-?authenticat`; None → false.
pub fn auth_error(text: Option<&str>) -> bool {
    todo!("port: Mcp.AuthError")
}

/// mcp_status `mcpServers[].name` when read, else init `mcp[].name`.
pub fn names(meta: &SessionMeta) -> Vec<String> {
    todo!("port: Mcp.Names")
}
