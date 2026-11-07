//! Port of the disk side of `Core/WorkflowRuns.cs` — read a Workflow run from its receipt, final file, live
//! snapshot or journal + agent logs; path safety; incremental tails — spec 05 §4. Owner: core-extensions.
//! Pure parts (meta, phases, snapshot agents, json_result) are `ccui_shared::workflow`.

use crate::live::state::SessionState;
use ccui_shared::model::{Time, ToolItem};
use ccui_shared::workflow::{WfAgent, WorkflowRun};
use ccui_shared::api::WorkflowAgentLog;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `Read(tool, now)` (spec 05 §4 algorithm, exact). Cached per (tool id, ended_at/progress revision) for 500 ms.
pub fn read(tool: &ToolItem, now: Time) -> WorkflowRun {
    todo!("port: WorkflowRuns.Read / Of")
}

/// Real path of `path` when it is inside `Real(projects root)` (every symlink resolved), else None. Cached.
pub fn inside(path: Option<&str>) -> Option<PathBuf> {
    todo!("port: WorkflowRuns.Inside")
}

/// `<session>/workflows/<runId>.json` for a run dir shaped `<session>/subagents/workflows/<runId>`; cached by
/// (length, mtime); parse/IO errors → None (retried on change).
pub fn final_file(dir: &Path) -> Option<Value> {
    todo!("port: WorkflowRuns.Final")
}

/// Max mtime of the files directly in dir.
pub fn last_write(dir: &Path) -> Option<Time> {
    todo!("port: WorkflowRuns.LastWrite")
}

/// `dir/agent-<id>.jsonl` when the id is valid.
pub fn agent_file(dir: &Path, agent_id: &str) -> Option<PathBuf> {
    todo!("port: WorkflowRuns.AgentFile")
}

/// Agents from `journal.jsonl` (+ agent stats), incremental, one state per dir. None when the journal is missing.
pub fn journal(dir: &Path, ended: bool, now: Time) -> Option<Vec<WfAgent>> {
    todo!("port: WorkflowRuns.Journal")
}

/// Incremental line reader (offset; truncation → reset; a trailing line without '\n' waits).
pub struct Tail {
    path: PathBuf,
    offset: u64,
    /// The file shrank since the last read: consumers clear their state.
    pub reset: bool,
}

impl Tail {
    pub fn new(path: PathBuf) -> Tail {
        Tail { path, offset: 0, reset: false }
    }

    /// New complete lines (trimmed `\r`, empties dropped); IO errors → [].
    pub fn next(&mut self) -> Vec<String> {
        todo!("port: WorkflowRuns.Tail.Next")
    }
}

/// One agent's stats folded line by line: model, LAST message's tokens, distinct tool_use ids, first/last timestamps.
#[derive(Default)]
pub struct AgentStats {
    pub model: Option<String>,
    pub tokens: i64,
    pub tool_calls: i32,
    pub first: Option<Time>,
    pub last: Option<Time>,
}

impl AgentStats {
    pub fn fold(&mut self, line: &Value) {
        todo!("port: WorkflowRuns.AgentStats.Read (per line)")
    }
}

/// One agent's log for the inspector: tails `agent-<id>.jsonl` through a scratch reducer.
pub struct AgentLog {
    tail: Tail,
    state: SessionState,
}

impl AgentLog {
    pub fn new(path: PathBuf) -> AgentLog {
        todo!("port: WorkflowRuns.AgentLog constructor")
    }

    /// Fold new lines (`toolUseResult` → `tool_use_result`, `replay::apply`); returns (last text, top-level tools).
    pub fn read(&mut self) -> WorkflowAgentLog {
        todo!("port: WorkflowRuns.AgentLog.Read")
    }
}

/// Agent logs kept per (session id, tool id, agent id) between `workflow_agent_log` calls.
#[derive(Default)]
pub struct AgentLogs {
    pub logs: Mutex<HashMap<(String, String, String), AgentLog>>,
}

impl AgentLogs {
    /// The log of that agent of that run (created on first call; recreated when its file key changes).
    pub fn read(&self, session_id: &str, tool: &ToolItem, agent_id: &str) -> WorkflowAgentLog {
        todo!("port: AgentPanel/WorkflowPanel AgentLog ownership")
    }

    /// The inspector stopped showing that agent (`workflow_agent_log_release`): drop its log. Spec 05/07: a log is
    /// kept only while selected.
    pub fn release(&self, session_id: &str, tool_id: &str, agent_id: &str) {
        todo!("port: (AgentLog disposed with its panel)")
    }

    /// Drop every log of a session (called by the `session_stop` command after `SessionManager::stop`).
    pub fn forget_session(&self, session_id: &str) {
        todo!("port: (AgentLog disposed with its session)")
    }
}
