//! Port of the records and pure parts of `Core/MemoryFiles.cs` (/memory equivalent) — spec 05 §3.
//! Owner: core-extensions. File I/O (read, save, delete) is `ccui_core::memory_files`.

use crate::json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Component, Path, PathBuf, MAIN_SEPARATOR};

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

/// `Path.GetFullPath` without the current directory: `.` dropped, `..` resolved lexically, no symlink resolution.
pub fn normalize(path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for c in Path::new(path).components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn key(path: &str, case_insensitive: bool) -> String {
    let s = normalize(path).to_string_lossy().into_owned();
    if case_insensitive {
        s.to_lowercase()
    } else {
        s
    }
}

/// `MemoryFiles.Same`: lexical full-path equality, case-insensitive when asked (not Linux).
pub fn same_path(a: &str, b: &str, case_insensitive: bool) -> bool {
    key(a, case_insensitive) == key(b, case_insensitive)
}

fn join(dir: &str, parts: &[&str]) -> String {
    let mut p = PathBuf::from(dir);
    parts.iter().for_each(|x| p.push(x));
    p.to_string_lossy().into_owned()
}

/// CLI-loaded files (`get_context_usage.memoryFiles`, absolute paths only, CLI order) then the standard candidates it
/// did not load (user CLAUDE.md; cwd CLAUDE.md / .claude/CLAUDE.md / CLAUDE.local.md, or the AGENTS.md fallback).
/// `exists` is injected (the backend passes the filesystem check). `case_insensitive` = not Linux.
// ponytail: paths are compared as written. A cwd reached through a symlink (/tmp vs /private/tmp) can list a standard
// file twice, once loaded and once as its candidate; resolve every component if that ever shows up for real.
pub fn list(context: Option<&Value>, cwd: &str, user_dir: &str, exists: &dyn Fn(&str) -> bool, case_insensitive: bool) -> Vec<MemoryFile> {
    let mut list: Vec<MemoryFile> = Vec::new();
    let has = |list: &[MemoryFile], p: &str| list.iter().any(|f| same_path(&f.path, p, case_insensitive));
    if let Some(Value::Array(a)) = context.and_then(|c| json::prop(c, "memoryFiles")) {
        for m in a {
            if let Some(p) = json::str(m, "path") {
                if Path::new(p).is_absolute() && !has(&list, p) {
                    list.push(MemoryFile { path: p.to_string(), kind: json::string(m, "type"), tokens: json::num(m, "tokens"), exists: exists(p) });
                }
            }
        }
    }
    // A cwd that is not absolute (a session still without one) would resolve against the server's own folder.
    let mut candidates = vec![(join(user_dir, &["CLAUDE.md"]), "User")];
    if Path::new(cwd).is_absolute() {
        // An AGENTS.md-only folder: the CLI reads AGENTS.md when no CLAUDE.md is there (its "agents-fallback"), so
        // creating a CLAUDE.md would silently stop it being read. AGENTS.md is offered instead.
        if exists(&join(cwd, &["AGENTS.md"])) && !exists(&join(cwd, &["CLAUDE.md"])) && !exists(&join(cwd, &[".claude", "CLAUDE.md"])) {
            candidates.push((join(cwd, &["AGENTS.md"]), "Project"));
            candidates.push((join(cwd, &["CLAUDE.local.md"]), "Local"));
        } else {
            candidates.push((join(cwd, &["CLAUDE.md"]), "Project"));
            candidates.push((join(cwd, &[".claude", "CLAUDE.md"]), "Project"));
            candidates.push((join(cwd, &["CLAUDE.local.md"]), "Local"));
        }
    }
    for (p, kind) in candidates {
        if !has(&list, &p) {
            let e = exists(&p);
            list.push(MemoryFile { path: p, kind: kind.to_string(), tokens: None, exists: e });
        }
    }
    list
}

/// First `folders[]` with kind "auto" and an absolute path (None while auto-memory is off).
pub fn auto_folder(dialog: &Value) -> Option<String> {
    let Some(Value::Array(fs)) = json::prop(dialog, "folders") else { return None };
    fs.iter()
        .filter(|f| json::str(f, "kind") == Some("auto"))
        .filter_map(|f| json::str(f, "path"))
        .find(|p| is_absolute(p))
        .map(str::to_string)
}

/// Absolute on any host, by its text: `/…`, `C:\…` / `C:/…`, `\\server\…`. The UI (wasm32-unknown-unknown) runs this:
/// there `std::path::Path::is_absolute` is false for every path.
pub fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/') || p.starts_with("\\\\") || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\'))
}

