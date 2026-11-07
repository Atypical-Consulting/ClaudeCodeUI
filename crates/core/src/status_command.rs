//! Port of the I/O parts of `Core/StatusCommand.cs` — spec 04 §9. Owner: core-live-session.
//! (`of` / `text` are pure in `ccui_shared::status_command`.)
//! ponytail: event-driven only (after boot and each turn); refreshInterval and subagentStatusLine are not run.

use crate::process_job::ProcessJob;
use ccui_shared::json;
use ccui_shared::session::SessionMeta;
use serde_json::{json, Map, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// C# `default(DateTimeOffset).ToUnixTimeSeconds()`: a window the CLI never reported.
const NO_RESET: i64 = -62_135_596_800;

/// The JSON piped on stdin (the CLI's StatusLineCommandInput fields the app knows). `transcript_path` =
/// `transcript_store::find(id)` or "".
pub fn payload(meta: &SessionMeta, transcript_path: Option<&str>) -> Value {
    payload_with(meta, None, transcript_path)
}

/// `ModelMenu.Current`: the initialize_info.models entry for the session's model (by value, else by resolved id).
fn current_model<'a>(info: Option<&'a Value>, model: Option<&str>) -> Option<&'a Value> {
    let models = info.and_then(|i| json::prop(i, "models")).and_then(Value::as_array)?;
    if let Some(m) = models.iter().find(|m| json::str(m, "value") == model && m.is_object()) {
        return Some(m);
    }
    let x = model?.to_lowercase();
    let resolved: Vec<&Value> =
        models.iter().filter(|m| m.is_object() && json::str(m, "resolvedModel").is_some_and(|r| x.starts_with(&r.to_lowercase()))).collect();
    resolved.iter().find(|m| json::str(m, "value") != Some("default")).or(resolved.first()).copied()
}

/// `payload` with the initialize answer, for `model.display_name` and `version` (additive: the session runner has it).
pub fn payload_with(meta: &SessionMeta, initialize_info: Option<&Value>, transcript_path: Option<&str>) -> Value {
    let display = current_model(initialize_info, meta.model.as_deref()).and_then(|m| json::str(m, "displayName")).map(str::to_string);
    let cost = rust_decimal::prelude::ToPrimitive::to_f64(&meta.cost_usd).unwrap_or(0.0);
    let mut o = Map::new();
    o.insert("session_id".into(), json!(meta.id));
    o.insert("transcript_path".into(), json!(transcript_path.unwrap_or("")));
    o.insert("cwd".into(), json!(meta.cwd));
    o.insert("session_name".into(), json!(meta.name));
    o.insert("permission_mode".into(), json!(meta.mode));
    o.insert("model".into(), json!({"id": meta.model, "display_name": display.or_else(|| meta.model.clone())}));
    o.insert("workspace".into(), json!({"current_dir": meta.cwd, "project_dir": meta.cwd, "added_dirs": meta.add_dirs}));
    o.insert("version".into(), json!(initialize_info.and_then(|i| json::str(i, "claude_code_version"))));
    o.insert("output_style".into(), json!({"name": meta.output_style.as_deref().unwrap_or("default")}));
    o.insert(
        "cost".into(),
        json!({"total_cost_usd": cost, "total_duration_ms": (chrono::Utc::now() - meta.started_at).num_milliseconds()}),
    );
    o.insert("fast_mode".into(), json!(meta.fast_mode_state == "on"));
    o.insert("thinking".into(), json!({"enabled": meta.thinking}));
    if let Some(e) = &meta.effort {
        o.insert("effort".into(), json!({"level": e}));
    }
    if let Some(l) = &meta.limits {
        // Math.Round(x * 100, 1): banker's rounding.
        let pct = |v: f64| (v * 1000.0).round_ties_even() / 10.0;
        let secs = |t: Option<ccui_shared::model::Time>| t.map_or(NO_RESET, |t| t.timestamp());
        o.insert(
            "rate_limits".into(),
            json!({
                "five_hour": {"used_percentage": pct(l.five_hour), "resets_at": secs(l.five_hour_reset)},
                "seven_day": {"used_percentage": pct(l.seven_day), "resets_at": secs(l.seven_day_reset)},
            }),
        );
    }
    Value::Object(o)
}

