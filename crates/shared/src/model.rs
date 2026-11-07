//! CONTRACT — conversation model shared by backend and UI.
//! Ports the records of `Core/LiveSession.cs` (Item union, SessionStatus, ToolState, Decision, QueuedMessage,
//! RewindPreview, BgTask, PendingPermission), the event payload records of `Core/Events.cs` that the reducer stores
//! (InitEvt, HookEvt, NoticeEvt, RetryEvt, RateLimitEvt, PermissionInfo, Artifact, StatusSection, McpAuthStart) and
//! `Core/Images.cs` UserImage. Spec 01 §2, 02 §2 & §5, 04 §6.
//!
//! Owner: tauri-glue (contract). Field names are the wire names (serde default snake_case, both ends are Rust).
//! Times are UTC (`Time`); the UI formats them in local time. Durations are `i64` milliseconds.

use crate::api_errors::ApiError;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Time = chrono::DateTime<chrono::Utc>;

/// `LiveSession.Status`. Initial value: Exited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionStatus {
    Starting,
    Idle,
    Running,
    Waiting,
    #[default]
    Exited,
    Crashed,
}

/// `ToolState`. NotRun = held back by auto mode (not a decision: retrying is fine).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolState {
    #[default]
    Running,
    Done,
    Error,
    Waiting,
    Denied,
    NotRun,
}

/// Permission decision (`session_answer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    AllowSession,
    Deny,
}

/// `Images.UserImage` — an image or document block of a user message (spec 04 §6).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserImage {
    pub media_type: String,
    /// `data:` URL; None = placeholder (url source, svg, missing source).
    pub url: Option<String>,
    /// Document title (PDF / text only).
    pub name: Option<String>,
}

impl UserImage {
    /// The base64 payload: the url's part after the first ',', "" when url is None.
    pub fn data(&self) -> &str {
        self.url.as_deref().and_then(|u| u.split_once(',')).map_or("", |(_, d)| d)
    }

