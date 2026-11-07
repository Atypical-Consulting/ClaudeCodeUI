//! Port of the pure parts of `Core/CliEdit.cs` (payload builders + error extraction) — spec 04 §8.
//! Owner: core-extensions. Running `claude edit-hook --json` / `edit-memory-settings --json` is
//! `ccui_core::cli_edit::run`.

use crate::hooks::HookEdit;
use crate::json;
use serde_json::{json, Map, Value};

/// The CLI's error for a failed writer: last stdout line (from the end) that parses as JSON with an `error` string;
/// else the last non-empty trimmed line of `err + "\n" + output`; else `"claude " + command`.
pub fn error(output: &str, err: &str, command: &str) -> String {
    output
        .split('\n')
        .rev()
        .filter(|l| l.trim_start().starts_with('{'))
        .find_map(|l| serde_json::from_str::<Value>(l).ok().and_then(|v| json::str(&v, "error").map(str::to_string)))
        .or_else(|| crate::plugins::last_line(err, output))
        .unwrap_or_else(|| format!("claude {command}"))
}

/// `{"op","source","event","matcher"[,"hook"][,"target":{"event","matcher","hook"}]}` in exactly that key order.
/// op = add (no target) | replace (hook + target) | remove (target only). `target` = (row event, row edit).
pub fn hook(op: &str, source: &str, event: &str, matcher: &str, hook: Option<Value>, target: Option<(&str, &HookEdit)>) -> Value {
    let mut o = Map::new();
    o.insert("op".into(), op.into());
    o.insert("source".into(), source.into());
    o.insert("event".into(), event.into());
    o.insert("matcher".into(), matcher.into());
    if let Some(h) = hook {
        o.insert("hook".into(), h);
    }
    if let Some((ev, t)) = target {
        o.insert("target".into(), json!({"event": ev, "matcher": t.matcher, "hook": t.config}));
    }
    Value::Object(o)
}

/// Clone of the stored hook (or `{}`) with type = "command", command, timeout (> 0 else removed),
/// onFailure = "block" when block (else removed).
pub fn command_hook(stored: Option<&Value>, command: &str, timeout: i32, block: bool) -> Value {
    let mut h = match stored {
        Some(Value::Object(o)) => o.clone(),
        _ => Map::new(),
    };
    h.insert("type".into(), "command".into());
    h.insert("command".into(), command.into());
    if timeout > 0 {
        h.insert("timeout".into(), timeout.into());
    } else {
        h.shift_remove("timeout");
    }
    if block {
        h.insert("onFailure".into(), "block".into());
    } else {
        h.shift_remove("onFailure");
    }
    Value::Object(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_text() {
        // claude 2.1.296: edit-hook's refusal, edit-memory-settings' success line, a bare stderr failure.
        assert_eq!(
            error(
                "{\"ok\":false,\"error\":\"edit_hook: event is not a known hook event\",\"code\":\"bad_request\"}\n",
                "edit_hook: event is not a known hook event\n",
                "edit-hook"
            ),
            "edit_hook: event is not a known hook event"
        );
        assert_eq!(error("", "boom\n\n", "x"), "boom");
        assert_eq!(error("", "", "x"), "claude x");
    }

    #[test]
    fn hook_payloads() {
        let edit = HookEdit {
            matcher: String::new(),
            config: serde_json::from_str(r#"{"type":"command","command":"echo bye","onFailure":"block","statusMessage":"Bye"}"#).unwrap(),
        };
        let add = hook("add", "projectSettings", "PreToolUse", "Bash", Some(command_hook(None, "echo hi", 5, false)), None).to_string();
        assert_eq!(
            add,
            r#"{"op":"add","source":"projectSettings","event":"PreToolUse","matcher":"Bash","hook":{"type":"command","command":"echo hi","timeout":5}}"#,
            "add, the shape the probe wrote"
        );
        let rep = hook(
            "replace",
            "projectSettings",
            "PostToolUse",
            "Edit",
            Some(command_hook(Some(&edit.config), "echo x", 0, false)),
            Some(("PostToolUse", &edit)),
        );
        assert_eq!(rep["target"]["hook"]["onFailure"], "block");
        assert_eq!(rep["hook"]["statusMessage"], "Bye");
        assert!(rep["hook"].get("onFailure").is_none() && rep["hook"].get("timeout").is_none());
        assert_eq!(rep["target"]["event"], "PostToolUse");
        // replace keeps the stored key order
        assert_eq!(rep["hook"].to_string(), r#"{"type":"command","command":"echo x","statusMessage":"Bye"}"#);
        let rm = hook("remove", "projectSettings", "PostToolUse", "", None, Some(("PostToolUse", &edit)));
        assert!(rm.get("hook").is_none() && rm.get("target").is_some(), "remove names only the target");
    }
}
