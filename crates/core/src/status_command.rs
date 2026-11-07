//! Port of the I/O parts of `Core/StatusCommand.cs` — spec 04 §9. Owner: core-live-session.
//! (`of` / `text` are pure in `ccui_shared::status_command`.)

use ccui_shared::session::SessionMeta;
use serde_json::Value;

/// The JSON piped on stdin (the CLI's StatusLineCommandInput fields the app knows). `transcript_path` =
/// `transcript_store::find(id)` or "".
pub fn payload(meta: &SessionMeta, transcript_path: Option<&str>) -> Value {
    todo!("port: StatusCommand.Payload")
}

/// `/bin/sh -c` (Windows `cmd.exe /d /s /c`) in cwd, payload on stdin, 5 s budget for exit AND stdout (timeout →
/// kill tree, None) → `status_command::text(stdout)`.
pub async fn run(command: &str, cwd: &str, payload: &Value) -> Option<String> {
    todo!("port: StatusCommand.Run")
}
