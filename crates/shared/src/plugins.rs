//! Port of the records and parsers of `Core/Plugins.cs` (`claude plugin … --json` driver) — spec 05 §1.
//! Owner: core-extensions. Running the CLI is `ccui_core::plugins`.

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
        todo!("port: Plugins.Row.Name")
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
    todo!("port: Plugins.List (parse)")
}

/// `claude plugin marketplace list --json` output (array; else []).
pub fn parse_markets(json: &Value) -> Vec<Market> {
    todo!("port: Plugins.Markets (parse)")
}

/// Last stdout line (from the end) that is a JSON object with a string `outcome`.
pub fn parse_outcome(stdout: &str) -> Option<Outcome> {
    todo!("port: Plugins.ParseOutcome")
}

/// `claude plugin configure <id> --json` output: one option per `schema` property, schema order.
pub fn parse_options(json: &Value) -> Vec<PluginOption> {
    todo!("port: Plugins.Options (parse)")
}

/// Failed save: stdout's `refused.message`, else `line(err, out)`.
pub fn save_error(stdout: &str, stderr: &str) -> String {
    todo!("port: Plugins.Save (error)")
}

/// Last non-empty trimmed line of `err + "\n" + out`, else "claude plugin".
pub fn line(err: &str, out: &str) -> String {
    todo!("port: Plugins.Line")
}
