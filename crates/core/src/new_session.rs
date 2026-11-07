//! Backend helpers of the NewSession page (`Components/Pages/NewSession.razor` code-behind) — spec 06 §6.1.
//! Owner: core-worktrees-files.

use crate::session_manager::SessionManager;
use ccui_shared::api::ClaudeFound;

/// `ClaudeSession.OnPath()` + `NpmShimOnly()`.
pub fn claude_found() -> ClaudeFound {
    todo!("port: NewSession claudeFound")
}

/// The 5 most recent transcript roots (`RootOf(cwd)`, distinct case-insensitive, existing dirs).
pub fn recent_roots() -> Vec<String> {
    todo!("port: NewSession recents")
}

/// `Local(d)`: fully-qualified, not UNC (`\\`, `//` — Windows would authenticate to its host), existing directory.
pub fn local_dir(path: &str) -> bool {
    todo!("port: NewSession.Local")
}

/// Process current dir (the default cwd when there is no recent).
pub fn default_cwd() -> String {
    todo!("port: Environment.CurrentDirectory")
}

/// Agent names: every live session's agents ∪ `*.md` stems of `<dir>/.claude/agents` and `~/.claude/agents`;
/// distinct, sorted; IO errors → [].
pub fn agent_names(dir: &str, sessions: &SessionManager) -> Vec<String> {
    todo!("port: NewSession.AgentNames")
}

/// Generated worktree name made free: `-2`, `-3`… while `<root>/.claude/worktrees/<n>` exists or a live session uses it.
pub async fn free_worktree_name(cwd: &str, name: &str, sessions: &SessionManager) -> String {
    todo!("port: NewSession.Free")
}
