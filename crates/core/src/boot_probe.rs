//! Port of `Core/BootProbe.cs` (`--boot-probe <dir> [seconds=90]`) — spec 03 §10. Owner: core-events.
//! Hidden CLI flag of the Tauri binary handled before `tauri::Builder` (see desktop main.rs): spawns claude with the
//! args of a fresh session, sends `initialize`, prints `initialize OK in <ms> ms (<n> models, <n> commands)` (exit 0),
//! `initialize TIMEOUT after <s> s` / `initialize ERROR <msg>` (exit 1); process exit printed as `exited <code>: <text>`.
//! (`--self-check` becomes `cargo test`; `--probe-cli` is not ported.)

use crate::live::{args::args, state::SessionState};
use crate::process::{ClaudeError, ClaudeProcess};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Run the probe; returns the process exit code.
pub async fn run(dir: &str, seconds: u64) -> i32 {
    let state = SessionState::new(&uuid::Uuid::new_v4().to_string(), "boot-probe", dir, "default", chrono::Utc::now());
    let argv = args(&state, false, crate::app_flag::flag(ccui_shared::api::FLAG_THINKING));
    let spawned = ClaudeProcess::spawn(
        dir,
        argv,
        false,
        Arc::new(|_| {}),
        Arc::new(|code, text| println!("exited {}: {text}", code.unwrap_or(-1))),
        Some(Arc::new(|line: &str| println!("{line}"))),
    );
    let p = match spawned {
        Ok(p) => p,
        Err(e) => {
            println!("initialize ERROR {e}");
            return 1;
        }
    };
    let started = Instant::now();
    let code = match p.request("initialize", Value::Null, Duration::from_secs(seconds)).await {
        Ok(info) => {
            println!(
                "initialize OK in {} ms ({} models, {} commands)",
                started.elapsed().as_millis(),
                count(&info, "models"),
                count(&info, "commands")
            );
            0
        }
        Err(ClaudeError::Timeout) => {
            println!("initialize TIMEOUT after {seconds} s");
            1
        }
        Err(e) => {
            println!("initialize ERROR {e}");
            1
        }
    };
    p.dispose().await;
    code
}

fn count(info: &Value, name: &str) -> usize {
    ccui_shared::json::prop(info, name).and_then(Value::as_array).map_or(0, Vec::len)
}
