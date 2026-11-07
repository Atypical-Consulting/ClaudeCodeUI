//! Port of the records and parsers of `Core/Plugins.cs` (`claude plugin … --json` driver) — spec 05 §1.
//! Owner: core-extensions. Running the CLI is `ccui_core::plugins`.

use crate::json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Install scope `<select>` options, in this order.
pub const SCOPES: [&str; 3] = ["user", "project", "local"];

/// One installed plugin in one scope (a plugin installed twice is two rows).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginRow {
    pub id: String,
    pub version: String,
    /// user | project | local | session.
    pub scope: String,
    pub enabled: bool,
    /// The CLI's own lines, worded for a person: display verbatim.
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    pub has_options: bool,
    pub mcp_servers: Vec<String>,
}

impl PluginRow {
    /// `id` before '@' ("demo@probe-mk" → "demo").
    pub fn name(&self) -> &str {
        self.id.split('@').next().unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ListedPlugin {
    pub id: String,
    pub description: Option<String>,
    pub marketplace: String,
    pub installs: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginList {
    pub installed: Vec<PluginRow>,
    pub available: Vec<ListedPlugin>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub name: String,
    /// repo ?? url ?? path ?? source ?? "".
    pub location: String,
}

/// Result of an act (install, enable…). `command` + `command_sha`: the marketplace runs a command first — show it
/// and offer Accept (re-run the same args + `--accept-command <sha>`); never auto-accept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub ok: bool,
    pub message: String,
    pub command: Option<String>,
    pub command_sha: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginOption {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub required: bool,
    pub sensitive: bool,
    /// Always "" for a sensitive one (never echoed).
    pub value: String,
    pub choices: Vec<String>,
    pub set: bool,
}

/// `claude plugin list --json --available` output.
pub fn parse_list(json: &Value) -> PluginList {
    let arr = |k| match json::prop(json, k) {
        Some(Value::Array(a)) => a.as_slice(),
        _ => &[],
    };
    PluginList {
        installed: arr("installed")
            .iter()
            .map(|p| PluginRow {
                id: json::string(p, "id"),
                version: json::string(p, "version"),
                scope: json::string(p, "scope"),
                enabled: json::bool(p, "enabled"),
                errors: json::names(p, "errors"),
                notes: json::names(p, "notes"),
                has_options: json::bool(p, "hasUserConfig"),
                mcp_servers: match json::prop(p, "mcpServers") {
                    Some(Value::Object(ms)) => ms.keys().cloned().collect(),
                    _ => Vec::new(),
                },
            })
            .collect(),
        available: arr("available")
            .iter()
            .map(|p| ListedPlugin {
                id: json::string(p, "pluginId"),
                description: json::str(p, "description").map(str::to_string),
                marketplace: json::string(p, "marketplaceName"),
                installs: json::long(p, "installCount"),
            })
            .collect(),
    }
}

/// `claude plugin marketplace list --json` output (array; else []).
pub fn parse_markets(json: &Value) -> Vec<Market> {
    let Value::Array(a) = json else { return Vec::new() };
    a.iter()
        .map(|m| Market {
            name: json::string(m, "name"),
            location: ["repo", "url", "path", "source"].iter().find_map(|k| json::str(m, k)).unwrap_or_default().to_string(),
        })
        .collect()
}

/// Last stdout line (from the end) that is a JSON object with a string `outcome`.
pub fn parse_outcome(stdout: &str) -> Option<Outcome> {
    stdout.split('\n').rev().filter(|l| l.trim_start().starts_with('{')).find_map(|line| {
        let r: Value = serde_json::from_str(line).ok()?;
        let o = json::str(&r, "outcome")?;
        let shown = json::prop(&r, "shownCommand");
        let command = match shown.and_then(|c| json::prop(c, "command")) {
            Some(Value::Array(a)) => Some(a.iter().map(json::to_text).collect::<Vec<_>>().join(" ")),
            Some(c) => Some(json::to_text(c)),
            None => None,
        };
        Some(Outcome {
            ok: o == "ok",
            message: json::str(&r, "message").unwrap_or(o).to_string(),
            command,
            command_sha: shown.and_then(|c| json::str(c, "sha256")).map(str::to_string),
        })
    })
}

/// `claude plugin configure <id> --json` output: one option per `schema` property, schema order.
pub fn parse_options(json: &Value) -> Vec<PluginOption> {
    let set = json::names(json, "configured");
    let inputs = json::prop(json, "inputs");
    let choices = json::prop(json, "choices");
    let Some(Value::Object(schema)) = json::prop(json, "schema") else { return Vec::new() };
    schema
        .iter()
        .map(|(k, o)| PluginOption {
            key: k.clone(),
            kind: json::str(o, "type").unwrap_or("string").to_string(),
            title: json::str(o, "title").unwrap_or(k).to_string(),
            description: json::str(o, "description").map(str::to_string),
            required: json::bool(o, "required"),
            sensitive: json::bool(o, "sensitive"),
            value: inputs.and_then(|iv| json::str(iv, k)).unwrap_or_default().to_string(),
            choices: choices.map(|cv| json::names(cv, k)).unwrap_or_default(),
            set: set.contains(k),
        })
        .collect()
}

/// Failed save: stdout's `refused.message`, else `line(err, out)`.
pub fn save_error(stdout: &str, stderr: &str) -> String {
    serde_json::from_str::<Value>(stdout)
        .ok()
        .and_then(|r| json::prop(&r, "refused").and_then(|rf| json::str(rf, "message")).map(str::to_string))
        .unwrap_or_else(|| line(stderr, stdout))
}

/// Last non-empty trimmed line of `err + "\n" + out`, else "claude plugin".
pub fn line(err: &str, out: &str) -> String {
    last_line(err, out).unwrap_or_else(|| "claude plugin".into())
}

/// Last non-empty trimmed line of `err + "\n" + out` (shared with `cli_edit::error`).
pub(crate) fn last_line(err: &str, out: &str) -> Option<String> {
    err.split('\n').chain(out.split('\n')).map(str::trim).filter(|l| !l.is_empty()).last().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed real answers of claude 2.1.296 (a local marketplace with a "demo" plugin declaring 4 userConfig options).
    fn v(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn list() {
        let l = parse_list(&v(r#"{"installed":[{"id":"demo@probe-mk","version":"1.0.0","scope":"user","enabled":false,"installPath":"/c/demo/1.0.0","projectEnabled":false,"hasUserConfig":true},{"id":"bad@inline","version":"unknown","scope":"session","enabled":false,"installPath":"/w/bad","errors":["Path not found: /w/bad"],"errorDetails":[{"type":"path-not-found","path":"/w/bad"}]}],"available":[{"pluginId":"demo@probe-mk","name":"demo","description":"Demo plugin","marketplaceName":"probe-mk","version":"1.0.0","source":"./plugins/demo"},{"pluginId":"figma@claude-plugins-official","name":"figma","marketplaceName":"claude-plugins-official","installCount":3952}]}"#));
        let r = &l.installed;
        assert_eq!(r.len(), 2);
        assert!(r[0].name() == "demo" && r[0].scope == "user" && !r[0].enabled && r[0].has_options && r[0].errors.is_empty());
        assert!(r[1].errors == ["Path not found: /w/bad"] && r[1].notes.is_empty());
        let a = &l.available;
        assert!(a.len() == 2 && a[0].id == "demo@probe-mk" && a[0].description.as_deref() == Some("Demo plugin") && a[0].installs == 0);
        assert!(a[1].marketplace == "claude-plugins-official" && a[1].installs == 3952);
    }

    #[test]
    fn marketplaces() {
        let m = parse_markets(&v(r#"[{"name":"official","source":"github","repo":"anthropics/claude-plugins-official","installLocation":"/m"},{"name":"probe-mk","source":"directory","path":"/w/mk","installLocation":"/w/mk"}]"#));
        assert!(m.len() == 2 && m[0].name == "official" && m[0].location == "anthropics/claude-plugins-official" && m[1].location == "/w/mk");
        assert!(parse_markets(&v("{}")).is_empty());
    }

    #[test]
    fn outcomes() {
        let ok = parse_outcome(&(r#"{"command":"enable","outcome":"ok","plugin":"demo@probe-mk","pluginId":"demo@probe-mk","scope":"user","message":"Successfully enabled plugin: demo (scope: user)"}"#.to_string() + "\n")).unwrap();
        assert!(ok.ok && ok.message == "Successfully enabled plugin: demo (scope: user)" && ok.command.is_none());
        let failed = parse_outcome(r#"{"command":"disable","outcome":"failed","plugin":"demo@probe-mk","message":"Plugin \"demo@probe-mk\" is already disabled","failureCode":"already_in_goal_state","alreadyInGoalState":true}"#).unwrap();
        assert!(!failed.ok && failed.message == "Plugin \"demo@probe-mk\" is already disabled");
        let cmd = parse_outcome(r#"{"command":"install","outcome":"failed","plugin":"x@m","message":"Refused","failureCode":"command_source_refused","shownCommand":{"kind":"command_source","pluginId":"x@m","command":["npx","x"],"sha256":"ab12"}}"#).unwrap();
        assert!(!cmd.ok && cmd.command.as_deref() == Some("npx x") && cmd.command_sha.as_deref() == Some("ab12"));
        assert!(parse_outcome("Usage: …").is_none());
    }

    #[test]
    fn configure() {
        let o = parse_options(&v(r#"{"pluginId":"demo@probe-mk","displayName":"demo","schema":{"api_url":{"type":"string","title":"API URL","description":"Where to call","required":true},"token":{"type":"string","title":"Token","description":"Secret","sensitive":true},"verbose":{"type":"boolean","title":"Verbose","description":"Log more","default":false}},"inputs":{"api_url":"https://x.test","token":"","verbose":"true"},"choices":{"verbose":["true","false"]},"configured":["api_url","token"],"unconfigured":["verbose"]}"#));
        assert_eq!(o.len(), 3);
        assert!(o[0].key == "api_url" && o[0].required && o[0].value == "https://x.test" && o[0].set);
        assert!(o[1].sensitive && o[1].value.is_empty() && o[1].set);
        assert!(o[2].kind == "boolean" && o[2].choices == ["true", "false"] && !o[2].set);
    }

    #[test]
    fn error_lines() {
        assert_eq!(line("", "a\n  b  \n\n"), "b");
        assert_eq!(line("err\n", ""), "err");
        assert_eq!(line("", ""), "claude plugin");
        assert_eq!(save_error(r#"{"refused":{"message":"out of range"}}"#, "x"), "out of range");
        assert_eq!(save_error("not json", "boom\n"), "not json");
    }
}
