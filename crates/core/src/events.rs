//! Port of `Core/Events.cs` — the single parsing point for CLI stdout lines AND transcript (.jsonl) lines.
//! Spec 02 §2-§5. Owner: core-events.
//!
//! Malformed-line rule: a malformed line is skipped, never thrown; a malformed block is skipped and the next block
//! of the same message still parses. Never `unwrap()` a field: go through `ccui_shared::json`.
//! Tests: port every `Check()` of Events.cs (real claude 2.1.296 captures, culture fr) as #[test].

use ccui_shared::hooks::HookListing;
use ccui_shared::model::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// CLI 2.1.296 constant that tells a compaction summary from a skill body.
pub const SUMMARY_LEAD: &str = "This session is being continued from a previous conversation that ran out of context.";

/// `ResultEvt`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResultEvt {
    pub subtype: String,
    pub is_error: bool,
    /// Cumulative per process: assign, never add.
    pub total_cost_usd: Decimal,
    pub duration_ms: i32,
    pub num_turns: i32,
    /// Last usage iteration: input + cache_read + cache_creation.
    pub context_tokens: i64,
    pub context_window: Option<i64>,
    pub fast_mode_state: Option<String>,
    pub fast_mode_reason: Option<String>,
    pub terminal_reason: Option<String>,
    /// permission_denials[].tool_use_id (None when absent).
    pub denied: Option<Vec<String>>,
    pub cache_read: i64,
    pub cache_write: i64,
}

/// `TaskDoneEvt` (system/task_notification and `<task-notification>` user text).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskDone {
    pub task_id: String,
    pub tool_use_id: String,
    pub status: String,
    pub result: Option<String>,
    pub tokens: i64,
    pub tool_uses: i32,
    pub duration_ms: i32,
}

/// The parsed event union (spec 02 §2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClaudeEvent {
    Init(InitInfo),
    /// system/commands_changed: the `commands` array.
    Commands(Value),
    PluginStatus { plugin: String, text: Option<String> },
    ElicitDone { server: String, elicitation_id: String },
    Notice(NoticeEvt),
    Status { status: Option<String>, permission_mode: Option<String> },
    Thinking { estimated_tokens: i32 },
    Thought { text: String, parent_tool_use_id: Option<String> },
    Compact { trigger: String, pre_tokens: i64, post_tokens: i64, duration_ms: i32 },
    Summary { text: String },
    Fallback { text: String, retry: Option<String>, retracted: Vec<String>, model: Option<String> },
    Artifacts(Vec<Artifact>),
    TextDelta { text: String, parent_tool_use_id: Option<String> },
    AssistantText {
        message_id: String,
        text: String,
        parent_tool_use_id: Option<String>,
        synthetic: bool,
        uuid: Option<String>,
        at: Option<Time>,
        model: Option<String>,
    },
    ToolUse { id: String, name: String, input: Value, parent_tool_use_id: Option<String>, model: Option<String> },
    ToolResult { tool_use_id: String, text: String, is_error: bool, structured: Option<Value>, not_run: bool },
    UserText { text: String, at: Option<Time>, images: Option<Vec<UserImage>> },
    Permission {
        request_id: String,
        tool: String,
        input: Value,
        description: Option<String>,
        tool_use_id: Option<String>,
        suggestions: Option<Value>,
        info: Option<PermissionInfo>,
    },
    CancelRequest { request_id: String },
    Retry(RetryEvt),
    Result(ResultEvt),
    RateLimit(RateLimits),
    TaskStarted {
        task_id: String,
        tool_use_id: String,
        description: String,
        subagent_type: String,
        task_type: Option<String>,
        backgrounded: bool,
        ambient: bool,
    },
    TaskUpdated { task_id: String, status: Option<String>, ended_at: Option<Time>, backgrounded: bool },
    TaskProgress { task_id: String, total_tokens: i64, tool_uses: i32, duration_ms: i32, workflow: Option<Value> },
    TaskDone(TaskDone),
    Title { title: String },
    Scheduled { prompt: String, at: Option<Time> },
    Pr { url: String, number: Option<String> },
    Reset,
    /// state: queued | started | completed | cancelled | discarded | refused.
    Queue { uuid: String, state: String },
    Suggestion { text: String },
    Hook(HookEvt),
    HookStarted { id: String, name: String },
}

/// Every event of one line (one per content block for assistant/user lines). Unknown types → [].
pub fn parse_all(line: &Value) -> Vec<ClaudeEvent> {
    todo!("port: Events.ParseAll")
}

/// First of `parse_all`.
pub fn parse(line: &Value) -> Option<ClaudeEvent> {
    todo!("port: Events.Parse")
}

/// `type: "system"` → at most one event (spec 02 §3.1).
pub fn parse_system(line: &Value) -> Option<ClaudeEvent> {
    todo!("port: Events.ParseSystem")
}

/// `Notification(text)`: a `<task-notification>` block → TaskDone (result span cut out before reading the tags).
pub fn notification(text: &str) -> Option<TaskDone> {
    todo!("port: Events.Notification")
}

/// tool_use_result minus `originalFile` and `file.content`.
pub fn slim(tool_use_result: &Value) -> Value {
    todo!("port: Events.Slim")
}

/// toolDenialKind (transcript) ?? tool_result_meta[id].non_execution_kind (stream) ∈ {automode-unavailable,
/// automode-parsing-error}.
pub fn not_run(line: &Value, tool_use_id: &str) -> bool {
    todo!("port: Events.NotRun")
}

/// `Info(request)` → PermissionInfo, None when every field is empty/false.
pub fn info(request: &Value) -> Option<PermissionInfo> {
    todo!("port: Events.Info")
}

/// `get_hooks_listing` response → HookListing (spec 02 §5.1).
pub fn hooks(listing: &Value) -> HookListing {
    todo!("port: Events.Hooks")
}

/// `mcp_authenticate` response → McpAuthStart (spec 02 §5.3).
pub fn mcp_auth(response: &Value) -> McpAuthStart {
    todo!("port: Events.McpAuth")
}

/// `get_status` response → sections (spec 02 §5.4).
pub fn status_sections(response: &Value) -> Vec<StatusSection> {
    todo!("port: StatusSection.Parse")
}
