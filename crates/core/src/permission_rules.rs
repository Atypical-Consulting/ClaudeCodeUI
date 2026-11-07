//! Port of the filesystem parts of `Core/PermissionRules.cs` (`FileOf`, `Save`) — spec 02 §9. Owner: core-extensions.
//! The CLI lists rules but writes none over stream-json: edit the settings file it watches (reloaded ~3 s later).

use crate::CoreResult;
use std::path::PathBuf;

/// userSettings → `{config_dir}/settings.json`; projectSettings → `{cwd}/.claude/settings.json`; localSettings →
/// `{root}/.claude/settings.local.json` (root = git_root unless empty or the home folder, else cwd); else None.
pub fn file_of(source: &str, cwd: &str, git_root: Option<&str>, config_dir: &str) -> Option<PathBuf> {
    todo!("port: PermissionRules.FileOf")
}

/// Read if exists, `permission_rules::edit`, create parent dirs, write (temp + rename is fine). Errors propagate.
pub fn save(path: &PathBuf, behavior: &str, rule: &str, add: bool) -> CoreResult<()> {
    todo!("port: PermissionRules.Save")
}

/// Whole Extensions flow for one rule: validate, resolve the file (git root for localSettings), save.
/// Err(`Ext.Perm.Invalid(rule)`) / Err(`Ext.Perm.WriteFailed…`).
pub async fn edit_rule(cwd: &str, source: &str, behavior: &str, rule: &str, add: bool) -> CoreResult<()> {
    todo!("port: Extensions.EditRule")
}
