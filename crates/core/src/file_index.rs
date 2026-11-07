//! Port of the scan of `Core/FileIndex.cs` ("@" mentions) — spec 04 §3.1-3.2. Owner: core-worktrees-files.
//! git ls-files first (tracked + untracked-not-ignored), else a breadth-first walk (links listed, never followed,
//! SKIPPED dirs dropped), 20 000 entries, 5 s budget. Blocking work: the command runs it in spawn_blocking.

use ccui_shared::api::FileIndexResult;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Folders the walk never enters (case-insensitive).
pub const SKIPPED: [&str; 4] = ["bin", "obj", "node_modules", ".git"];

/// Cancel flags of running scans by request id (`file_index_cancel`).
#[derive(Default)]
pub struct ScanRegistry {
    pub running: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

/// Index of `cwd` (paths relative, '/'-separated, folders end with '/'). `cancel` aborts (partial, truncated).
pub fn scan(cwd: &str, cancel: &AtomicBool) -> FileIndexResult {
    todo!("port: FileIndex.ScanAsync")
}

/// Lexical containment (no symlink resolution): false for empty, quoted, rooted or escaping `rel`.
/// Case-insensitive on Windows, ordinal elsewhere.
pub fn inside(cwd: &str, rel: &str) -> bool {
    todo!("port: FileIndex.Inside")
}