    /// `application/pdf` or `text/plain`.
    pub fn document(&self) -> bool {
        self.media_type == "application/pdf" || self.media_type == "text/plain"
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Items (immutable: an update REPLACES the item at its index)
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserItem {
    pub text: String,
    pub at: Time,
    pub ultracode: bool,
    pub images: Option<Vec<UserImage>>,
    /// The uuid we sent with the message (this process only); transcript-replayed lines have none.
    pub uuid: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextItem {
    pub markdown: String,
    pub parent_tool_use_id: Option<String>,
    /// Where "Fork from here" cuts.
    pub uuid: Option<String>,
    pub at: Option<Time>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThoughtItem {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompactItem {
    pub trigger: String,
    pub pre_tokens: i64,
    pub post_tokens: i64,
    pub duration_ms: i32,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FallbackItem {
    pub text: String,
    /// The refused user message uuid.
    pub retry: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiErrorItem {
    pub error: ApiError,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HookItem {
    pub hook: HookEvt,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoticeItem {
    pub notice: NoticeEvt,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScheduledItem {
    pub prompt: String,
    pub at: Time,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolItem {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub parent_tool_use_id: Option<String>,
    pub state: ToolState,
    pub result_text: Option<String>,
    /// tool_use_result, slimmed (no Edit originalFile, no Read file.content).
    pub structured: Option<Value>,
    pub started_at: Time,
    pub ended_at: Option<Time>,
    /// Agent & Workflow: the background task id.
    pub task_id: Option<String>,
    /// task-notification status (e.g. "completed").
    pub task_status: Option<String>,
    /// Workflow: last `workflow_progress` snapshot (array).
    pub progress: Option<Value>,
    pub tokens: i64,
    pub sub_tool_uses: i32,
    /// Async agent: outlives the turn, ends on task-notification.
    pub background: bool,
    /// Agent: model of its subagent's first message.
    pub model: Option<String>,
}

/// The conversation item union (spec 01 §2.2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Item {
    User(UserItem),
    Text(TextItem),
    Thought(ThoughtItem),
    Compact(CompactItem),
    Fallback(FallbackItem),
    ApiError(ApiErrorItem),
    /// `/clear` went through; not rendered; starts a new todo list.
    Reset,
    /// Marker: the crash-cut turn re-run by the relaunched CLI.
    Resumed,
    Hook(HookItem),
    Notice(NoticeItem),
    Scheduled(ScheduledItem),
    Tool(ToolItem),
}

impl Item {
    pub fn as_tool(&self) -> Option<&ToolItem> {
        match self {
            Item::Tool(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_user(&self) -> Option<&UserItem> {
        match self {
            Item::User(u) => Some(u),
            _ => None,
        }
    }

    /// parent_tool_use_id of a Tool or Text item (subagent items), else None.
    pub fn parent(&self) -> Option<&str> {
        match self {
            Item::Tool(t) => t.parent_tool_use_id.as_deref(),
            Item::Text(t) => t.parent_tool_use_id.as_deref(),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Event payloads kept by the reducer (Events.cs records)
// ---------------------------------------------------------------------------------------------------------------

/// `McpBrief` of system/init. status: connected | failed | needs-auth | pending.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct McpBrief {
    pub name: String,
    pub status: String,
}

/// A `--mcp-config` entry skipped by validation (init `mcp_server_errors`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct McpSkipped {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

/// A plugin that did not load / loaded demoted (init `plugin_errors`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginError {
    pub plugin: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
    pub path: Option<String>,
}

/// `InitEvt` (system/init). `mcp_errors` / `plugin_errors` are None when the key is absent (≠ empty).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InitInfo {
    pub session_id: String,
    pub cwd: String,
    pub model: String,
    pub permission_mode: String,
    pub tools: Vec<String>,
    pub mcp: Vec<McpBrief>,
    pub slash_commands: Vec<String>,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub plugins: Vec<String>,
    /// Default "off".
    pub fast_mode_state: String,
    pub fast_mode_reason: Option<String>,
    pub mcp_errors: Option<Vec<McpSkipped>>,
    pub plugin_errors: Option<Vec<PluginError>>,
    pub output_style: Option<String>,
}

/// `HookEvt` (system/hook_response). output = stdout+stderr, trailing whitespace trimmed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HookEvt {
    pub name: String,
    pub outcome: String,
    pub exit_code: Option<i32>,
    pub output: String,
    pub id: Option<String>,
}

/// `NoticeEvt` (system/notification, ui_toast, ui_log).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NoticeEvt {
    pub text: String,
    pub color: Option<String>,
}

/// `RetryEvt` (system/api_retry). error_status None = no HTTP response (timeout).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RetryEvt {
    pub attempt: i32,
    pub max_retries: i32,
    pub delay_ms: i32,
    pub error_status: Option<i32>,
    pub error: Option<String>,
}

/// `RateLimitEvt` — the quota windows (utilization 0..1). A reset of None = the event carried no such window
/// (C# `default(DateTimeOffset)`). On disk (`limits.json`) it is PascalCase: see core `session_manager::SavedLimits`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RateLimits {
    pub five_hour: f64,
    pub five_hour_reset: Option<Time>,
    pub seven_day: f64,
    pub seven_day_reset: Option<Time>,
    /// allowed | allowed_warning | rejected (default "allowed").
    pub status: String,
    /// rateLimitType: the limiting window (five_hour, seven_day…).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub extra_on: bool,
    pub extra_in_use: bool,
    pub extra_off: Option<String>,
}

/// `PermissionInfo` — None on the wire when every field is empty/false.
/// `reason` / `ask_rule` may carry ANSI escapes: render through `permission_rules::plain`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PermissionInfo {
    /// The asking subagent's task id.
    pub agent_id: Option<String>,
    pub title: Option<String>,
    pub blocked_path: Option<String>,
    pub reason_type: Option<String>,
    pub reason: Option<String>,
    /// e.g. `Bash(git push:*)`.
    pub ask_rule: Option<String>,
    pub ask_rule_source: Option<String>,
    pub no_always_allow: bool,
}

/// A CLI request waiting for the user (can_use_tool or MCP elicitation, tool `dialog:mcp_elicitation`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingPermission {
    pub request_id: String,
    pub tool: String,
    /// can_use_tool input; for an elicitation the whole request object.
    pub input: Value,
    pub description: Option<String>,
    pub tool_use_id: Option<String>,
    /// permission_suggestions raw (`[{"type":"setMode","mode":…,"destination":…}]`).
    pub suggestions: Option<Value>,
    pub at: Time,
    pub info: Option<PermissionInfo>,
    /// Write only: whether `input.file_path` existed when the request arrived (PermissionCard titles it
    /// `Perm.Overwrite` vs `Perm.Create`, checked ONCE per permission — spec 07 §35). Filled by `live::lifecycle`
    /// (the reducer is pure) right after it folds the can_use_tool event; None for every other tool.
    pub target_exists: Option<bool>,
}

/// A message written to stdin mid-turn, not started yet (the "chip").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub uuid: String,
    pub text: String,
    pub at: Time,
}

/// `rewind_files {dry_run:true}` answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RewindPreview {
    pub can_rewind: bool,
    pub files: Vec<String>,
    pub insertions: i32,
    pub deletions: i32,
    pub error: Option<String>,
}

impl RewindPreview {
    /// `Parse(r)`: canRewind must be JSON true; filesChanged keeps non-empty strings; counts else 0.
    pub fn parse(r: &Value) -> RewindPreview {
        let int = |k: &str| match crate::json::prop(r, k) {
            Some(Value::Number(n)) => n.as_i64().map_or(0, |n| n as i32),
            _ => 0,
        };
        RewindPreview {
            can_rewind: matches!(crate::json::prop(r, "canRewind"), Some(Value::Bool(true))),
            files: match crate::json::prop(r, "filesChanged") {
                Some(Value::Array(a)) => {
                    a.iter().filter_map(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).collect()
                }
                _ => Vec::new(),
            },
            insertions: int("insertions"),
            deletions: int("deletions"),
            error: crate::json::str(r, "error").map(str::to_string),
        }
    }

    /// `can_rewind && (files non-empty || insertions + deletions > 0)` (a hosted transport drops filesChanged).
    pub fn changes(&self) -> bool {
        self.can_rewind && (!self.files.is_empty() || self.insertions + self.deletions > 0)
    }
}

/// A background task of this process (`BgTask`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BgTask {
    pub id: String,
    pub tool_use_id: String,
    pub description: String,
    pub started_at: Time,
    /// task_type when not local_bash.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Default "running".
    pub status: String,
    pub ended_at: Option<Time>,
}

impl BgTask {
    /// status ∈ {running, pending, paused}.
    pub fn running(&self) -> bool {
        matches!(self.status.as_str(), "running" | "pending" | "paused")
    }

    /// `Tail(output, lines)`: strip ANSI, keep each line's text after its last `\r`, drop trailing empties, last N lines.
    /// Test: `"tick 1\r\ntick 2\n\e[31mred\e[0m\n50%\r100%\n\n"`, 3 → `"tick 2\nred\n100%"`.
    pub fn tail(output: &str, lines: usize) -> String {
        let plain = crate::permission_rules::ansi().replace_all(output, "");
        let mut all: Vec<&str> = plain
            .split('\n')
            .map(|l| l.trim_end_matches('\r'))
            .map(|l| l.rfind('\r').map_or(l, |i| &l[i + 1..]))
            .collect();
        while all.last().is_some_and(|l| l.is_empty()) {
            all.pop();
        }
        all[all.len().saturating_sub(lines)..].join("\n")
    }
}

/// `session_task_output` answer (spec 07 §25): TasksPanel marks a task final on `RequestFailed` (the CLI answered
/// with an error: the task is gone with its output file) and retries on `Timeout` (next tick).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum TaskOutput {
    /// The output so far ("" when none).
    Text(String),
    /// The session has no process (nothing asked).
    NoProcess,
    /// `ClaudeRequestException`: the CLI's error reply.
    RequestFailed(String),
    /// `TimeoutException`: no reply in time.
    Timeout,
}

/// A hook that is running (`running_hooks` entry).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunningHook {
    pub id: String,
    pub name: String,
    pub at: Time,
}

/// `Artifact` (system/session_metadata; http(s) only).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub title: String,
}

/// One row of a /status section: `(label?, value)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusRow {
    pub label: Option<String>,
    pub value: String,
}

