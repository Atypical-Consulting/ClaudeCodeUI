//! Port of the filesystem parts of `Core/PermissionRules.cs` (`FileOf`, `Save`) — spec 02 §9. Owner: core-extensions.
//! The CLI lists rules but writes none over stream-json: edit the settings file it watches (reloaded ~3 s later).

use crate::{git, paths, CoreResult};
use ccui_shared::{permission_rules, t};
use std::path::{Path, PathBuf};

/// userSettings → `{config_dir}/settings.json`; projectSettings → `{cwd}/.claude/settings.json`; localSettings →
/// `{root}/.claude/settings.local.json` (root = git_root unless empty or the home folder, else cwd); else None.
pub fn file_of(source: &str, cwd: &str, git_root: Option<&str>, config_dir: &str) -> Option<PathBuf> {
    file_of_in(source, cwd, git_root, config_dir, &|| paths::home().to_string_lossy().into_owned())
}

fn file_of_in(source: &str, cwd: &str, git_root: Option<&str>, config_dir: &str, home: &dyn Fn() -> String) -> Option<PathBuf> {
    match source {
        "userSettings" => Some(Path::new(config_dir).join("settings.json")),
        "projectSettings" => Some(Path::new(cwd).join(".claude").join("settings.json")),
        "localSettings" => {
            let root = git_root.filter(|g| !g.is_empty() && *g != home()).unwrap_or(cwd);
            Some(Path::new(root).join(".claude").join("settings.local.json"))
        }
        _ => None,
    }
}

/// Read if exists, `permission_rules::edit`, create parent dirs, write (temp + rename is fine). Errors propagate.
pub fn save(path: &PathBuf, behavior: &str, rule: &str, add: bool) -> CoreResult<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let next = permission_rules::edit(text.as_deref(), behavior, rule, add)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, next).map_err(|e| e.to_string())
}

/// Whole Extensions flow for one rule: validate, resolve the file (git root for localSettings), save.
/// Err(`Ext.Perm.Invalid(rule)`) / Err(`Ext.Perm.WriteFailed…`).
pub async fn edit_rule(cwd: &str, source: &str, behavior: &str, rule: &str, add: bool) -> CoreResult<()> {
    if !permission_rules::valid(rule) {
        return Err(t!("Ext.Perm.Invalid", rule));
    }
    let root = if source == "localSettings" { git::root(cwd).await } else { None };
    let config = paths::config_dir();
    let path = file_of(source, cwd, root.as_deref(), &config.to_string_lossy()).ok_or_else(|| t!("Ext.Perm.WriteFailed", source))?;
    save(&path, behavior, rule, add).map_err(|e| t!("Ext.Perm.WriteFailed", e.trim_end()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn file_locations() {
        let home = || "/h".to_string();
        let f = |s, cwd, g, c| file_of_in(s, cwd, g, c, &home);
        assert_eq!(f("localSettings", "/r/.claude/worktrees/x", Some("/r"), "/c"), Some(PathBuf::from("/r/.claude/settings.local.json")));
        assert_eq!(f("projectSettings", "/r/.claude/worktrees/x", Some("/r"), "/c"), Some(PathBuf::from("/r/.claude/worktrees/x/.claude/settings.json")));
        assert_eq!(f("localSettings", "/t", None, "/c"), Some(PathBuf::from("/t/.claude/settings.local.json")));
        assert_eq!(f("localSettings", "/t", Some("/h"), "/c"), Some(PathBuf::from("/t/.claude/settings.local.json")), "home as root: cwd");
        assert_eq!(f("localSettings", "/t", Some(""), "/c"), Some(PathBuf::from("/t/.claude/settings.local.json")));
        assert_eq!(f("userSettings", "/t", None, "/c"), Some(PathBuf::from("/c/settings.json")));
        assert_eq!(f("policySettings", "/t", None, "/c"), None);
    }

    #[test]
    fn save_creates_and_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/.claude/settings.local.json");
        save(&path, "allow", "Bash(ls:*)", true).unwrap();
        save(&path, "deny", "Read(./.env)", true).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls:*)");
        assert_eq!(v["permissions"]["deny"][0], "Read(./.env)");
        save(&path, "allow", "Bash(ls:*)", false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["permissions"]["allow"].as_array().unwrap().len(), 0);
        std::fs::write(&path, "{ // comment\n}").unwrap();
        assert!(save(&path, "allow", "Bash", true).is_err(), "a file the CLI would skip is never rewritten");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ // comment\n}");
    }
}
