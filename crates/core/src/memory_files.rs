//! Port of the file I/O of `Core/MemoryFiles.cs` — spec 05 §3. Owner: core-extensions.
//! Only paths of the session's known list may be read/written/deleted (recomputed here from the session state,
//! never trusted from the UI). Writes are IN PLACE (CLAUDE.md is often a symlink to AGENTS.md), BOM kept,
//! line endings follow the loaded text.

use crate::live::LiveSession;
use crate::CoreResult;
use ccui_shared::memory::{AutoMemory, MemoryFile, MemorySave};

/// `memory::list(context, cwd, config_dir, exists, case_insensitive)` for the session.
pub fn list(session: &LiveSession) -> Vec<MemoryFile> {
    todo!("port: MemoryFiles.List (session call site)")
}

/// `memory::memories(memory_dialog)` for the session.
pub fn autos(session: &LiveSession) -> Vec<AutoMemory> {
    todo!("port: MemoryFiles.Memories (session call site)")
}

/// None if missing; Err(`Mem.TooLarge`) above 256 KiB; UTF-8 with BOM stripped.
pub fn read(path: &str) -> CoreResult<Option<String>> {
    todo!("port: MemoryFiles.Read")
}

/// NotAllowed (not a known file), Conflict (disk ≠ loaded), else Saved.
pub fn save(known: &[MemoryFile], path: &str, loaded: Option<&str>, text: &str) -> CoreResult<MemorySave> {
    todo!("port: MemoryFiles.Save")
}

/// Delete a known auto-memory file; false when not allowed.
pub fn delete(known: &[AutoMemory], path: &str) -> CoreResult<bool> {
    todo!("port: MemoryFiles.Delete")
}

/// Lexical full-path equality; ordinal on Linux, case-insensitive on Windows/macOS.
pub fn same(a: &str, b: &str) -> bool {
    todo!("port: MemoryFiles.Same")
}
