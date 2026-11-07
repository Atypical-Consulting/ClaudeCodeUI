//! Port of the file I/O of `Core/MemoryFiles.cs` — spec 05 §3. Owner: core-extensions.
//! Only paths of the session's known list may be read/written/deleted (recomputed here from the session state,
//! never trusted from the UI). Writes are IN PLACE (CLAUDE.md is often a symlink to AGENTS.md), BOM kept,
//! line endings follow the loaded text.

use crate::live::LiveSession;
use crate::{paths, CoreResult};
use ccui_shared::memory::{self, AutoMemory, MemoryFile, MemorySave, MAX_BYTES};
use ccui_shared::{fmt, t};
use std::path::Path;

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Ordinal on Linux, case-insensitive on Windows/macOS.
const CASE_INSENSITIVE: bool = !cfg!(target_os = "linux");

/// `memory::list(context, cwd, config_dir, exists, case_insensitive)` for the session.
pub fn list(session: &LiveSession) -> Vec<MemoryFile> {
    let (context, cwd) = session.with_state(|s| (s.heavy.context.clone(), s.cwd.clone()));
    let user_dir = paths::config_dir().to_string_lossy().into_owned();
    memory::list(context.as_ref(), &cwd, &user_dir, &|p| Path::new(p).is_file(), CASE_INSENSITIVE)
}

/// `memory::memories(memory_dialog)` for the session.
pub fn autos(session: &LiveSession) -> Vec<AutoMemory> {
    session.with_state(|s| s.heavy.memory_dialog.as_ref().map(|d| memory::memories(d, CASE_INSENSITIVE)).unwrap_or_default())
}

/// Known = `list` ∪ `autos` as `MemoryFile { path, kind: "Auto", tokens: None, exists: true }` (spec 07 §34): the
/// only paths `memory_read` / `memory_save` accept.
pub fn known(session: &LiveSession) -> Vec<MemoryFile> {
    let mut files = list(session);
    files.extend(autos(session).into_iter().map(|m| MemoryFile { path: m.path, kind: "Auto".into(), tokens: None, exists: true }));
    files
}

/// None if missing; Err(`Mem.TooLarge`) above 256 KiB; UTF-8 with BOM stripped.
pub fn read(path: &str) -> CoreResult<Option<String>> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => {
            if m.len() > MAX_BYTES {
                return Err(t!("Mem.TooLarge", fmt::size(MAX_BYTES as i64)));
            }
        }
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(Some(String::from_utf8_lossy(bytes.strip_prefix(BOM).unwrap_or(&bytes)).into_owned()))
}

/// NotAllowed (not a known file), Conflict (disk ≠ loaded), else Saved.
pub fn save(known: &[MemoryFile], path: &str, loaded: Option<&str>, text: &str) -> CoreResult<MemorySave> {
    if !known.iter().any(|f| same(&f.path, path)) {
        return Ok(MemorySave::NotAllowed);
    }
    if read(path)?.as_deref() != loaded {
        return Ok(MemorySave::Conflict);
    }
    let bom = loaded.is_some() && std::fs::read(path).map(|b| b.starts_with(BOM)).unwrap_or(false);
    if let Some(dir) = Path::new(path).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut bytes = if bom { BOM.to_vec() } else { Vec::new() };
    bytes.extend_from_slice(memory::eol(loaded, text).as_bytes());
    // In place (fs::write follows a symlink), never write-then-rename: a rename would replace the link.
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    Ok(MemorySave::Saved)
}

/// Delete a known auto-memory file; false when not allowed.
pub fn delete(known: &[AutoMemory], path: &str) -> CoreResult<bool> {
    if !known.iter().any(|m| same(&m.path, path)) {
        return Ok(false);
    }
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(true),
    }
}

/// Lexical full-path equality; ordinal on Linux, case-insensitive on Windows/macOS.
pub fn same(a: &str, b: &str) -> bool {
    memory::same_path(a, b, CASE_INSENSITIVE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> MemoryFile {
        MemoryFile { path: path.into(), kind: "Project".into(), tokens: None, exists: false }
    }

    #[test]
    fn save_read_delete() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("repo");
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let md = s(&cwd.join("CLAUDE.md"));
        let list = vec![file(&md)];
        assert_eq!(save(&list, &s(&cwd.join("evil.md")), None, "x").unwrap(), MemorySave::NotAllowed);
        assert!(!cwd.join("evil.md").exists(), "unknown path refused");
        assert_eq!(save(&list, &s(&cwd.join("sub/../../repo/x.md")), None, "x").unwrap(), MemorySave::NotAllowed, "dot-dot refused");
        assert_eq!(save(&list, &md, None, "one\n").unwrap(), MemorySave::Saved, "create a missing file (folder included)");
        assert_eq!(read(&md).unwrap().as_deref(), Some("one\n"));
        assert_eq!(save(&list, &md, None, "two\n").unwrap(), MemorySave::Conflict, "created meanwhile");
        assert_eq!(read(&md).unwrap().as_deref(), Some("one\n"));
        std::fs::write(&md, "changed on disk\n").unwrap();
        assert_eq!(save(&list, &md, Some("one\n"), "mine\n").unwrap(), MemorySave::Conflict, "changed since load");
        assert_eq!(read(&md).unwrap().as_deref(), Some("changed on disk\n"));
        std::fs::write(&md, b"\xEF\xBB\xBFa\r\nb\r\n").unwrap();
        let loaded = read(&md).unwrap();
        assert_eq!(loaded.as_deref(), Some("a\r\nb\r\n"), "BOM stripped on read");
        assert_eq!(save(&list, &md, loaded.as_deref(), "a\nb\nc\n").unwrap(), MemorySave::Saved);
        assert_eq!(std::fs::read(&md).unwrap(), b"\xEF\xBB\xBFa\r\nb\r\nc\r\n", "BOM and CRLF kept");
        assert_eq!(read(&s(&cwd.join("none.md"))).unwrap(), None);
        std::fs::write(&md, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        assert!(read(&md).is_err(), "too large");

        let auto = cwd.join("mem");
        std::fs::create_dir_all(&auto).unwrap();
        let role = s(&auto.join("user_role.md"));
        std::fs::write(&role, "x").unwrap();
        let mems = vec![AutoMemory { name: "user_role.md".into(), path: role.clone(), description: None, kind: None }];
        assert!(!delete(&mems, &md).unwrap() && Path::new(&md).exists());
        assert!(delete(&mems, &role).unwrap() && !Path::new(&role).exists(), "delete a listed memory only");
    }

    #[cfg(unix)]
    #[test]
    fn save_writes_through_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let agents = root.path().join("AGENTS.md");
        let link = root.path().join("CLAUDE.md");
        std::fs::write(&agents, "a\n").unwrap();
        std::os::unix::fs::symlink(&agents, &link).unwrap();
        let l = link.to_string_lossy().into_owned();
        assert_eq!(save(&[file(&l)], &l, Some("a\n"), "b\n").unwrap(), MemorySave::Saved);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&agents).unwrap(), "b\n");
    }
}
