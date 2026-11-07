//! Port of `LiveSession.Args(resume)` — exact argv order, spec 01 §4. Owner: core-live-session.
//! Pure (the Thinking app flag is passed in) so the Self-check expectations port as unit tests.

use super::state::SessionState;

/// `[--permission-mode M ("default" → "manual")] --include-partial-messages --forward-subagent-text
/// --allowedTools=… --include-hook-events --prompt-suggestions true` then resume / fork / fresh, then model,
/// effort, fallback, add-dirs, restricted, safe-mode, agent, `--thinking-display summarized` (iff thinking_flag).
pub fn args(s: &SessionState, resume: bool, thinking_flag: bool) -> Vec<String> {
    todo!("port: LiveSession.Args")
}
