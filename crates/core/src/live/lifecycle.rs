//! Port of the lifecycle of `Core/LiveSession.cs`: EnsureProcess, Boot, OnExit, Restart, DisposeAsync,
//! RunStatusLine, RefreshGit, OnEvent — spec 01 §5, §7.1. Owner: core-live-session.

use super::LiveSession;
use crate::process::ClaudeProcess;
use crate::CoreResult;
use serde_json::Value;
use std::sync::Arc;

impl LiveSession {
    /// Locked check-then-spawn (two windows / boot + reader must not start two claude on one --session-id):
    /// existing proc → it; stopped → Err(Session.ProcessStopped); spawn (failure → on_exit(None, -1,
    /// Session.StartFailed) then Err); status Starting; then notify and fire `boot`.
    pub async fn ensure_process(self: &Arc<Self>, continue_turn: bool) -> CoreResult<Arc<ClaudeProcess>> {
        todo!("port: LiveSession.EnsureProcess")
    }

    /// initialize (180 s, waited ONCE), thinking-off live 0, get_settings, get_usage, then Starting → Idle and the
    /// status line. Initialize timeout → crash banner (on_exit), never a fake Idle.
    pub(crate) async fn boot(self: Arc<Self>, p: Arc<ClaudeProcess>) {
        todo!("port: LiveSession.Boot")
    }

    /// Process exited (`generation` None = never spawned). Ignored when a newer process replaced it. Returns whether
    /// it applied (status Crashed, pending/queue cleared, tools ended, plugin status cleared…).
    pub(crate) fn on_exit(&self, generation: Option<u64>, code: Option<i32>, text: String) -> bool {
        todo!("port: LiveSession.OnExit")
    }

    /// Drop the current process first (its exit is then ignored), dispose it, respawn (re-running a crash-cut turn).
    pub async fn restart(self: &Arc<Self>) -> CoreResult<()> {
        todo!("port: LiveSession.Restart")
    }

    /// SessionManager.Stop: stopped = true, dispose the process, Exited unless Crashed, queues cleared.
    pub async fn dispose(&self) {
        todo!("port: LiveSession.DisposeAsync")
    }

    /// Run the statusLine command (one at a time); a non-empty result replaces status_text.
    pub(crate) async fn run_status_line(self: Arc<Self>) {
        todo!("port: LiveSession.RunStatusLine")
    }

    /// branch (`git branch --show-current`) and repo (common dir's parent name).
    pub async fn refresh_git(self: Arc<Self>) {
        todo!("port: LiveSession.RefreshGit")
    }

    /// Every stdout line: parse_all → reduce under the gate → side effects (§7.1 steps 2-9) → notify/throttled.
    pub(crate) fn on_event(self: &Arc<Self>, raw: Value) {
        todo!("port: LiveSession.OnEvent")
    }
}
