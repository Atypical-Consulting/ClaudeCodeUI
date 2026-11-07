//! Port of the hook records of `Core/Events.cs` (`HookRow`, `HookEdit`, `HookEvent`, `HookListing`) and
//! `Events.ContextOnly` — spec 02 §5.1-5.2. Owner: core-events (parse lives in `ccui_core::events::hooks`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What `claude edit-hook` names as the target of a replace or remove (rows of writable settings files only).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HookEdit {
    pub matcher: String,
    /// The stored hook entry, raw.
    pub config: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HookRow {
    pub event: String,
    pub matcher: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// commandText ?? displayText ?? "".
    pub command: String,
    /// displayText, None when equal to command.
    pub display: Option<String>,
    pub source: String,
    pub source_label: Option<String>,
    pub plugin_name: Option<String>,
    pub timeout: Option<i32>,
    /// The row's own flag (set on every row under disableAllHooks).
    pub disabled: bool,
    pub edit: Option<HookEdit>,
}

impl HookRow {
    /// edit present AND edit.config.onFailure == "block".
    pub fn blocks_on_failure(&self) -> bool {
        todo!("port: HookRow.BlocksOnFailure")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HookEvent {
    pub name: String,
    pub summary: String,
    /// supportsMatcher.
    pub matcher: bool,
}

/// `get_hooks_listing` answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HookListing {
    pub rows: Vec<HookRow>,
    /// allDisabled || disabledByPolicy.
    pub disabled: bool,
    /// disabledByPolicy || managedOnly || pluginOnly || policyUnreadable.
    pub locked: bool,
    pub catalog: Vec<HookEvent>,
}

/// True iff `output` (trim_start) is a JSON object whose keys ⊆ {hookSpecificOutput, suppressOutput},
/// hookSpecificOutput is an object with keys ⊆ {hookEventName, additionalContext}. Parse error → false.
pub fn context_only(output: &str) -> bool {
    todo!("port: Events.ContextOnly")
}
