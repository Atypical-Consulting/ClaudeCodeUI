//! Port of `Core/Plugins.cs` (CLI calls) — spec 05 §1. Owner: core-extensions.
//! The stream-json protocol has no request for installs/marketplaces/settings: shell out to
//! `claude plugin <sub> … --json` in the session's folder (or home), parse with `ccui_shared::plugins`; the UI then
//! reloads the running session (`session_reload_plugins`).

use crate::cli;
use crate::CoreResult;
use ccui_shared::api::CliOutput;
use ccui_shared::memory::replace_line_endings;
use ccui_shared::plugins::{self as p, Market, Outcome, PluginList, PluginOption};
use serde_json::{Map, Value};

async fn run(cwd: &str, args: &[&str], stdin: Option<&str>) -> CoreResult<CliOutput> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    cli::run_claude(cwd, "plugin", &args, stdin, cli::DEFAULT_TIMEOUT).await
}

/// Exit 0 → the parsed stdout, else Err(line(err, out)).
async fn query(cwd: &str, args: &[&str]) -> CoreResult<Value> {
    let o = run(cwd, args, None).await?;
    if o.code != 0 {
        return Err(p::line(&o.stderr, &o.stdout));
    }
    serde_json::from_str(&o.stdout).map_err(|e| e.to_string())
}

/// `claude plugin list --json --available` (non-zero → Err(line(err,out))).
pub async fn list(cwd: &str) -> CoreResult<PluginList> {
    Ok(p::parse_list(&query(cwd, &["list", "--json", "--available"]).await?))
}

/// `claude plugin marketplace list --json`.
pub async fn markets(cwd: &str) -> CoreResult<Vec<Market>> {
    Ok(p::parse_markets(&query(cwd, &["marketplace", "list", "--json"]).await?))
}

/// `claude plugin <args…> --json` (exit code ignored): parse_outcome(stdout) ?? Outcome(false, line(err,out)).
/// args e.g. `["install", id, "--scope", s]`, `["marketplace", "add", src]`, + `["--accept-command", sha]` on accept.
pub async fn act(cwd: &str, args: Vec<String>) -> CoreResult<Outcome> {
    let mut args = args;
    args.push("--json".into());
    let o = cli::run_claude(cwd, "plugin", &args, None, cli::DEFAULT_TIMEOUT).await?;
    Ok(p::parse_outcome(&o.stdout).unwrap_or_else(|| Outcome { ok: false, message: p::line(&o.stderr, &o.stdout), ..Default::default() }))
}

/// `claude plugin configure <id> --json`.
pub async fn options(cwd: &str, id: &str) -> CoreResult<Vec<PluginOption>> {
    Ok(p::parse_options(&query(cwd, &["configure", id, "--json"]).await?))
}

/// `claude plugin configure <id> --values-stdin --json` with only the edited keys (single-line strings).
/// Ok(None) = saved; Ok(Some(message)) = refused.
pub async fn save(cwd: &str, id: &str, values: Map<String, Value>) -> CoreResult<Option<String>> {
    let o = run(cwd, &["configure", id, "--values-stdin", "--json"], Some(&values_json(&values))).await?;
    Ok((o.code != 0).then(|| p::save_error(&o.stdout, &o.stderr)))
}

/// The options left out keep their values; every value a single-line string.
fn values_json(values: &Map<String, Value>) -> String {
    let m: Map<String, Value> =
        values.iter().map(|(k, v)| (k.clone(), Value::String(replace_line_endings(&ccui_shared::json::to_text(v), " ")))).collect();
    Value::Object(m).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_payload() {
        let v: Map<String, Value> = serde_json::from_str(r#"{"b":"x\r\ny","a":"1","n":3}"#).unwrap();
        assert_eq!(values_json(&v), r#"{"b":"x y","a":"1","n":"3"}"#);
    }
}
