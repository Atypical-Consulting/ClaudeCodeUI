//! Port of the pure parts of `Core/FileIndex.cs` ("@" mentions in the Composer) — spec 04 §3.3.
//! Owner: core-worktrees-files. The scan (git ls-files / walk) and `inside` are `ccui_core::file_index`.
//! The UI fetches the index once per cwd (`file_index_scan`) and runs these on each keystroke.

pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_RESULTS: usize = 10;

/// The live `@` query: after the last '@' preceded by whitespace (or at start), no whitespace or '"' in it.
/// `"look at @src/Fo"` → `"src/Fo"`, `"@"` → `""`, `"mail me@host.com"` → None.
pub fn query(text: &str) -> Option<String> {
    todo!("port: FileIndex.Query")
}

/// Replace from the last '@' with `@path ` (or `@"path" ` when it has whitespace); no '@' → unchanged.
pub fn insert(text: &str, path: &str) -> String {
    todo!("port: FileIndex.Insert")
}

/// Last segment, keeping a folder's trailing '/': `"src/Bar/"` → `"Bar/"`.
pub fn name(path: &str) -> &str {
    todo!("port: FileIndex.Name")
}

/// 0 name starts with q, 1 name contains, 2 path contains, 3 subsequence, -1 none (case-insensitive).
pub fn score(path: &str, q: &str) -> i32 {
    todo!("port: FileIndex.Score")
}

/// Empty query: by depth then case-insensitively; else score ≥ 0 by (score, len, path). Take `max`.
pub fn filter(paths: &[String], query: &str, max: usize) -> Vec<String> {
    todo!("port: FileIndex.Filter")
}
