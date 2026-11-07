//! Port of the scan of `Core/FileIndex.cs` ("@" mentions) — spec 04 §3.1-3.2. Owner: core-worktrees-files.
//! git ls-files first (tracked + untracked-not-ignored), else a breadth-first walk (links listed, never followed,
//! SKIPPED dirs dropped), 20 000 entries, 5 s budget. Blocking work: the command runs it in spawn_blocking.

use ccui_shared::api::FileIndexResult;
use ccui_shared::file_index::MAX_ENTRIES;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Folders the walk never enters (case-insensitive).
pub const SKIPPED: [&str; 4] = ["bin", "obj", "node_modules", ".git"];

const BUDGET: Duration = Duration::from_secs(5);

/// Cancel flags of running scans by request id (`file_index_cancel`).
#[derive(Default)]
pub struct ScanRegistry {
    pub running: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

/// Index of `cwd` (paths relative, '/'-separated, folders end with '/'). `cancel` aborts (partial, truncated).
pub fn scan(cwd: &str, cancel: &AtomicBool) -> FileIndexResult {
    let deadline = Instant::now() + BUDGET;
    let stop = || cancel.load(Ordering::SeqCst) || Instant::now() >= deadline;
    let mut set = HashSet::new();
    let truncated = match git(cwd, &mut set, &stop) {
        Some(t) => t,
        None => walk(cwd, &mut set, &stop),
    };
    FileIndexResult { paths: set.into_iter().filter(|p| inside(cwd, p)).collect(), truncated: truncated || stop() }
}

/// None = not a git repo (or no git): the caller walks instead.
fn git(cwd: &str, set: &mut HashSet<String>, stop: &(dyn Fn() -> bool + Sync)) -> Option<bool> {
    let mut cmd = Command::new("git");
    cmd.args(["-C", cwd, "-c", "core.quotePath=off", "ls-files", "--cached", "--others", "--exclude-standard"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let child = Arc::new(Mutex::new(child));
    // Watchdog: a git that hangs past the budget (or a cancel) is killed, which ends the read below.
    let done = Arc::new(AtomicBool::new(false));
    let fired = Arc::new(AtomicBool::new(false));
    std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                if stop() {
                    let mut c = child.lock();
                    if !done.load(Ordering::SeqCst) {
                        fired.store(true, Ordering::SeqCst);
                        let _ = c.kill();
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let mut truncated = false;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line.starts_with('"') {
                continue; // still C-quoted (newline, tab, quote in the name): not insertable
            }
            if set.len() >= MAX_ENTRIES {
                truncated = true;
                break;
            }
            // parent folders, once each
            let mut end = line.len();
            while let Some(i) = line[..end].rfind('/') {
                if i == 0 || !set.insert(line[..=i].to_string()) {
                    break;
                }
                end = i;
            }
            set.insert(line);
        }
        if truncated {
            let _ = child.lock().kill();
        }
        let status = {
            let mut c = child.lock();
            let st = c.wait();
            done.store(true, Ordering::SeqCst);
            st
        };
        if truncated || fired.load(Ordering::SeqCst) {
            return Some(true);
        }
        match status {
            Ok(st) if st.success() => Some(false),
            _ if !set.is_empty() => Some(false),
            _ => None,
        }
    })
}

/// Breadth first, so a cut keeps the shallow entries. Skips SKIPPED folders and never follows a link.
fn walk(cwd: &str, set: &mut HashSet<String>, stop: &(dyn Fn() -> bool + Sync)) -> bool {
    let mut queue = VecDeque::from([String::new()]);
    while let Some(rel) = queue.pop_front() {
        if stop() {
            return true;
        }
        let Ok(rd) = std::fs::read_dir(Path::new(cwd).join(&rel)) else { continue };
        let mut entries: Vec<_> = rd.flatten().take(MAX_ENTRIES).collect(); // a huge folder is cut before the sort
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if set.len() >= MAX_ENTRIES {
                return true;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.chars().any(char::is_control) {
                continue; // tab, newline: not insertable, like git's C-quoted names
            }
            let Ok(ft) = e.file_type() else { continue };
            let link = ft.is_symlink();
            let dir = ft.is_dir() || link && e.path().is_dir();
            if dir {
                if SKIPPED.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                let d = format!("{rel}{name}/");
                set.insert(d.clone());
                if !link {
                    queue.push_back(d);
                }
            } else {
                set.insert(format!("{rel}{name}"));
            }
        }
    }
    false
}

/// Lexical containment (no symlink resolution): false for empty, quoted, rooted or escaping `rel`.
/// Case-insensitive on Windows, ordinal elsewhere.
pub fn inside(cwd: &str, rel: &str) -> bool {
    if rel.is_empty() || rel.contains('"') || rel.starts_with(['/', '\\']) || Path::new(rel).has_root() || Path::new(rel).is_absolute() {
        return false;
    }
    let root = crate::worktrees::norm(cwd);
    let root = format!("{}/", root.trim_end_matches('/'));
    let full = crate::worktrees::norm(&format!("{root}{rel}"));
    if cfg!(windows) {
        full.to_lowercase().starts_with(&root.to_lowercase())
    } else {
        full.starts_with(&root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_cases() {
        let cwd = std::env::temp_dir().join("ccui-cwd").to_string_lossy().into_owned();
        assert!(inside(&cwd, "src/a.cs") && inside(&cwd, "src/../a.cs") && inside(&cwd, "src/"));
        assert!(!inside(&cwd, "../etc/passwd") && !inside(&cwd, "src/../../x") && !inside(&cwd, "/etc/passwd") && !inside(&cwd, ""));
        assert!(!inside(&cwd, "../ccui-cwd-evil/x"));
        assert!(!inside(&cwd, "a\"b"));
    }

    #[test]
    fn walk_then_git() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut files = vec!["src/a b.cs", "node_modules/x.js", "bin/y.dll", "out/z.log", ".gitignore"];
        if cfg!(unix) {
            files.push("src/t\tab.cs"); // not a legal file-name character on Windows
        }
        for f in files {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, if f == ".gitignore" { "out/\n" } else { "" }).unwrap();
        }
        let d = dir.to_string_lossy().into_owned();
        let none = AtomicBool::new(false);
        let walked = scan(&d, &none);
        let w = &walked.paths;
        assert!(!walked.truncated);
        assert!(w.contains(&"src/".into()) && w.contains(&"src/a b.cs".into()) && w.contains(&"out/z.log".into()), "{w:?}");
        assert!(!w.iter().any(|p| p.starts_with("node_modules") || p.starts_with("bin") || p.contains('\t')), "{w:?}");
        crate::git::tests::g(dir, &["init", "-q"]);
        let listed = scan(&d, &none).paths;
        assert!(listed.contains(&"src/".into()) && listed.contains(&"src/a b.cs".into()) && listed.contains(&"node_modules/x.js".into()), "{listed:?}");
        assert!(!listed.iter().any(|p| p.starts_with("out") || p.contains('\t')), "{listed:?}");
        // A cancelled scan is partial and says so.
        assert!(scan(&d, &AtomicBool::new(true)).truncated);
    }
}