/// Segments split on both separators, `.` dropped, `..` resolved lexically (never above the root).
fn segments(p: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for c in p.split(['/', '\\']) {
        match c {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// The host's separator as the path writes it (a `\` anywhere = Windows), not the UI target's.
fn sep_of(p: &str) -> char {
    if p.contains('\\') {
        '\\'
    } else {
        '/'
    }
}

/// `memories[]` inside the auto folder (lexically normalized containment, `..` rejected), CLI order.
pub fn memories(dialog: &Value, case_insensitive: bool) -> Vec<AutoMemory> {
    let (Some(folder), Some(Value::Array(a))) = (auto_folder(dialog), json::prop(dialog, "memories")) else { return Vec::new() };
    let root = key(&folder, case_insensitive);
    let root = format!("{}{MAIN_SEPARATOR}", root.trim_end_matches(MAIN_SEPARATOR));
    a.iter()
        .filter_map(|m| {
            let p = json::str(m, "path")?;
            if !Path::new(p).is_absolute() || !key(p, case_insensitive).starts_with(&root) {
                return None;
            }
            Some(AutoMemory {
                name: json::str(m, "name")
                    .map(str::to_string)
                    .unwrap_or_else(|| Path::new(p).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()),
                path: p.to_string(),
                description: json::str(m, "description").map(str::to_string),
                kind: json::str(m, "type").map(str::to_string),
            })
        })
        .collect()
}

/// `Path.GetRelativePath(base, path)` when path lies under base (no `..`): the rest, "." for base itself. String-based:
/// the UI runs it on wasm (see `is_absolute`).
fn under(path: &str, base: &str) -> Option<String> {
    if base.is_empty() || !is_absolute(base) || !is_absolute(path) {
        return None;
    }
    // ponytail: on wasm the host OS is unknown, so case-insensitive there (macOS / Windows); a Linux host could
    // match two folders differing only by case. Pass the host from BootInfo if that ever matters.
    let ci = !cfg!(target_os = "linux");
    let (pc, bc) = (segments(path), segments(base));
    let eq = |x: &&str, y: &&str| if ci { x.to_lowercase() == y.to_lowercase() } else { x == y };
    if pc.len() < bc.len() || !bc.iter().zip(&pc).all(|(x, y)| eq(x, y)) {
        return None;
    }
    let rest = &pc[bc.len()..];
    Some(if rest.is_empty() { ".".into() } else { rest.join(&sep_of(path).to_string()) })
}

/// Display path: relative to cwd, else `~/…` relative to home, else the full path.
pub fn display(path: &str, cwd: &str, home: &str) -> String {
    under(path, cwd).or_else(|| under(path, home).map(|h| format!("~{}{h}", sep_of(path)))).unwrap_or_else(|| path.to_string())
}

/// .NET `string.ReplaceLineEndings(with)`: every CRLF, CR, LF, FF, NEL, LS and PS becomes `with`.
pub fn replace_line_endings(text: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                out.push_str(with);
            }
            '\n' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}' => out.push_str(with),
            c => out.push(c),
        }
    }
    out
}

