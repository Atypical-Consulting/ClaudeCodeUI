//! Port of the pure parts of `Core/EditDiff.cs` — spec 04 §7. Owner: core-worktrees-files.
//! No LCS: the edited block is cut to whole lines; common leading/trailing lines become context.
//! `read(path)` and `from_input` (which reads the file) are `ccui_core::edit_diff`.
//! Indices: work on byte offsets of `&str` consistently (only line splits matter).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CONTEXT: usize = 2;
/// A bigger file is diffed without line numbers.
pub const MAX_READ: u64 = 2_000_000;
/// replace_all previews only the first 20 occurrences.
pub const MAX_OCCURRENCES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    Ctx,
    Add,
    Del,
    Hunk,
    More,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffKind,
    /// Old number for Del, new number otherwise; More: occurrences not shown.
    pub n: Option<i32>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub lines: Vec<DiffLine>,
}

/// `\r\n` → `\n`.
pub fn norm(s: &str) -> String {
    todo!("port: EditDiff.Norm")
}

/// [] for empty; else drop one trailing `\n` and split on `\n`.
pub fn lines(s: &str) -> Vec<&str> {
    todo!("port: EditDiff.Lines")
}

/// `Hunk("@@ -0,0 +1,{n} @@")` then `Add(i+1, line)`.
pub fn created(content: &str) -> Vec<DiffLine> {
    todo!("port: EditDiff.Created")
}

/// Hunks for `(old, new)` edits against `file` (None = unreadable/unknown → unnumbered Del/Add); the
/// "already applied" rule rebuilds the pre-edit text so a replay shows the same diff.
pub fn edits(file: Option<&str>, edits: &[(String, String)], replace_all: bool) -> Vec<DiffLine> {
    todo!("port: EditDiff.Edits")
}

/// `from_input` given the file text the backend read (`file`) and whether the path exists.
pub fn from_input_with(tool: &str, input: &Value, file: Option<&str>, exists: bool) -> Vec<DiffLine> {
    todo!("port: EditDiff.FromInput (pure part)")
}

/// `tool_use_result.structuredPatch` hunks.
pub fn from_patch(structured_patch: &Value) -> Vec<DiffLine> {
    todo!("port: EditDiff.FromPatch")
}

/// `tool_use_result.bashEditDiff.files[]`; a file with no hunks gets `Hunk(None, Diff.NoHunks)`.
pub fn from_bash(structured: &Value) -> Vec<FileDiff> {
    todo!("port: EditDiff.FromBash")
}

/// Unified `git diff` output → files (the /diff panel).
pub fn from_git(unified: &str) -> Vec<FileDiff> {
    todo!("port: EditDiff.FromGit")
}

/// highlight.js language by extension (`data-hl`), "" when unknown.
pub fn lang(path: &str) -> &'static str {
    todo!("port: EditDiff.Lang")
}
