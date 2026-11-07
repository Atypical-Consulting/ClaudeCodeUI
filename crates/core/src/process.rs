//! Port of `ClaudeSession.cs` (→ `ClaudeProcess`): one long-lived `claude` speaking stream-json over stdin/stdout.
//! Spec 01 §3. Owner: core-events.
//!
//! Spawn: `claude` on PATH (no shell, never cmd.exe), cwd = session cwd, argv = `PREFIX_ARGS` + LiveSession args,
//! env `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=1` (+ `CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1` when resuming a
//! crash-cut turn), whole process tree tracked by `ProcessJob`. Readers: stdout per JSON line (non-JSON skipped;
//! control_response routed to the pending request, then EVERY line goes to `on_event`), stderr ring of 50 lines.
//! After EOF: wait exit → kill tree → fail pending (`Session.ProcessStopped`) → `on_exit(code, text)` unless disposing.
//!
//! The env also drops `CLAUDECODE` / `CLAUDE_CODE_CHILD_SESSION`: an app started from inside a Claude Code session
//! must not make its claude believe it is a nested child.
//! Must be spawned from inside a tokio runtime (the readers are tokio tasks).

use ccui_shared::json;
use ccui_shared::model::UserImage;
use ccui_shared::t;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Notify};

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
        match self {
            // .NET TimeoutException's default message.
            ClaudeError::Timeout => f.write_str("The operation has timed out."),
            ClaudeError::Request(m) | ClaudeError::Io(m) => f.write_str(m),
            ClaudeError::Stopped => f.write_str(&t!("Session.ProcessStopped")),
        }
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

type Pending = Mutex<HashMap<String, oneshot::Sender<Result<Value, ClaudeError>>>>;

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

pub struct ClaudeProcess {
    pid: u32,
    /// Identity for `on_exit` (a replaced process's late exit is ignored).
    generation: u64,
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    pending: Pending,
    stderr_tail: Mutex<VecDeque<String>>,
    disposing: AtomicBool,
    job: Mutex<Option<crate::process_job::ProcessJob>>,
    /// Set once the process exited, before the pending requests are failed.
    exited: AtomicBool,
    /// Asks the waiter task to kill the child itself (the untracked case, a broken stdout pipe).
    kill: Arc<Notify>,
    /// The stdout reader (it also runs the exit path).
    reader: Mutex<Option<tokio::task::JoinHandle<()>>>,
    clock: Instant,
    trace: Option<Trace>,
}

/// A control_request already written, awaiting its control_response (`ClaudeProcess::post` / `await_reply`).
pub struct Posted {
    id: String,
    rx: oneshot::Receiver<Result<Value, ClaudeError>>,
}

/// Removes a pending request in all cases, including a dropped future.
struct PendingGuard<'a>(&'a Pending, String);

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().remove(&self.1);
    }
}