/// Line endings of `text` normalized to `\r\n` when `loaded` used them, else `\n`.
pub fn eol(loaded: Option<&str>, text: &str) -> String {
    replace_line_endings(text, if loaded.is_some_and(|l| l.contains("\r\n")) { "\r\n" } else { "\n" })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    const ROOT: &str = "/tmp/ccui-mem";
    const HOME: &str = "/tmp/ccui-mem/home";
    const CWD: &str = "/tmp/ccui-mem/repo";
    const USER: &str = "/tmp/ccui-mem/home/.claude";

    fn ctx() -> Value {
        json!({"memoryFiles": [
            {"path": format!("{USER}/CLAUDE.md"), "type": "User", "tokens": 206},
            {"path": format!("{CWD}/CLAUDE.md"), "type": "Project", "tokens": 23},
            {"path": format!("{CWD}/docs/extra.md"), "type": "Project", "tokens": 12},
            {"path": "relative.md", "type": "Project", "tokens": 1},
        ]})
    }

    #[test]
    fn list_files() {
        let c = ctx();
        let l = list(Some(&c), CWD, USER, &|p| p.ends_with("CLAUDE.md"), false);
        let shown: Vec<String> = l.iter().map(|f| display(&f.path, CWD, HOME)).collect();
        assert_eq!(shown, ["~/.claude/CLAUDE.md", "CLAUDE.md", "docs/extra.md", ".claude/CLAUDE.md", "CLAUDE.local.md"], "CLI files first");
        assert!(l[0].kind == "User" && l[0].tokens == Some(206) && l[0].exists);
        assert!(!l[2].exists);
        assert!(l[4].kind == "Local" && l[4].tokens.is_none() && !l[4].exists);
        assert_eq!(list(None, CWD, USER, &|_| false, false).len(), 4, "no context: the standard locations");
        let empty = list(None, "", USER, &|_| false, false);
        assert!(empty.len() == 1 && empty[0].kind == "User");
        assert_eq!(list(Some(&c), "repo", USER, &|_| true, false).len(), 3, "relative cwd: no candidate");
        assert_eq!(display(&format!("{ROOT}/elsewhere.md"), CWD, HOME), format!("{ROOT}/elsewhere.md"));
    }

    #[test]
    fn agents_fallback() {
        let agents = list(None, CWD, USER, &|p| p == format!("{CWD}/AGENTS.md"), false);
        let shown: Vec<String> = agents.iter().map(|f| display(&f.path, CWD, HOME)).collect();
        assert_eq!(shown, ["~/.claude/CLAUDE.md", "AGENTS.md", "CLAUDE.local.md"]);
        assert!(agents[1].kind == "Project" && agents[1].exists);
        let both = list(None, CWD, USER, &|p| p.ends_with("AGENTS.md") || p == format!("{CWD}/CLAUDE.md"), false);
        assert_eq!(both.len(), 4, "AGENTS.md beside CLAUDE.md: the standard list");
    }

    #[test]
    fn auto_memories() {
        let auto = format!("{USER}/projects/-repo/memory/");
        let dlg = json!({
            "files": [],
            "folders": [{"kind": "auto", "path": auto, "label": "Open auto-memory folder", "description": ""}],
            "memories": [
                {"name": "MEMORY.md", "path": format!("{auto}MEMORY.md"), "description": null, "type": null, "modified_ms": 1},
                {"name": "user_role.md", "path": format!("{auto}user_role.md"), "description": "Role", "type": "user", "modified_ms": 2},
                {"name": "evil.md", "path": format!("{ROOT}/evil.md"), "description": null, "type": null, "modified_ms": 3},
                {"name": "up.md", "path": format!("{auto}../up.md"), "description": null, "type": null, "modified_ms": 4},
            ],
            "auto_memory": {"enabled": true, "toggleable": true, "status": "on"},
        });
        let m = memories(&dlg, false);
        assert_eq!(auto_folder(&dlg).as_deref(), Some(auto.as_str()));
        assert!(m.len() == 2 && m[0].name == "MEMORY.md" && m[0].kind.is_none());
        assert!(m[1].name == "user_role.md" && m[1].description.as_deref() == Some("Role") && m[1].kind.as_deref() == Some("user"));
        assert!(memories(&json!({"folders":[],"auto_memory":{"enabled":false}}), false).is_empty());
        assert!(auto_folder(&Value::Null).is_none());
        // a missing name falls back to the file name; case-insensitive containment
        let dlg2 = json!({"folders":[{"kind":"auto","path":"/A/mem"}],"memories":[{"path":"/a/MEM/x.md"}]});
        assert_eq!(memories(&dlg2, true)[0].name, "x.md");
        assert!(memories(&dlg2, false).is_empty());
    }

    #[test]
    fn line_endings_and_paths() {
        assert_eq!(eol(Some("a\r\nb"), "a\nb\nc"), "a\r\nb\r\nc");
        assert_eq!(eol(Some("a\nb"), "x\r\ny"), "x\ny");
        assert_eq!(eol(None, "x\ny"), "x\ny");
        assert_eq!(replace_line_endings("a\r\nb\rc\u{2028}d", " "), "a b c d");
        assert!(same_path("/r/sub/../x.md", "/r/x.md", false));
        assert!(!same_path("/r/sub/../../repo/x.md", "/r/x.md", false));
        assert!(same_path("/R/X.md", "/r/x.md", true) && !same_path("/R/X.md", "/r/x.md", false));
        assert_eq!(display(CWD, CWD, HOME), ".");
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn string_based_on_any_host() {
        assert!(is_absolute("/u/a") && is_absolute("C:\\u") && is_absolute("c:/u") && is_absolute("\\\\srv\\share"));
        assert!(!is_absolute("rel/a") && !is_absolute("C:") && !is_absolute("C:rel") && !is_absolute(""));
        assert_eq!(display("/u/p/CLAUDE.md", "/u/p", "/u"), "CLAUDE.md");
        assert_eq!(display("/u/p/.claude/CLAUDE.md", "/u/p/", "/u"), ".claude/CLAUDE.md");
        assert_eq!(display("/u/.claude/CLAUDE.md", "/u/p", "/u"), "~/.claude/CLAUDE.md");
        assert_eq!(display("/x/CLAUDE.md", "/u/p", "/u"), "/x/CLAUDE.md");
        assert_eq!(display("/u/p/../q/a.md", "/u/q", "/u"), "a.md", ".. resolved lexically");
        assert_eq!(display("C:\\Users\\me\\p\\CLAUDE.local.md", "C:\\Users\\me\\p", "C:\\Users\\me"), "CLAUDE.local.md");
        assert_eq!(display("C:\\Users\\me\\.claude\\CLAUDE.md", "D:\\w", "C:\\Users\\me"), "~\\.claude\\CLAUDE.md");
        let dlg = serde_json::json!({"folders": [{"kind": "auto", "path": "rel"}, {"kind": "auto", "path": "C:\\m"}]});
        assert_eq!(auto_folder(&dlg).as_deref(), Some("C:\\m"));
    }
}
