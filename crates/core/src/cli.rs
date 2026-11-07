//! Port of `Plugins.Run` — the generic "run a claude subcommand" primitive shared by Plugins, CliEdit,
//! BackgroundAgents, StatusPanel (`claude auth logout`) and the auto-title — spec 05 §1 "Run process contract".
//! Owner: core-extensions.
//!
//! `claude <command> <args…>` (same PATH resolution as the session spawner, argv list, no shell), cwd given, stdin a
//! pipe written then closed (never a TTY: confirmations are refused, not awaited), UTF-8 without BOM, stdout/stderr
//! drained concurrently, env minus `CLAUDECODE` and `CLAUDE_CODE_CHILD_SESSION`, no window on Windows, timeout
//! (default 5 min) kills the whole tree → Err("claude <command> <args…>").

use crate::process_job::ProcessJob;
use crate::CoreResult;
use ccui_shared::api::CliOutput;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

pub async fn run_claude(cwd: &str, command: &str, args: &[String], stdin: Option<&str>, timeout: Duration) -> CoreResult<CliOutput> {
    run_program("claude", cwd, command, args, stdin, timeout).await
}

/// `run_claude` with an explicit program (tests drive `sh`).
async fn run_program(program: &str, cwd: &str, command: &str, args: &[String], stdin: Option<&str>, timeout: Duration) -> CoreResult<CliOutput> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.current_dir(cwd)
        .arg(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A person accepts here, not an agent: inherited from a Claude Code shell, these make the CLI ignore
        // --accept-command (asking again forever) and the edit-* writers refuse to run.
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_CHILD_SESSION")
        .kill_on_drop(true);
    ProcessJob::prepare(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("{program}: {e}"))?;
    let job = ProcessJob::attach(&child);
    let (si, so, se) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let input = stdin.map(str::to_string);
    let run = async {
        let write = async move {
            if let Some(mut si) = si {
                if let Some(text) = input {
                    let _ = si.write_all(text.as_bytes()).await;
                }
                // dropped here: stdin closed
            }
        };
        let ((), out, err) = tokio::join!(write, read_all(so), read_all(se));
        (child.wait().await, out, err)
    };
    let done = tokio::time::timeout(timeout, run).await;
    match done {
        Ok((status, stdout, stderr)) => {
            // The tree is killed on timeout only, like the C# Process: whatever it started on purpose (`claude daemon`)
            // outlives a normal exit. ponytail: leaks the job (a Windows handle per call); add a disarm to ProcessJob if
            // that ever matters.
            std::mem::forget(job);
            Ok(CliOutput { code: status.map(exit_code).unwrap_or(-1), stdout, stderr })
        }
        Err(_) => {
            if let Some(j) = &job {
                j.kill();
            }
            let _ = child.start_kill();
            Err(format!("claude {command} {}", args.join(" ")).trim_end().to_string())
        }
    }
}

async fn read_all<R: tokio::io::AsyncRead + Unpin>(r: Option<R>) -> String {
    let mut buf = Vec::new();
    if let Some(mut r) = r {
        let _ = r.read_to_end(&mut buf).await;
    }
    String::from_utf8_lossy(&buf).into_owned()
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

/// StatusPanel "Sign out": `claude auth logout` in cwd.
pub async fn auth_logout(cwd: &str) -> CoreResult<CliOutput> {
    run_claude(cwd, "auth", &["logout".to_string()], None, DEFAULT_TIMEOUT).await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[tokio::test]
    async fn stdin_env_and_exit_code() {
        std::env::set_var("CLAUDE_CODE_CHILD_SESSION", "1");
        let o = run_program("sh", "/", "-c", &s(&["echo \"[${CLAUDE_CODE_CHILD_SESSION-}]\"; cat; echo err >&2; exit 3"]), Some("héllo"), DEFAULT_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(o.stdout, "[]\nhéllo");
        assert_eq!(o.stderr, "err\n");
        assert_eq!(o.code, 3);
    }

    #[tokio::test]
    async fn timeout_kills_and_names_the_command() {
        let t = std::time::Instant::now();
        let e = run_program("sh", "/", "-c", &s(&["sleep 30"]), None, Duration::from_millis(200)).await.unwrap_err();
        assert_eq!(e, "claude -c sleep 30");
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn spawn_failure_is_an_error() {
        assert!(run_program("definitely-not-a-program-ccui", "/", "x", &[], None, DEFAULT_TIMEOUT).await.is_err());
    }
}
