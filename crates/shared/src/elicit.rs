//! Port of `Core/Elicit.cs` (MCP elicitation) — spec 02 §12. Owner: shared-pure-a.
//! Queued as a PendingPermission (tool `dialog:mcp_elicitation`, input = the whole request).

use crate::model::PendingPermission;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

/// The tool_name the CLI itself gives this pending action.
pub const TOOL: &str = "dialog:mcp_elicitation";

/// Field kind: `multi` (array), `boolean`, `number`, `integer`, `enum` (options non-empty), `string`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Multi,
    Boolean,
    Number,
    Integer,
    Enum,
    String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub key: String,
    /// title ?? key.
    pub label: String,
    pub description: Option<String>,
    pub kind: FieldKind,
    pub required: bool,
    /// `(value, label)`.
    pub options: Vec<(String, String)>,
    pub default: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// `<input type>`: email | url | date | datetime-local | text.
    pub input_type: String,
}

/// mode == "url".
pub fn is_url(request: &Value) -> bool {
    todo!("port: Elicit.IsUrl")
}

/// `url` only if absolute http(s).
pub fn url(request: &Value) -> Option<String> {
    todo!("port: Elicit.Url")
}

/// pending.tool == TOOL && input.mcp_server_name == server && input.elicitation_id == id.
pub fn matches(pending: &PendingPermission, server: &str, elicitation_id: &str) -> bool {
    todo!("port: Elicit.Matches")
}

/// Fields of `requested_schema.properties` in JSON order; missing → [].
pub fn parse(request: &Value) -> Vec<Field> {
    todo!("port: Elicit.Parse")
}

/// Form content in schema order, or `(None, Some(label))` for the first field missing or invalid.
/// Integral f64 numbers are written as JSON integers (C# shortest round-trip: `36`, not `36.0`).
/// datetime-local goes out as RFC 3339 with the local offset.
pub fn content(
    fields: &[Field],
    text: &HashMap<String, String>,
    picks: &HashMap<String, BTreeSet<String>>,
) -> (Option<Map<String, Value>>, Option<String>) {
    todo!("port: Elicit.Content")
}

/// `{"action":…}` or `{"action":…,"content":…}` (that key order).
pub fn reply(action: &str, content: Option<Value>) -> Value {
    todo!("port: Elicit.Reply")
}