/// `/bin/sh -c` (Windows `cmd.exe /d /s /c`) in cwd, payload on stdin, 5 s budget for exit AND stdout (timeout →
/// kill tree, None) → `status_command::text(stdout)`.
pub async fn run(command: &str, cwd: &str, payload: &Value) -> Option<String> {
    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd.exe");
        c.args(["/d", "/s", "/c", command]);
        c
    } else {
        let mut c = tokio::process::Command::new("/bin/sh");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    ProcessJob::prepare(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("statusLine: {e}");
            return None;
        }
    };
    let job = ProcessJob::attach(&child);
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    let out = tokio::spawn(async move {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b).await;
        String::from_utf8_lossy(&b).into_owned()
    });
    // drained: a chatty script must not block on a full pipe
    tokio::spawn(async move {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b).await;
    });
    if let Some(mut stdin) = child.stdin.take() {
        // a command that does not read stdin may exit first
        let _ = stdin.write_all(payload.to_string().as_bytes()).await;
    }
    // The read is bounded too: a background child of the script (`git fetch &`) can hold stdout open after the shell exits.
    let work = async {
        let _ = child.wait().await;
        out.await.unwrap_or_default()
    };
    match tokio::time::timeout(Duration::from_secs(5), work).await {
        Ok(stdout) => ccui_shared::status_command::text(&stdout),
        Err(_) => {
            if let Some(j) = &job {
                j.kill();
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> SessionMeta {
        SessionMeta {
            id: "sl".into(),
            name: "name".into(),
            cwd: "/w".into(),
            mode: "plan".into(),
            model: Some("opus".into()),
            effort: Some("high".into()),
            started_at: chrono::Utc::now(),
            ..Default::default()
        }
    }

    #[test]
    fn payload_fields() {
        let o = payload(&meta(), None);
        assert_eq!(o["session_id"], "sl");
        assert_eq!(o["workspace"]["current_dir"], "/w");
        assert_eq!(o["effort"]["level"], "high");
        assert_eq!(o["output_style"]["name"], "default");
        assert!(o.get("rate_limits").is_none());
        assert_eq!(o["model"]["display_name"], "opus");
        assert_eq!(o["transcript_path"], "");
        let info = json!({"models":[{"value":"default","resolvedModel":"claude-opus-5-5"},{"value":"opus","displayName":"Opus 5.5","resolvedModel":"claude-opus-5-5"}],"claude_code_version":"2.1.296"});
        let o = payload_with(&meta(), Some(&info), Some("/t.jsonl"));
        assert_eq!(o["model"]["display_name"], "Opus 5.5");
        assert_eq!(o["version"], "2.1.296");
        let resolved = SessionMeta { model: Some("claude-opus-5-5[1m]".into()), ..meta() };
        assert_eq!(payload_with(&resolved, Some(&info), None)["model"]["display_name"], "Opus 5.5", "resolved id, not the default row");
        let limited = SessionMeta {
            limits: Some(ccui_shared::model::RateLimits { five_hour: 0.4235, seven_day: 0.91, ..Default::default() }),
            ..meta()
        };
        let rl = &payload(&limited, None)["rate_limits"];
        assert_eq!(rl["five_hour"]["used_percentage"], 42.4);
        assert_eq!(rl["seven_day"]["resets_at"], NO_RESET);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_command_reads_the_payload_on_stdin() {
        let got = run("cat | head -c 200 | grep -o '\"session_id\":\"sl\"'", &std::env::temp_dir().to_string_lossy(), &payload(&meta(), None)).await;
        assert_eq!(got.as_deref(), Some("\"session_id\":\"sl\""));
        assert_eq!(run("sleep 30", "/", &json!({})).await, None, "5 s budget");
    }
}
