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
        self.edit.as_ref().is_some_and(|e| crate::json::str(&e.config, "onFailure") == Some("block"))
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
    if !output.trim_start().starts_with('{') {
        return false;
    }
    let only = |v: &Value, keys: &[&str]| v.as_object().is_some_and(|o| o.keys().all(|k| keys.contains(&k.as_str())));
    match serde_json::from_str::<Value>(output) {
        Ok(r) => {
            only(&r, &["hookSpecificOutput", "suppressOutput"])
                && crate::json::prop(&r, "hookSpecificOutput").is_some_and(|o| only(o, &["hookEventName", "additionalContext"]))
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_only_cases() {
        // claude 2.1.296: a SubagentStart hook's output (compact), a SessionStart plugin's (pretty-printed).
        assert!(context_only(
            r#"{"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": "ccui-probe-subagent-ctx"}}"#
        ));
        assert!(context_only(
            "{\n  \"hookSpecificOutput\": {\n    \"hookEventName\": \"SessionStart\",\n    \"additionalContext\": \"x\"\n  }\n}"
        ));
        assert!(!context_only(
            r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"no"}}"#
        ));
        assert!(!context_only(r#"{"decision":"block","reason":"tests fail"}"#));
        assert!(!context_only("ccui-probe-pre") && !context_only("{oops"));
        assert!(!context_only(r#"{"suppressOutput":true}"#));
    }

    #[test]
    fn blocks_on_failure() {
        let row = |edit| HookRow {
            event: "Stop".into(),
            matcher: "".into(),
            kind: "command".into(),
            command: "x".into(),
            display: None,
            source: "projectSettings".into(),
            source_label: None,
            plugin_name: None,
            timeout: None,
            disabled: false,
            edit,
        };
        assert!(row(Some(HookEdit { matcher: "".into(), config: json!({"onFailure":"block"}) })).blocks_on_failure());
        assert!(!row(Some(HookEdit { matcher: "".into(), config: json!({}) })).blocks_on_failure());
        assert!(!row(None).blocks_on_failure());
    }
}
