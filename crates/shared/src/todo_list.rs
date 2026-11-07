//! Port of `Core/TodoList.cs` — spec 02 §10. Owner: shared-pure-a.
//! The agent's task list, rebuilt from its own top-level tool calls (live session and transcript replay agree).
//! UI: a `Memo` over the items signal.

use crate::model::Item;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ALLOWED_TOOLS: [&str; 5] = ["TodoWrite", "TaskCreate", "TaskGet", "TaskUpdate", "TaskList"];
/// ONE argv token (the flag is variadic and would swallow a positional argument).
pub const ALLOWED_TOOLS_ARG: &str = "--allowedTools=TodoWrite,TaskCreate,TaskGet,TaskUpdate,TaskList";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TodoEntry {
    /// TodoWrite: index as string; TaskCreate: task id, or "#<tool_use_id>" until the result names it.
    pub key: String,
    pub content: String,
    pub status: TodoStatus,
    pub active_form: Option<String>,
}

impl TodoEntry {
    /// active_form ?? content when InProgress, else content (the terminal's spinner text).
    pub fn label(&self) -> &str {
        todo!("port: TodoEntry.Label")
    }
}

/// Fold the items (ResetItem clears; top-level ToolItems not Error/Denied/NotRun apply).
pub fn from<I: std::borrow::Borrow<Item>>(items: &[I]) -> Vec<TodoEntry> {
    todo!("port: TodoList.From")
}

/// "pending" | "in_progress" | "completed", else None.
pub fn status(s: &str) -> Option<TodoStatus> {
    todo!("port: TodoList.Status")
}

/// i18n `Todo.Progress` (completed, total): en "{0}/{1} done", fr "{0}/{1} faites".
pub fn progress(list: &[TodoEntry]) -> String {
    todo!("port: TodoList.Progress")
}

/// i18n `Todo.Pending|Todo.InProgress|Todo.Completed`.
pub fn status_label(s: TodoStatus) -> String {
    todo!("port: TodoList.StatusLabel")
}

/// Tool-log one-liner for TodoWrite / TaskUpdate / TaskCreate, None for other tools.
pub fn target(name: &str, input: &Value) -> Option<String> {
    todo!("port: TodoList.Target")
}
