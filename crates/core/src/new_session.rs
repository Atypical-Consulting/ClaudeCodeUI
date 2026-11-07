//! Backend helpers of the NewSession page (`Components/Pages/NewSession.razor` code-behind) — spec 06 §6.1.
//! Owner: core-worktrees-files.

use crate::session_manager::SessionManager;
use ccui_shared::api::ClaudeFound;
use std::path::{Path, PathBuf};

/// `ClaudeSession.OnPath()` + `NpmShimOnly()`.
pub fn claude_found() -> ClaudeFound {
    use crate::process::ClaudeProcess;
    ClaudeFound { found: ClaudeProcess::on_path(), npm_shim_only: ClaudeProcess::npm_shim_only() }
}

/// The 5 most recent transcript roots (`RootOf(cwd)`, distinct case-insensitive, existing dirs).
pub fn recent_roots() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in crate::transcript_store::recent(30) {
        let r = ccui_shared::text::root_of(&p.cwd);
        if !out.iter().any(|x| x.to_lowercase() == r.to_lowercase()) && Path::new(&r).is_dir() {
            out.push(r);
            if out.len() == 5 {
                break;
            }
        }
    }
    out
}

/// `Local(d)`: fully-qualified, not UNC (`\\`, `//` — Windows would authenticate to its host), existing directory.
pub fn local_dir(path: &str) -> bool {
    Path::new(path).is_absolute() && !path.starts_with("\\\\") && !path.starts_with("//") && Path::new(path).is_dir()
}

/// `validate_extra_dir` (spec 07 §20, ExtraDirs.Add checks — they need the filesystem and the OS rules):
/// `session_rules::extra_dir(raw, cwd, dirs, case_sensitive = Linux)` else Err(`Dirs.Known`); `!local_dir(d)` →
/// Err(`Dirs.NoFolder`); `d` is its own path root (`/`, `C:\`) → Err(`Dirs.Root`). Errors are localized messages.
pub fn validate_extra_dir(raw: &str, cwd: &str, dirs: &[String]) -> Result<String, String> {
    let d = ccui_shared::session_rules::extra_dir(raw, cwd, dirs, cfg!(target_os = "linux"))
        .ok_or_else(|| ccui_shared::t!("Dirs.Known"))?;
    check_local(d)
}

/// The filesystem half of `validate_extra_dir`.
fn check_local(d: String) -> Result<String, String> {
    if !local_dir(&d) {
        return Err(ccui_shared::t!("Dirs.NoFolder"));
    }
    if Path::new(&d).parent().is_none() {
        return Err(ccui_shared::t!("Dirs.Root")); // the whole drive is never a working folder
    }
    Ok(d)
}

/// The default cwd when there is no recent: the home dir (the C# sidecar ran there; a Finder/Dock launch has cwd `/`).
pub fn default_cwd() -> String {
    crate::paths::home().to_string_lossy().into_owned()
}

/// Agent names: every live session's agents ∪ `*.md` stems of `<dir>/.claude/agents` and `~/.claude/agents`;
/// distinct, sorted; IO errors → [].
pub fn agent_names(dir: &str, sessions: &SessionManager) -> Vec<String> {
    let mut names: Vec<String> = sessions
        .all()
        .iter()
        .flat_map(|s| {
            s.with_state(|st| {
                match st.heavy.initialize_info.as_ref().and_then(|i| i.get("agents")).and_then(|a| a.as_array()) {
                    Some(a) => a.iter().map(|x| ccui_shared::json::string(x, "name")).collect::<Vec<_>>(),
                    None => st.heavy.init.as_ref().map(|i| i.agents.clone()).unwrap_or_default(),
                }
            })
        })
        .collect();
    match agent_files(&[PathBuf::from(dir), dirs::home_dir().unwrap_or_default()]) {
        Some(f) => names.extend(f),
        None => return Vec::new(),
    }
    names.retain(|n| !n.is_empty());
    names.sort();
    names.dedup();
    names
}

/// `*.md` stems of `<d>/.claude/agents` for each d (ponytail: the CLI keys on the frontmatter name, which nearly
/// always matches the file name). None on an IO error.
fn agent_files(dirs: &[PathBuf]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for d in dirs.iter().map(|d| d.join(".claude").join("agents")).filter(|d| d.is_dir()) {
        for e in std::fs::read_dir(&d).ok()? {
            let p = e.ok()?.path();
            if p.is_file() && p.extension().is_some_and(|x| x == "md") {
                out.push(p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            }
        }
    }
    Some(out)
}

/// Generated worktree name made free: `-2`, `-3`… while `<root>/.claude/worktrees/<n>` exists or a live session uses it.
pub async fn free_worktree_name(cwd: &str, name: &str, sessions: &SessionManager) -> String {
    let used: Vec<String> = sessions.all().iter().filter_map(|s| s.with_state(|st| st.worktree.clone())).collect();
    free_in(cwd, name, &used).await
}

async fn free_in(cwd: &str, name: &str, used: &[String]) -> String {
    let root = crate::git::output(cwd, &["rev-parse", "--show-toplevel"])
        .await
        .map(|r| r.trim().to_string())
        .unwrap_or_else(|| cwd.to_string());
    let taken = |n: &str| Path::new(&root).join(".claude").join("worktrees").join(n).is_dir() || used.iter().any(|u| u == n);
    let mut free = name.to_string();
    let mut i = 2;
    while taken(&free) {
        free = format!("{name}-{i}");
        i += 1;
    }
    free
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_agents_free() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let ds = d.to_string_lossy().into_owned();
        assert!(local_dir(&ds) && !local_dir("relative") && !local_dir("//host/share") && !local_dir(&format!("{ds}/nope")));
        assert_eq!(check_local(ds.clone()), Ok(ds.clone()));
        assert_eq!(check_local("/".into()), Err(ccui_shared::t!("Dirs.Root")));
        assert_eq!(check_local(format!("{ds}/nope")), Err(ccui_shared::t!("Dirs.NoFolder")));

        std::fs::create_dir_all(d.join(".claude/agents")).unwrap();
        std::fs::write(d.join(".claude/agents/reviewer.md"), "x").unwrap();
        std::fs::write(d.join(".claude/agents/notes.txt"), "x").unwrap();
        assert_eq!(agent_files(&[d.to_path_buf(), d.join("none")]), Some(vec!["reviewer".to_string()]));

        std::fs::create_dir_all(d.join(".claude/worktrees/brave-cat")).unwrap();
        assert_eq!(free_in(&ds, "calm-dog", &[]).await, "calm-dog");
        assert_eq!(free_in(&ds, "brave-cat", &[]).await, "brave-cat-2");
        assert_eq!(free_in(&ds, "calm-dog", &["calm-dog".into(), "calm-dog-2".into()]).await, "calm-dog-3");
        assert_eq!(std::path::PathBuf::from(default_cwd()), crate::paths::home(), "home, never the process cwd");
    }
}
