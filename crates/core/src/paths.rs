//! Paths shared by every area — spec 03 §0, 04 §1.4. Owner: core-store-sessions.
//! KEEP the C# locations so existing users keep their state: app data = `dirs::data_local_dir()/ClaudeCodeUI`
//! (`%LOCALAPPDATA%`, `~/Library/Application Support`, `$XDG_DATA_HOME` or `~/.local/share`) — never Tauri's
//! bundle-id `app_data_dir()`.

use std::path::PathBuf;

/// `$CLAUDE_CONFIG_DIR` if set and non-empty, else `~/.claude` (re-evaluated on each call).
pub fn config_dir() -> PathBuf {
    todo!("port: TranscriptStore.ConfigDir")
}

/// `config_dir()/projects` (`TranscriptStore.Root`).
pub fn projects_root() -> PathBuf {
    todo!("port: TranscriptStore.Root")
}

/// `dirs::data_local_dir()/ClaudeCodeUI` (`AppFlag` / ledgers / repos.json / bookmarks / limits).
pub fn app_data() -> PathBuf {
    todo!("port: Environment.SpecialFolder.LocalApplicationData + ClaudeCodeUI")
}

/// User home (Windows: USERPROFILE).
pub fn home() -> PathBuf {
    todo!("port: Environment.SpecialFolder.UserProfile")
}

/// `~/.claude/jobs` — NB the C# used the profile folder, ignoring CLAUDE_CONFIG_DIR (kept for parity).
pub fn jobs_dir() -> PathBuf {
    todo!("port: BackgroundAgents jobs folder")
}

/// `~/Downloads` if it exists, else home (export).
pub fn downloads() -> PathBuf {
    todo!("port: LiveSession.Export folder")
}
