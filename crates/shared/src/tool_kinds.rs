//! Port of `Core/ToolKinds.cs` — spec 02 §6. Owner: shared-pure-a. Used by the UI everywhere (rows, cards, queue).

use serde_json::Value;

/// SendUserFile tool (input: `files`, `caption`).
pub const SEND_FILE: &str = "SendUserFile";

/// CSS var for the tool name (`style="color:…"`): Read/NotebookRead var(--read), Grep/Glob var(--grep),
/// Edit/MultiEdit/NotebookEdit var(--edit), Bash/PowerShell/BashOutput/KillShell var(--bash), Write var(--write),
/// WebFetch/WebSearch var(--web), else var(--fg-3).
pub fn color(name: &str) -> &'static str {
    todo!("port: ToolKinds.Color")
}

/// Bash | PowerShell.
pub fn is_shell(name: &str) -> bool {
    todo!("port: ToolKinds.IsShell")
}

/// Agent | Task.
pub fn is_agent(name: &str) -> bool {
    todo!("port: ToolKinds.IsAgent")
}

/// Edit | MultiEdit | Write.
pub fn is_edit(name: &str) -> bool {
    todo!("port: ToolKinds.IsEdit")
}

/// One-line target of a tool call; first match wins (elicitation, file_path, command, pattern, AskUser headers,
/// TodoList target, description, url, query, first string property, "").
pub fn target(name: &str, input: &Value, cwd: &str) -> String {
    todo!("port: ToolKinds.Target")
}

/// AskUserQuestion → `Tool.Label.Question`; elicitation → `Tool.Label.Elicit`; `mcp__s__t` → part after the last
/// `__` when its index > 3; else the name.
pub fn label(name: &str) -> String {
    todo!("port: ToolKinds.Label")
}

/// Path relative to cwd when it starts with cwd (case-insensitive plain prefix, `.` when equal), then `\` → `/`.
/// `/w/abc` vs cwd `/w/a` yields `../abc` (kept for parity).
pub fn relative(path: &str, cwd: &str) -> String {
    todo!("port: ToolKinds.Relative")
}
