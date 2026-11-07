//! Port of the pure parts of `Core/StatusCommand.cs` (the settings' statusLine command) — spec 04 §9.
//! Owner: core-live-session. `payload` / `run` are `ccui_core::status_command`.

use serde_json::Value;

/// `settings.effective.statusLine.command` when type == "command" and non-empty (input: get_settings response).
pub fn of(settings: &Value) -> Option<String> {
    todo!("port: StatusCommand.Of")
}

/// `PermissionRules::plain(stdout)`, lines trimmed at the end, blanks dropped, at most 3, joined `\n`; none → None.
pub fn text(stdout: &str) -> Option<String> {
    todo!("port: StatusCommand.Text")
}