pub(crate) fn trace_line(ms: u128, what: &str) -> String {
    format!("+{ms} ms {what}")
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
        let argv = PREFIX_ARGS.iter().map(|a| a.to_string()).chain(args).collect();
        Self::start("claude", cwd, argv, resume_turn, on_event, on_exit, trace)
    }

    /// `spawn` with an explicit program and full argv (tests drive a shell script).
    fn start(
        program: &str,
        cwd: &str,
        argv: Vec<String>,
        resume_turn: bool,
        on_event: EventHandler,
        on_exit: ExitHandler,
        trace: Option<Trace>,
    ) -> Result<Arc<ClaudeProcess>, ClaudeError> {
        let mut cmd = tokio::process::Command::new(program);
        cmd.current_dir(cwd)
            .args(&argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // A non-interactive CLI only snapshots files before edits (what rewind_files restores) when asked to.
            .env("CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING", "1")
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_CHILD_SESSION");
        if resume_turn {
            cmd.env("CLAUDE_CODE_RESUME_INTERRUPTED_TURN", "1");
        }
        crate::process_job::ProcessJob::prepare(&mut cmd);
        let clock = Instant::now();
        let mut child = cmd.spawn().map_err(|e| ClaudeError::Io(format!("{program}: {e}")))?;
        let job = crate::process_job::ProcessJob::attach(&child);
        let pid = child.id().unwrap_or(0);
        let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
            return Err(ClaudeError::Io("stdio not piped".into()));
        };
        let me = Arc::new(ClaudeProcess {
            pid,
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
            stdin: tokio::sync::Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            stderr_tail: Mutex::new(VecDeque::new()),
            disposing: AtomicBool::new(false),
            job: Mutex::new(job),
            exited: AtomicBool::new(false),
            kill: Arc::new(Notify::new()),
            reader: Mutex::new(None),
            clock,
            trace,
        });
        me.trace(&format!("spawn pid={pid}"));

        // The waiter owns the child: it reaps it, or kills it when asked.
        let (exit_tx, exit_rx) = oneshot::channel();
        let kill = me.kill.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                s = child.wait() => s,
                _ = kill.notified() => {
                    let _ = child.start_kill();
                    child.wait().await
                }
            };
            let _ = exit_tx.send(status.ok().map(exit_code));
        });

        // stderr line by line, so what the CLI prints while it is slow shows up live.
        let weak = Arc::downgrade(&me);
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match lines.read_until(b'\n', &mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf).trim_end_matches(['\n', '\r']).to_string();
                        let Some(me) = weak.upgrade() else { break };
                        me.trace(&format!("stderr {line}"));
                        let mut tail = me.stderr_tail.lock();
                        tail.push_back(line);
                        if tail.len() > STDERR_LINES {
                            tail.pop_front();
                        }
                    }
                }
            }
        });

        let reader = tokio::spawn(read_stdout(Arc::downgrade(&me), stdout, stderr_task, exit_rx, on_event, on_exit));
        *me.reader.lock() = Some(reader);
        Ok(me)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn trace(&self, what: &str) {
        if let Some(t) = &self.trace {
            t(&trace_line(self.clock.elapsed().as_millis(), what));
        }
    }

    fn kill_tree(&self) {
        if let Some(job) = self.job.lock().as_ref() {
            job.kill();
        }
    }

    /// A control_response's `response`: complete the matching request.
    fn complete(&self, r: &Value) {
        let Some(id) = json::str(r, "request_id") else { return };
        let Some(tx) = self.pending.lock().remove(id) else {
            self.trace(&format!("late control_response {id}"));
            return;
        };
        let result = if json::str(r, "subtype") == Some("error") {
            Err(ClaudeError::Request(json::str(r, "error").map(str::to_string).unwrap_or_else(|| t!("Session.UnknownError"))))
        } else {
            Ok(r.get("response").cloned().unwrap_or_else(|| json!({})))
        };
        let _ = tx.send(result);
    }

    /// `{"type":"control_request","request_id":<32 hex>,"request":{"subtype":…, ...fields}}` and await the matching
    /// control_response (pending entry removed in all cases).
    pub async fn request(&self, subtype: &str, fields: Value, timeout: Duration) -> Result<Value, ClaudeError> {
        let posted = self.post(subtype, fields).await?;
        self.await_reply(posted, timeout).await
    }

    /// The write half of `request`: the control_request is on stdin when this returns (so a later write lands after
    /// it), its answer is awaited by `await_reply`. A `Posted` never passed to it leaves its pending entry until exit.
    pub async fn post(&self, subtype: &str, fields: Value) -> Result<Posted, ClaudeError> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id.clone(), tx);
        let guard = PendingGuard(&self.pending, id.clone());
        if self.exited.load(Ordering::SeqCst) {
            return Err(ClaudeError::Stopped);
        }
        let mut req = Map::new();
        req.insert("subtype".into(), Value::String(subtype.into()));
        if let Value::Object(f) = fields {
            req.extend(f);
        }
        self.write(json!({"type": "control_request", "request_id": id, "request": req})).await?;
        std::mem::forget(guard); // `await_reply` owns the cleanup from here
        Ok(Posted { id, rx })
    }

    /// The await half of `request`: the matching control_response, or Timeout (pending entry removed in all cases).
    pub async fn await_reply(&self, posted: Posted, timeout: Duration) -> Result<Value, ClaudeError> {
        let _guard = PendingGuard(&self.pending, posted.id);
        match tokio::time::timeout(timeout, posted.rx).await {
            Err(_) => Err(ClaudeError::Timeout),
            Ok(Err(_)) => Err(ClaudeError::Stopped),
            Ok(Ok(r)) => r,
        }
    }

    /// `{"type":"user","message":{"role":"user","content":Images.content(text, images)},"uuid":uuid}`.
    pub async fn send_user(&self, text: &str, images: &[UserImage], uuid: &str) -> Result<(), ClaudeError> {
        let mut msg = json!({"type": "user", "message": {"role": "user", "content": ccui_shared::images::content(text, images)}});
        if !uuid.is_empty() {
            msg["uuid"] = Value::String(uuid.into());
        }
        self.write(msg).await
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
        self.answer(request_id, Self::reply(allow, input, updated_permissions, message)).await
    }

    /// Any control_response success payload (elicitation replies).
    pub async fn answer(&self, request_id: &str, payload: Value) -> Result<(), ClaudeError> {
        self.write(json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": payload},
        }))
        .await
    }

    /// `request("interrupt")`.
    pub async fn interrupt(&self) -> Result<(), ClaudeError> {
        self.request("interrupt", Value::Null, DEFAULT_TIMEOUT).await.map(|_| ())
    }

    /// One serialized line on stdin, flushed.
    async fn write(&self, msg: Value) -> Result<(), ClaudeError> {
        let mut stdin = self.stdin.lock().await;
        let pipe = stdin.as_mut().ok_or(ClaudeError::Stopped)?;
        if self.trace.is_some() {
            let sub = msg.get("request").and_then(|r| json::str(r, "subtype")).map(|s| format!("/{s}")).unwrap_or_default();
            self.trace(&format!("write {}{sub}", json::str(&msg, "type").unwrap_or_default()));
        }
        let mut line = msg.to_string();
        line.push('\n');
        let io = |e: std::io::Error| match e.kind() {
            std::io::ErrorKind::BrokenPipe => ClaudeError::Stopped,
            _ => ClaudeError::Io(e.to_string()),
        };
        pipe.write_all(line.as_bytes()).await.map_err(io)?;
        pipe.flush().await.map_err(io)
    }

    /// disposing = true → close stdin → wait ≤ 2000 ms → kill tree → await readers.
    pub async fn dispose(&self) {
        self.disposing.store(true, Ordering::SeqCst);
        let deadline = Duration::from_millis(2000);
        let started = Instant::now();
        if let Ok(mut stdin) = tokio::time::timeout(deadline, self.stdin.lock()).await {
            stdin.take(); // closes the pipe: claude ends on EOF
        }
        let reader = self.reader.lock().take();
        if let Some(mut reader) = reader {
            let left = deadline.saturating_sub(started.elapsed());
            if tokio::time::timeout(left, &mut reader).await.is_err() {
                self.kill.notify_one();
                self.kill_tree();
                let _ = reader.await;
            }
        }
        self.kill_tree();
    }

    /// can_use_tool payload: allow `{"behavior":"allow","updatedInput":…[,"updatedPermissions":…]}`; deny
    /// `{"behavior":"deny","message":trimmed or "The user denied this tool use."}`.
    pub fn reply(allow: bool, input: &Value, updated_permissions: Option<Value>, message: Option<&str>) -> Value {
        if allow {
            let mut res = json!({"behavior": "allow", "updatedInput": input});
            if let Some(p) = updated_permissions {
                res["updatedPermissions"] = p;
            }
            res
        } else {
            let message = message.map(str::trim).filter(|m| !m.is_empty()).unwrap_or("The user denied this tool use.");
            json!({"behavior": "deny", "message": message})
        }
    }

    /// `claude` (`claude.exe` on Windows) found in a PATH dir (each trimmed of '"').
    pub fn on_path() -> bool {
        on_path(if cfg!(windows) { "claude.exe" } else { "claude" })
    }

    /// Windows && !on_path && `claude.cmd` on PATH (the not-found screen then points to the native installer).
    pub fn npm_shim_only() -> bool {
        cfg!(windows) && !Self::on_path() && on_path("claude.cmd")
    }
}

