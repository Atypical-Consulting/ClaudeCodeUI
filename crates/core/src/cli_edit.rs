//! Port of `CliEdit.Run` — the CLI's own settings writers (`claude edit-hook --json`, `claude edit-memory-settings
//! --json`) — spec 04 §8. Owner: core-extensions. Payload builders are `ccui_shared::cli_edit`.

use serde_json::Value;

/// Edit JSON on stdin; exit 0 → None (written), else Some(`cli_edit::error(stdout, stderr, command)`).
pub async fn run(cwd: &str, command: &str, edit: &Value) -> Option<String> {
    todo!("port: CliEdit.Run")
}
