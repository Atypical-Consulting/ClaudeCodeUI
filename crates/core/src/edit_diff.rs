//! Port of the I/O parts of `Core/EditDiff.cs` (`Read`, `FromInput`) and the /diff panel's git read — spec 04 §7,
//! spec 07 §17 (DiffPanel). Owner: core-worktrees-files. Pure diffing: `ccui_shared::edit_diff`.

use ccui_shared::api::GitDiff;
use ccui_shared::edit_diff::DiffLine;
use serde_json::Value;

/// File text for diffing: symlink → final target (non-file → None); missing or > MAX_READ → None; length 0 → ""
/// WITHOUT opening (devices, FIFOs); UTF-8, `\r\n` normalized; any error → None.
pub fn read(path: &str) -> Option<String> {
    todo!("port: EditDiff.Read")
}

/// Diff of an Edit / Write / MultiEdit input before approval (reads the file).
pub fn from_input(tool: &str, input: &Value) -> Vec<DiffLine> {
    todo!("port: EditDiff.FromInput")
}

/// Uncommitted changes of the session folder (`git diff HEAD` incl. untracked as new files), parsed by `from_git`.
pub async fn git_diff(cwd: &str) -> GitDiff {
    todo!("port: DiffPanel.razor load")
}
