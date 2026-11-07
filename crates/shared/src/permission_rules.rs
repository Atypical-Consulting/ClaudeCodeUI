//! Port of `Core/PermissionRules.cs` (the /permissions equivalent), pure parts — spec 02 §9. Owner: shared-pure-a.
//! `file_of` / `save` (filesystem) live in `ccui_core::permission_rules`.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `<select>` order in the UI.
pub const BEHAVIORS: [&str; 3] = ["allow", "ask", "deny"];
pub const FILES: [&str; 3] = ["localSettings", "projectSettings", "userSettings"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuleText {
    pub prefix: String,
    pub emphasis: String,
    pub suffix: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PermissionRule {
    pub behavior: String,
    pub source: String,
    pub rule: String,
    /// `description` split for display.
    pub text: Option<RuleText>,
    /// persistent | session | readonly (default readonly).
    pub editability: String,
    pub not_in_effect: bool,
}

impl PermissionRule {
    /// editability == "persistent" && !not_in_effect && source ∈ FILES.
    pub fn editable(&self) -> bool {
        todo!("port: PermissionRule.Editable")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceDir {
    pub path: String,
    pub source: String,
}

/// `list_permission_rules` answer (`response.state`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PermissionRulesState {
    pub rules: Vec<PermissionRule>,
    pub workspace_dirs: Vec<WorkspaceDir>,
    /// originalCwd.
    pub cwd: Option<String>,
    pub managed_only: bool,
    /// `file · path · message` per error.
    pub errors: Vec<String>,
}

pub fn parse(response: &Value) -> PermissionRulesState {
    todo!("port: PermissionRules.Parse")
}

/// `^[A-Za-z0-9_\-:.*]+(\(.+\))?$` (dot matches newline).
pub fn valid(rule: &str) -> bool {
    todo!("port: PermissionRules.Valid")
}

/// Add/remove `rule` under `permissions[behavior]` of a settings file's text, keeping every other key and their
/// order; strict JSON only (comments / trailing commas → Err, never rewrite such a file); output indented 2 spaces,
/// no HTML/unicode escaping, trailing "\n". Idempotent.
pub fn edit(json: Option<&str>, behavior: &str, rule: &str, add: bool) -> Result<String, String> {
    todo!("port: PermissionRules.Edit")
}

/// Control (Cc) and format (Cf) chars → `\u{XXXX}` (4+ uppercase hex). Iterates chars (astral Cf escaped too:
/// an accepted improvement over the C# UTF-16 walk).
pub fn visible(s: &str) -> String {
    todo!("port: PermissionRules.Visible")
}

/// ANSI escapes removed, then each line `visible`, newlines kept.
pub fn plain(s: &str) -> String {
    todo!("port: PermissionRules.Plain")
}

/// `\x1B(\[[0-?]*[ -/]*[@-~]|\][^\x07\x1B]*(\x07|\x1B\\)|[@-_])` — CSI, OSC (BEL or ST), 2-char escapes.
/// Reused by StatusCommand.text and BgTask.tail.
pub fn ansi() -> &'static Regex {
    todo!("port: PermissionRules.Ansi")
}