/// `StatusSection` (get_status): the terminal's /status screen, values already rendered as text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusSection {
    pub title: String,
    pub rows: Vec<StatusRow>,
}

/// `McpAuthStart` (mcp_authenticate). url only if absolute http(s); None = stored token still valid.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct McpAuthStart {
    pub url: Option<String>,
    pub callback_expected: bool,
}

/// `(plugins, skills, agents, errors)` of `reload_plugins` + `reload_skills`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReloadSummary {
    pub plugins: i32,
    pub skills: i32,
    pub agents: i32,
    pub errors: i32,
}

/// Cost as the UI shows it (invariant decimal).
pub type Usd = Decimal;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(status: &str) -> BgTask {
        BgTask {
            id: "b".into(),
            tool_use_id: "t".into(),
            description: "d".into(),
            started_at: Time::default(),
            kind: None,
            status: status.into(),
            ended_at: None,
        }
    }

    #[test]
    fn rewind_preview() {
        let pv = RewindPreview::parse(
            &json!({"canRewind":true,"filesChanged":["/w/notes.txt","/w/keep.txt"],"insertions":1,"deletions":2}),
        );
        assert!(pv.can_rewind && pv.files == ["/w/notes.txt", "/w/keep.txt"]);
        assert_eq!((pv.insertions, pv.deletions, pv.error), (1, 2, None));
        let e = RewindPreview::parse(&json!({"canRewind":false,"error":"No file checkpoint found for this message."}));
        assert!(!e.can_rewind && e.files.is_empty());
        assert_eq!(e.error.as_deref(), Some("No file checkpoint found for this message."));
        assert_eq!(RewindPreview::parse(&json!({"canRewind":true,"filesChanged":[null,"","/w/a"]})).files, ["/w/a"]);
        assert!(RewindPreview::parse(&json!({"canRewind":true,"insertions":3,"deletions":0})).changes());
        assert!(!RewindPreview::parse(&json!({"canRewind":true,"filesChanged":[]})).changes());
        assert!(!RewindPreview::parse(&json!({"canRewind":false,"insertions":3})).changes());
        assert!(!RewindPreview::parse(&json!({"canRewind":"true"})).can_rewind, "canRewind must be JSON true");
    }

    #[test]
    fn bg_task() {
        assert_eq!(BgTask::tail("tick 1\r\ntick 2\n\x1b[31mred\x1b[0m\n50%\r100%\n\n", 3), "tick 2\nred\n100%");
        assert_eq!(BgTask::tail("", 5), "");
        assert!(task("running").running() && task("pending").running() && task("paused").running());
        assert!(!task("completed").running() && !task("killed").running());
    }
}
