//! Port of `Core/PermissionRules.cs` (the /permissions equivalent), pure parts — spec 02 §9. Owner: shared-pure-a.
//! `file_of` / `save` (filesystem) live in `ccui_core::permission_rules`.

use crate::json;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::OnceLock;
use unicode_general_category::{get_general_category, GeneralCategory};

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
        self.editability == "persistent" && !self.not_in_effect && FILES.contains(&self.source.as_str())
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

fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    match json::prop(v, k) {
        Some(Value::Array(a)) => a,
        _ => &[],
    }
}

pub fn parse(response: &Value) -> PermissionRulesState {
    let st = json::get(response, "state");
    PermissionRulesState {
        rules: arr(st, "rules")
            .iter()
            .map(|r| PermissionRule {
                behavior: json::string(r, "behavior"),
                source: json::string(r, "source"),
                rule: json::string(r, "rule"),
                text: json::prop(r, "description").map(|d| RuleText {
                    prefix: json::string(d, "prefix"),
                    emphasis: json::string(d, "emphasis"),
                    suffix: json::string(d, "suffix"),
                }),
                editability: json::str(r, "editability").unwrap_or("readonly").to_string(),
                not_in_effect: json::bool(r, "notInEffect"),
            })
            .collect(),
        workspace_dirs: arr(st, "workspaceDirectories")
            .iter()
            .map(|d| WorkspaceDir { path: json::string(d, "path"), source: json::string(d, "source") })
            .collect(),
        cwd: json::str(st, "originalCwd").map(str::to_string),
        managed_only: json::bool(st, "managedOnly"),
        errors: arr(st, "errors")
            .iter()
            .map(|x| {
                ["file", "path", "message"].iter().filter_map(|k| json::non_empty(x, k)).collect::<Vec<_>>().join(" · ")
            })
            .collect(),
    }
}

/// `^[A-Za-z0-9_\-:.*]+(\(.+\))?$` (dot matches newline).
pub fn valid(rule: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)^[A-Za-z0-9_\-:.*]+(\(.+\))?$").expect("valid regex")).is_match(rule)
}

/// Add/remove `rule` under `permissions[behavior]` of a settings file's text, keeping every other key and their
/// order; strict JSON only (comments / trailing commas → Err, never rewrite such a file); output indented 2 spaces,
/// no HTML/unicode escaping, trailing "\n". Idempotent.
pub fn edit(json: Option<&str>, behavior: &str, rule: &str, add: bool) -> Result<String, String> {
    // A BOM (Notepad, PowerShell 5) is dropped like C# File.ReadAllText did; written back without one, like WriteAllText.
    let json = json.map(|s| s.strip_prefix('\u{feff}').unwrap_or(s));
    let mut root = match json.filter(|s| !s.trim().is_empty()) {
        None => Map::new(),
        Some(s) => match serde_json::from_str::<Value>(s).map_err(|e| e.to_string())? {
            Value::Object(o) => o,
            _ => return Err("not a JSON object".into()),
        },
    };
    let perms = root.entry("permissions").or_insert(Value::Null);
    if !perms.is_object() {
        *perms = Value::Object(Map::new());
    }
    let list = perms.as_object_mut().expect("object").entry(behavior).or_insert(Value::Null);
    if !list.is_array() {
        *list = Value::Array(Vec::new());
    }
    let list = list.as_array_mut().expect("array");
    let present = list.iter().any(|n| n.as_str() == Some(rule));
    if add && !present {
        list.push(Value::String(rule.to_string()));
    }
    if !add {
        list.retain(|n| n.as_str() != Some(rule));
    }
    let mut out = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    out.push('\n');
    Ok(out)
}

/// Control (Cc) and format (Cf) chars → `\u{XXXX}` (4+ uppercase hex). Iterates chars (astral Cf escaped too:
/// an accepted improvement over the C# UTF-16 walk).
pub fn visible(s: &str) -> String {
    let mut b = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() || get_general_category(c) == GeneralCategory::Format {
            b.push_str(&format!("\\u{{{:04X}}}", c as u32));
        } else {
            b.push(c);
        }
    }
    b
}

/// ANSI escapes removed, then each line `visible`, newlines kept.
pub fn plain(s: &str) -> String {
    ansi().replace_all(s, "").split('\n').map(visible).collect::<Vec<_>>().join("\n")
}

