//! Port of `Core/BackgroundAgents.cs` (I/O parts) — spec 03 §5. Owner: core-store-sessions.
//! `claude agents --json --all`, `claude daemon status`, `claude <logs|stop|respawn|rm> <id>` via `cli::run_claude`
//! (cwd = home). Pure parsing/ordering in `ccui_shared::bg_agents`.

use crate::{CoreResult, UiSink};
use ccui_shared::api::{BgList, BgRunResult};
use std::sync::Arc;

/// List rows (+ jobs/<id>/state.json + timeline), daemon status, update NeedsYouCount (emit `Push::BgAgents` on
/// change), order. On error: NeedsYouCount → 0 (emitted) then Err.
pub async fn list(sink: &Arc<dyn UiSink>) -> CoreResult<BgList> {
    todo!("port: BackgroundAgents.List")
}

/// Start the 1-minute watcher once per process (errors ignored). Called when the Rail mounts.
pub fn watch(sink: Arc<dyn UiSink>) {
    todo!("port: BackgroundAgents.Watch")
}

/// `claude <command> <id>`; command must be logs | stop | respawn | rm. text = (stdout + "\n" + stderr).trim().
pub async fn run(command: &str, id: &str) -> CoreResult<BgRunResult> {
    todo!("port: BackgroundAgents.Run")
}

pub fn needs_you_count() -> i32 {
    todo!("port: BackgroundAgents.NeedsYouCount")
}
