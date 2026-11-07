//! Port of `CliEdit.Run` — the CLI's own settings writers (`claude edit-hook --json`, `claude edit-memory-settings
//! --json`) — spec 04 §8. Owner: core-extensions. Payload builders are `ccui_shared::cli_edit`.

use crate::cli;
use serde_json::Value;

/// Edit JSON on stdin; exit 0 → None (written), else Some(`cli_edit::error(stdout, stderr, command)`).
/// A spawn failure or timeout is Some(its message) too.
pub async fn run(cwd: &str, command: &str, edit: &Value) -> Option<String> {
    match cli::run_claude(cwd, command, &["--json".to_string()], Some(&edit.to_string()), cli::DEFAULT_TIMEOUT).await {
        Ok(o) if o.code == 0 => None,
        Ok(o) => Some(ccui_shared::cli_edit::error(&o.stdout, &o.stderr, command)),
        Err(e) => Some(e),
    }
}
