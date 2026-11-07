//! Port of `Core/Plugins.cs` (CLI calls) — spec 05 §1. Owner: core-extensions.
//! The stream-json protocol has no request for installs/marketplaces/settings: shell out to
//! `claude plugin <sub> … --json` in the session's folder (or home), parse with `ccui_shared::plugins`; the UI then
//! reloads the running session (`session_reload_plugins`).

use crate::CoreResult;
use ccui_shared::plugins::{Market, Outcome, PluginList, PluginOption};
use serde_json::{Map, Value};

/// `claude plugin list --json --available` (non-zero → Err(line(err,out))).
pub async fn list(cwd: &str) -> CoreResult<PluginList> {
    todo!("port: Plugins.List")
}

/// `claude plugin marketplace list --json`.
pub async fn markets(cwd: &str) -> CoreResult<Vec<Market>> {
    todo!("port: Plugins.Markets")
}

/// `claude plugin <args…> --json` (exit code ignored): parse_outcome(stdout) ?? Outcome(false, line(err,out)).
/// args e.g. `["install", id, "--scope", s]`, `["marketplace", "add", src]`, + `["--accept-command", sha]` on accept.
pub async fn act(cwd: &str, args: Vec<String>) -> CoreResult<Outcome> {
    todo!("port: Plugins.Act")
}

/// `claude plugin configure <id> --json`.
pub async fn options(cwd: &str, id: &str) -> CoreResult<Vec<PluginOption>> {
    todo!("port: Plugins.Options")
}

/// `claude plugin configure <id> --values-stdin --json` with only the edited keys (single-line strings).
/// Ok(None) = saved; Ok(Some(message)) = refused.
pub async fn save(cwd: &str, id: &str, values: Map<String, Value>) -> CoreResult<Option<String>> {
    todo!("port: Plugins.Save")
}
