//! Port of the records and pure parts of `Core/MemoryFiles.cs` (/memory equivalent) — spec 05 §3.
//! Owner: core-extensions. File I/O (read, save, delete) is `ccui_core::memory_files`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A memory file is loaded into every turn: above this the textarea is the wrong tool (`Mem.TooLarge`).
pub const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryFile {
    pub path: String,
    /// The CLI's label: User | Project | Local.
    #[serde(rename = "type")]
    pub kind: String,
    /// None = not loaded by the CLI.
    pub tokens: Option<i64>,
    pub exists: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySave {
    Saved,
    Conflict,
    NotAllowed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoMemory {
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    /// user | feedback | project | reference | None.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// The memory panel's open file and unsaved edit. Kept in the UI per-session store (ui/src/state.rs), not in the
/// panel: it unmounts whenever a permission or a selected tool takes the inspector.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryEdit {
    pub open: Option<String>,
    /// File text as read (None = absent): the baseline of the conflict check.
    pub loaded: Option<String>,
    pub draft: Option<String>,
    pub editing: bool,
    pub conflict: bool,
}

/// CLI-loaded files (`get_context_usage.memoryFiles`, absolute paths only, CLI order) then the standard candidates it
/// did not load (user CLAUDE.md; cwd CLAUDE.md / .claude/CLAUDE.md / CLAUDE.local.md, or the AGENTS.md fallback).
/// `exists` is injected (the backend passes the filesystem check). `case_insensitive` = not Linux.
pub fn list(context: Option<&Value>, cwd: &str, user_dir: &str, exists: &dyn Fn(&str) -> bool, case_insensitive: bool) -> Vec<MemoryFile> {
    todo!("port: MemoryFiles.List")
}

/// First `folders[]` with kind "auto" and an absolute path (None while auto-memory is off).
pub fn auto_folder(dialog: &Value) -> Option<String> {
    todo!("port: MemoryFiles.AutoFolder")
}

/// `memories[]` inside the auto folder (lexically normalized containment, `..` rejected), CLI order.
pub fn memories(dialog: &Value, case_insensitive: bool) -> Vec<AutoMemory> {
    todo!("port: MemoryFiles.Memories")
}

/// Display path: relative to cwd, else `~/…` relative to home, else the full path.
pub fn display(path: &str, cwd: &str, home: &str) -> String {
    todo!("port: MemoryFiles.Display")
}

/// Line endings of `text` normalized to `\r\n` when `loaded` used them, else `\n`.
pub fn eol(loaded: Option<&str>, text: &str) -> String {
    todo!("port: MemoryFiles.Eol")
}
