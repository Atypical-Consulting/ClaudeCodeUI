//! Port of `ClaudeSession.cs` (→ `ClaudeProcess`): one long-lived `claude` speaking stream-json over stdin/stdout.
//! Spec 01 §3. Owner: core-events.
//!
//! Spawn: `claude` on PATH (no shell, never cmd.exe), cwd = session cwd, argv = `PREFIX_ARGS` + LiveSession args,
//! env `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=1` (+ `CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1` when resuming a
//! crash-cut turn), whole process tree tracked by `ProcessJob`. Readers: stdout per JSON line (non-JSON skipped;
//! control_response routed to the pending request, then EVERY line goes to `on_event`), stderr ring of 50 lines.
//! After EOF: wait exit → kill tree → fail pending (`Session.ProcessStopped`) → `on_exit(code, text)` unless disposing.

use ccui_shared::model::UserImage;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Duration;

/// Fixed argv prefix before `LiveSession::args`.
pub const PREFIX_ARGS: [&str; 6] =
    ["--output-format", "stream-json", "--verbose", "--input-format", "stream-json", "--permission-prompt-tool=stdio"];
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
pub const STDERR_LINES: usize = 50;

#[derive(Clone, Debug, PartialEq)]
pub enum ClaudeError {
    /// No answer within the timeout.
    Timeout,
    /// The CLI answered `subtype: "error"` (`r.error ?? "unknown error"`).
    Request(String),
    /// The process stopped (i18n `Session.ProcessStopped`).
    Stopped,
    /// Spawn / pipe failure.
    Io(String),
}

impl std::fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!("port: ClaudeRequestException / TimeoutException messages")
    }
}

impl From<ClaudeError> for String {
    fn from(e: ClaudeError) -> String {
        e.to_string()
    }
}

/// Called on the stdout reader task for every parsed line (control_responses included).
pub type EventHandler = Arc<dyn Fn(Value) + Send + Sync>;
/// `(exit code, "{Session.Exited(code)}\n{last 50 stderr lines}".trim())`, not called while disposing.
pub type ExitHandler = Arc<dyn Fn(Option<i32>, String) + Send + Sync>;
/// Boot timeline line `"+{ms} ms {what}"`.
pub type Trace = Arc<dyn Fn(&str) + Send + Sync>;

pub struct ClaudeProcess {
    pid: u32,
    /// Identity for `on_exit` (a replaced process's late exit is ignored).
    generation: u64,
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<Result<Value, ClaudeError>>>>,
    stderr_tail: Mutex<VecDeque<String>>,
    disposing: AtomicBool,
    job: Mutex<Option<crate::process_job::ProcessJob>>,
}

impl ClaudeProcess {
    /// Spawn and start the readers. `args` = everything after the fixed prefix.
    pub fn spawn(
        cwd: &str,
        args: Vec<String>,
        resume_turn: bool,
        on_event: EventHandler,
        on_exit: ExitHandler,
        trace: Option<Trace>,
    ) -> Result<Arc<ClaudeProcess>, ClaudeError> {
        todo!("port: ClaudeSession constructor + Start")
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// `{"type":"control_request","request_id":<32 hex>,"request":{"subtype":…, ...fields}}` and await the matching
    /// control_response (pending entry removed in all cases).
    pub async fn request(&self, subtype: &str, fields: Value, timeout: Duration) -> Result<Value, ClaudeError> {
        todo!("port: ClaudeSession.Request")
    }

    /// `{"type":"user","message":{"role":"user","content":Images.content(text, images)},"uuid":uuid}`.
    pub async fn send_user(&self, text: &str, images: &[UserImage], uuid: &str) -> Result<(), ClaudeError> {
        todo!("port: ClaudeSession.SendUser")
    }

    /// can_use_tool answer: `reply(allow, input, updated_permissions, message)` as a control_response success.
    pub async fn respond(
        &self,
        request_id: &str,
        allow: bool,
        input: &Value,
        updated_permissions: Option<Value>,
        message: Option<&str>,
    ) -> Result<(), ClaudeError> {
        todo!("port: ClaudeSession.Respond")
    }

    /// Any control_response success payload (elicitation replies).
    pub async fn answer(&self, request_id: &str, payload: Value) -> Result<(), ClaudeError> {
        todo!("port: ClaudeSession.Answer")
    }

    /// `request("interrupt")`.
    pub async fn interrupt(&self) -> Result<(), ClaudeError> {
        todo!("port: ClaudeSession.Interrupt")
    }

    /// disposing = true → close stdin → wait ≤ 2000 ms → kill tree → await readers.
    pub async fn dispose(&self) {
        todo!("port: ClaudeSession.DisposeAsync")
    }

    /// can_use_tool payload: allow `{"behavior":"allow","updatedInput":…[,"updatedPermissions":…]}`; deny
    /// `{"behavior":"deny","message":trimmed or "The user denied this tool use."}`.
    pub fn reply(allow: bool, input: &Value, updated_permissions: Option<Value>, message: Option<&str>) -> Value {
        todo!("port: ClaudeSession.Reply")
    }

    /// `claude` (`claude.exe` on Windows) found in a PATH dir (each trimmed of '"').
    pub fn on_path() -> bool {
        todo!("port: ClaudeSession.OnPath")
    }

    /// Windows && !on_path && `claude.cmd` on PATH (the not-found screen then points to the native installer).
    pub fn npm_shim_only() -> bool {
        todo!("port: ClaudeSession.NpmShimOnly")
    }
}