/// `\x1B(\[[0-?]*[ -/]*[@-~]|\][^\x07\x1B]*(\x07|\x1B\\)|[@-_])` — CSI, OSC (BEL or ST), 2-char escapes.
/// Reused by StatusCommand.text and BgTask.tail.
pub fn ansi() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\x1B(\[[0-?]*[ -/]*[@-~]|\][^\x07\x1B]*(\x07|\x1B\\)|[@-_])").expect("valid regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_list() {
        // claude 2.1.296, trimmed: a project ask rule, a project deny rule, a user rule without description.
        let st = parse(&json!({"state":{"rules":[{"behavior":"ask","source":"projectSettings","rule":"Bash(echo:*)","description":{"prefix":"Any Bash command starting with ","emphasis":"echo"},"editability":"persistent"},{"behavior":"deny","source":"projectSettings","rule":"Read(./secret.txt)","editability":"persistent"},{"behavior":"allow","source":"policySettings","rule":"WebSearch","description":{"prefix":"Any use of the ","emphasis":"WebSearch","suffix":" tool"},"editability":"readonly"}],"workspaceDirectories":[{"path":"/x","source":"cliArg"}],"originalCwd":"/w","managedOnly":false}}));
        let r = &st.rules;
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].behavior, "ask");
        assert_eq!(
            r[0].text,
            Some(RuleText { prefix: "Any Bash command starting with ".into(), emphasis: "echo".into(), suffix: "".into() })
        );
        assert!(r[0].editable() && r[1].text.is_none() && r[1].editable() && !r[2].editable());
        assert_eq!(r[2].text.as_ref().unwrap().suffix, " tool");
        assert_eq!(st.workspace_dirs, vec![WorkspaceDir { path: "/x".into(), source: "cliArg".into() }]);
        assert_eq!(st.cwd.as_deref(), Some("/w"));
        assert!(st.errors.is_empty());

        let m = parse(&json!({"state":{"rules":[{"behavior":"allow","source":"localSettings","rule":"Bash","editability":"readonly","notInEffect":true}],"workspaceDirectories":[],"originalCwd":"/w","managedOnly":true,"errors":[{"file":"/w/.claude/settings.json","path":"","message":"Invalid JSON"}]}}));
        assert!(m.managed_only && !m.rules[0].editable());
        assert_eq!(m.errors, vec!["/w/.claude/settings.json · Invalid JSON"]);
        assert_eq!(parse(&json!({})), PermissionRulesState::default());
    }

    #[test]
    fn edit_settings() {
        let added = edit(Some(r#"{"model":"haiku","permissions":{"allow":["Bash(ls:*)"],"defaultMode":"plan"}}"#), "deny", "Read(./.env)", true).unwrap();
        let n: Value = serde_json::from_str(&added).unwrap();
        assert_eq!(n["model"], "haiku");
        assert_eq!(n["permissions"]["defaultMode"], "plan");
        assert_eq!(n["permissions"]["deny"][0], "Read(./.env)");
        assert_eq!(n["permissions"]["allow"].as_array().unwrap().len(), 1);
        assert!(added.starts_with("{\n  \"model\": \"haiku\",\n  \"permissions\": {") && added.ends_with("}\n"), "{added}");
        assert_eq!(edit(Some(&added), "deny", "Read(./.env)", true).unwrap(), added, "add is idempotent");
        let removed: Value = serde_json::from_str(&edit(Some(&added), "allow", "Bash(ls:*)", false).unwrap()).unwrap();
        assert!(removed["permissions"]["allow"].as_array().unwrap().is_empty(), "remove");
        assert!(edit(None, "allow", "Bash(a<b)", true).unwrap().contains("\"Bash(a<b)\""), "no \\u escaping");
        assert!(edit(Some("  "), "allow", "Bash(é)", true).unwrap().contains("\"Bash(é)\""));
        assert!(edit(Some("{ // comment\n}"), "allow", "Bash", true).is_err(), "never rewrite a file the CLI skips");
        assert!(edit(Some(r#"{"a":1,}"#), "allow", "Bash", true).is_err());
        assert!(edit(Some("[]"), "allow", "Bash", true).is_err());
        let bom = edit(Some("\u{feff}{\"model\":\"haiku\"}"), "allow", "Bash", true).unwrap();
        assert!(bom.starts_with("{\n  \"model\": \"haiku\""), "BOM dropped, file still edited: {bom}");
    }

    #[test]
    fn syntax_and_sanitizing() {
        assert!(valid("Bash(npm run:*)") && valid("mcp__roseline__*") && valid("WebFetch(domain:github.com)"));
        assert!(valid("Bash(a\nb)"), "dot matches newline");
        assert!(!valid("") && !valid("Bash(") && !valid("two words"));
        assert_eq!(visible("Bash(a\u{200B}b)"), "Bash(a\\u{200B}b)");
        assert_eq!(plain("\u{1b}[31mBlocked\u{1b}[0m by hook\nline\u{7}"), "Blocked by hook\nline\\u{0007}");
        assert_eq!(ansi().replace_all("\u{1b}]0;title\u{7}x\u{1b}]8;;u\u{1b}\\y", ""), "xy");
    }
}