/// Same lookup as spawning `claude` without a shell: each PATH dir (trimmed of '"') holds the file.
fn on_path(exe: &str) -> bool {
    let sep = if cfg!(windows) { ';' } else { ':' };
    std::env::var("PATH")
        .unwrap_or_default()
        .split(sep)
        .filter(|d| !d.is_empty())
        .any(|d| std::path::Path::new(d.trim_matches('"')).join(exe).is_file())
}

/// .NET `Process.ExitCode`: the code, else 128 + signal on Unix, else -1.
fn exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128 + sig;
        }
    }
    status.code().unwrap_or(-1)
}

/// The stdout reader, then the exit path.
async fn read_stdout(
    weak: Weak<ClaudeProcess>,
    stdout: tokio::process::ChildStdout,
    stderr_task: tokio::task::JoinHandle<()>,
    exit_rx: oneshot::Receiver<Option<i32>>,
    on_event: EventHandler,
    on_exit: ExitHandler,
) {
    let mut lines = BufReader::new(stdout);
    let mut buf = Vec::new();
    let mut first = true;
    loop {
        buf.clear();
        match lines.read_until(b'\n', &mut buf).await {
            Ok(0) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                let Ok(evt) = serde_json::from_str::<Value>(text.trim_end_matches(['\n', '\r'])) else { continue };
                if let Some(me) = weak.upgrade() {
                    if first {
                        first = false;
                        me.trace(&format!("first stdout {}", json::str(&evt, "type").unwrap_or_default()));
                    }
                    if json::str(&evt, "type") == Some("control_response") {
                        if let Some(resp) = json::prop(&evt, "response") {
                            me.complete(resp);
                        }
                    }
                }
                let handler = on_event.clone();
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || handler(evt))).is_err() {
                    eprintln!("claude event handler failed");
                }
            }
            // Broken stdout pipe: the process can no longer be driven, stop it so the exit path below runs.
            Err(e) => {
                eprintln!("claude stdout reader failed: {e}");
                if let Some(me) = weak.upgrade() {
                    me.kill.notify_one();
                    me.kill_tree();
                }
                break;
            }
        }
    }
    let code = exit_rx.await.ok().flatten().unwrap_or(-1);
    let Some(me) = weak.upgrade() else { return };
    me.kill_tree(); // external kill or crash: take the MCP servers down with it, before on_exit
    me.exited.store(true, Ordering::SeqCst);
    let pending: Vec<_> = me.pending.lock().drain().map(|(_, tx)| tx).collect();
    for tx in pending {
        let _ = tx.send(Err(ClaudeError::Stopped));
    }
    let _ = stderr_task.await;
    if !me.disposing.load(Ordering::SeqCst) {
        let tail = me.stderr_tail.lock().iter().cloned().collect::<Vec<_>>().join("\n");
        let text = format!("{}\n{tail}", t!("Session.Exited", code)).trim().to_string();
        on_exit(Some(code), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_payloads() {
        let input = json!({"command": "ls"});
        assert_eq!(
            ClaudeProcess::reply(true, &input, None, Some("ignored")),
            json!({"behavior": "allow", "updatedInput": {"command": "ls"}})
        );
        let with = ClaudeProcess::reply(true, &input, Some(json!([{"type": "setMode"}])), None);
        assert_eq!(with.as_object().unwrap().keys().collect::<Vec<_>>(), ["behavior", "updatedInput", "updatedPermissions"]);
        assert_eq!(ClaudeProcess::reply(false, &input, Some(json!([])), Some("  add tests  ")), json!({"behavior": "deny", "message": "add tests"}));
        assert_eq!(ClaudeProcess::reply(false, &input, None, Some(" "))["message"], "The user denied this tool use.");
        assert_eq!(ClaudeProcess::reply(false, &input, None, None)["message"], "The user denied this tool use.");
    }

    #[test]
    fn trace_line_format() {
        assert_eq!(trace_line(42, "spawn pid=7"), "+42 ms spawn pid=7");
    }

    #[test]
    fn error_messages() {
        assert_eq!(ClaudeError::Request("boom".into()).to_string(), "boom");
        assert_eq!(String::from(ClaudeError::Io("x".into())), "x");
        assert!(!ClaudeError::Stopped.to_string().is_empty());
    }

    #[cfg(unix)]
    fn sh(script: &str, on_event: EventHandler, on_exit: ExitHandler, trace: Option<Trace>) -> Arc<ClaudeProcess> {
        ClaudeProcess::start("/bin/sh", ".", vec!["-c".into(), script.into()], false, on_event, on_exit, trace).unwrap()
    }

    /// A fake CLI: answers two control requests by request_id (success, then error), prints a non-JSON line, a JSON
    /// line and stderr, then exits 3.
    #[cfg(unix)]
    #[tokio::test]
    async fn requests_events_and_exit() {
        let script = r#"
id() { printf '%s' "$1" | sed 's/.*"request_id":"\([0-9a-f]*\)".*/\1/'; }
read l; printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"models":[1,2],"env":"%s"}}}\n' "$(id "$l")" "$CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING"
read l; printf '{"type":"control_response","response":{"subtype":"error","request_id":"%s","error":"boom"}}\n' "$(id "$l")"
read l; echo "not json"; echo '{"type":"system","subtype":"status"}'; echo oops >&2; exit 3
"#;
        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let (exit_tx, exit_rx) = std::sync::mpsc::channel();
        let traced = Arc::new(Mutex::new(Vec::<String>::new()));
        let ev = events.clone();
        let tr = traced.clone();
        let p = sh(
            script,
            Arc::new(move |v| ev.lock().push(v)),
            Arc::new(move |c, t| exit_tx.send((c, t)).unwrap()),
            Some(Arc::new(move |l: &str| tr.lock().push(l.to_string()))),
        );
        assert!(p.pid() > 0);
        let r = p.request("initialize", json!({"hooks": {}}), Duration::from_secs(10)).await.unwrap();
        assert_eq!(r, json!({"models": [1, 2], "env": "1"}));
        assert_eq!(p.request("mcp_status", Value::Null, Duration::from_secs(10)).await, Err(ClaudeError::Request("boom".into())));
        p.send_user_raw_for_test().await;
        let (code, text) = tokio::task::spawn_blocking(move || exit_rx.recv_timeout(Duration::from_secs(10)).unwrap()).await.unwrap();
        assert_eq!(code, Some(3));
        assert!(text.contains("(3)") && text.ends_with("\noops"), "{text}");
        let events = events.lock();
        assert_eq!(events.len(), 3, "non-JSON skipped, control_responses passed on");
        assert_eq!(json::str(&events[2], "subtype"), Some("status"));
        assert!(p.pending.lock().is_empty());
        assert!(matches!(p.request("interrupt", Value::Null, Duration::from_secs(5)).await, Err(ClaudeError::Stopped)));
        let traced = traced.lock();
        assert!(traced[0].starts_with('+') && traced[0].contains(" ms spawn pid="));
        assert!(traced.iter().any(|l| l.ends_with("write control_request/initialize")));
        assert!(traced.iter().any(|l| l.ends_with("first stdout control_response")));
        assert!(traced.iter().any(|l| l.ends_with("stderr oops")));
    }

    /// A CLI that ignores stdin EOF: dispose waits 2 s, kills the tree, and does not call on_exit.
    #[cfg(unix)]
    #[tokio::test]
    async fn dispose_kills_a_stuck_cli() {
        let exited = Arc::new(AtomicBool::new(false));
        let ex = exited.clone();
        let p = sh("trap '' TERM; sleep 60", Arc::new(|_| {}), Arc::new(move |_, _| ex.store(true, Ordering::SeqCst)), None);
        let started = Instant::now();
        p.dispose().await;
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!exited.load(Ordering::SeqCst));
        assert!(matches!(p.write(json!({"type": "user"})).await, Err(ClaudeError::Stopped)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn request_times_out_and_spawn_fails_cleanly() {
        let p = sh("sleep 60", Arc::new(|_| {}), Arc::new(|_, _| {}), None);
        assert_eq!(p.request("initialize", Value::Null, Duration::from_millis(200)).await, Err(ClaudeError::Timeout));
        assert!(p.pending.lock().is_empty());
        p.dispose().await;
        let err = ClaudeProcess::start("/no/such/claude", ".", vec![], false, Arc::new(|_| {}), Arc::new(|_, _| {}), None);
        assert!(matches!(err, Err(ClaudeError::Io(_))));
    }

    impl ClaudeProcess {
        /// Wakes the fake CLI's third `read` without going through `images::content` (another unit's code).
        async fn send_user_raw_for_test(&self) {
            self.write(json!({"type": "user", "message": {"role": "user", "content": "go"}})).await.unwrap();
        }
    }
}
