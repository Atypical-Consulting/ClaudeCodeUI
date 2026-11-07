//! Port of the records and pure parts of `Core/WorkflowRuns.cs` (ultracode `Workflow` tool runs) — spec 05 §4.
//! Owner: core-extensions. Disk reads (final file, journal, agent logs, path safety) are `ccui_core::workflow_runs`.

use crate::model::Time;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TOOL: &str = "Workflow";

/// Unknown: no end recorded (transcript stops mid-run).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WfState {
    Waiting,
    Running,
    Done,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WfAgent {
    pub label: String,
    pub phase: String,
    pub state: WfState,
    pub model: Option<String>,
    pub tokens: i64,
    /// Rebuilt from agent-*.jsonl.
    pub approx: bool,
    pub tool_calls: i32,
    pub duration_ms: Option<i64>,
    pub agent_id: Option<String>,
    pub result: Option<String>,
    /// The CLI's raw state word when Failed (shown verbatim).
    pub raw_state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WfPhase {
    pub title: String,
    pub detail: Option<String>,
    pub total: i32,
    pub done: i32,
    pub running: i32,
    pub failed: i32,
}

impl WfPhase {
    /// Failed>0 → Failed; Running>0 || (0<Done<Total) → Running; Total>0 && Done==Total → Done; else Waiting.
    pub fn state(&self) -> WfState {
        todo!("port: WfPhase.State")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub name: String,
    pub description: Option<String>,
    pub run_dir: Option<String>,
    pub phases: Vec<WfPhase>,
    pub agents: Vec<WfAgent>,
    pub tokens: i64,
    pub tool_calls: i32,
    pub elapsed_ms: i64,
    pub state: WfState,
    pub raw_status: Option<String>,
    pub model: Option<String>,
    /// Re-read once a second while live (Ticker).
    pub live: bool,
}

impl WorkflowRun {
    /// Count of agents Done.
    pub fn done(&self) -> usize {
        todo!("port: WorkflowRun.Done")
    }
}

/// `Meta(script, key)`: first `\b{key}\s*:\s*{Q}` after "meta" (Q = '…' | "…" | `…`, backslash escapes unescaped).
pub fn meta(script: &str, key: &str) -> Option<String> {
    todo!("port: WorkflowRuns.Meta")
}

/// `MetaPhases(script)`: `{ title: Q [, detail: Q] }` entries within 8 KB after "phases" (after "meta").
pub fn meta_phases(script: &str) -> Vec<(String, Option<String>)> {
    todo!("port: WorkflowRuns.MetaPhases")
}

/// Titles of `workflow_phase` items of a snapshot.
pub fn phase_titles(snapshot: &Value) -> Vec<String> {
    todo!("port: WorkflowRuns.PhaseTitles")
}

/// `workflow_agent` items of a snapshot → agents (done → Done; start → Waiting/Running; else Failed + raw word).
pub fn agents(snapshot: &Value, now: Time) -> Vec<WfAgent> {
    todo!("port: WorkflowRuns.Agents")
}

/// The text after the line starting `Transcript dir:` in a receipt text.
pub fn receipt_line(text: Option<&str>, prefix: &str) -> Option<String> {
    todo!("port: WorkflowRuns.Line")
}

/// Agent id usable as a file name: `^[A-Za-z0-9_-]{1,80}$`.
pub fn valid_agent_id(id: &str) -> bool {
    todo!("port: WorkflowRuns.AgentFile (id check)")
}

/// Trimmed `{…}` / `[…]` that parses → Some(JSON) (shown pretty-printed), else None.
pub fn json_result(s: &str) -> Option<Value> {
    todo!("port: WorkflowRuns.JsonResult")
}
