//! Port of `Core/BackgroundAgents.cs` (I/O parts) — spec 03 §5. Owner: core-store-sessions.
//! `claude agents --json --all`, `claude daemon status`, `claude <logs|stop|respawn|rm> <id>` via `cli::run_claude`
//! (cwd = home). Pure parsing/ordering in `ccui_shared::bg_agents`.

use crate::cli::{run_claude, DEFAULT_TIMEOUT};
use crate::{CoreResult, UiSink};
use ccui_shared::api::{BgList, BgRunResult};
use ccui_shared::bg_agents::{job, order, parse, Agent};
use ccui_shared::push::Push;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

static NEEDS_YOU: AtomicI32 = AtomicI32::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);

fn home() -> String {
    crate::paths::home().to_string_lossy().into_owned()
}

fn set_needs_you(n: i32, sink: &Arc<dyn UiSink>) {
    if NEEDS_YOU.swap(n, Ordering::SeqCst) != n {
        sink.emit(Push::BgAgents { needs_you: n });
    }
}

/// List rows (+ jobs/<id>/state.json + timeline), daemon status, update NeedsYouCount (emit `Push::BgAgents` on
/// change), order. On error: NeedsYouCount → 0 (emitted) then Err.
pub async fn list(sink: &Arc<dyn UiSink>) -> CoreResult<BgList> {
    let r = read().await;
    match &r {
        Ok((rows, _)) => set_needs_you(rows.iter().filter(|a| a.needs_you()).count() as i32, sink),
        // Unknown is not "1 needs you": the rail claims nothing.
        Err(_) => set_needs_you(0, sink),
    }
    r.map(|(rows, daemon_up)| BgList { rows: order(rows), daemon_up })
}

async fn read() -> CoreResult<(Vec<Agent>, bool)> {
    let out = run_claude(&home(), "agents", &["--json".into(), "--all".into()], None, DEFAULT_TIMEOUT).await?;
    if out.code != 0 {
        return Err(format!("{}\n{}", out.stderr, out.stdout).trim().to_string());
    }
    let json: Value = serde_json::from_str(&out.stdout).map_err(|e| e.to_string())?;
    let mut rows: Vec<Agent> = parse(&json).into_iter().map(with_job).collect();
    // `daemon status` never starts it. The daemon down means no background session runs: the list still says
    // their last state.
    let up = !rows.iter().any(Agent::background)
        || !run_claude(&home(), "daemon", &["status".into()], None, DEFAULT_TIMEOUT).await?.stdout.trim_start().starts_with("not running");
    if !up {
        rows.iter_mut().filter(|r| r.background()).for_each(|r| r.stale = true);
    }
    Ok((rows, up))
}

/// The session's own job record (`~/.claude/jobs/<id>`): the question it waits on, its last step.
fn with_job(a: Agent) -> Agent {
    if !a.background() || a.id.is_empty() || a.id.contains('/') || a.id.contains('.') || a.id.contains('\\') {
        return a; // path guard
    }
    let dir = crate::paths::jobs_dir().join(&a.id);
    let state = match std::fs::read_to_string(dir.join("state.json")) {
        Ok(t) => match serde_json::from_str::<Value>(&t) {
            Ok(v) => Some(v),
            Err(_) => return a,
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return a,
    };
    let last = match std::fs::read(dir.join("timeline.jsonl")) {
        Ok(b) => match String::from_utf8_lossy(&b).lines().rfind(|l| !l.is_empty()) {
            Some(l) => match serde_json::from_str::<Value>(l) {
                Ok(v) => Some(v),
                Err(_) => return a,
            },
            None => None,
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return a,
    };
    job(a, state.as_ref(), last.as_ref())
}

/// Start the 1-minute watcher once per process (errors ignored). Called when the Rail mounts.
pub fn watch(sink: Arc<dyn UiSink>) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        loop {
            let _ = list(&sink).await; // list already reset the count
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// `claude <command> <id>`; command must be logs | stop | respawn | rm. text = (stdout + "\n" + stderr).trim().
pub async fn run(command: &str, id: &str) -> CoreResult<BgRunResult> {
    if !matches!(command, "logs" | "stop" | "respawn" | "rm") {
        return Err(format!("unknown command: {command}"));
    }
    let out = run_claude(&home(), command, &[id.to_string()], None, DEFAULT_TIMEOUT).await?;
    // rm says what it would not delete, and the flag that would.
    Ok(BgRunResult { ok: out.code == 0, text: format!("{}\n{}", out.stdout, out.stderr).trim().to_string() })
}

pub fn needs_you_count() -> i32 {
    NEEDS_YOU.load(Ordering::SeqCst)
}
