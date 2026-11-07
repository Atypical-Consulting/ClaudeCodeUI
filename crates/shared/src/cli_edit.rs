//! Port of the pure parts of `Core/CliEdit.cs` (payload builders + error extraction) — spec 04 §8.
//! Owner: core-extensions. Running `claude edit-hook --json` / `edit-memory-settings --json` is
//! `ccui_core::cli_edit::run`.

use crate::hooks::HookEdit;
use serde_json::Value;

/// The CLI's error for a failed writer: last stdout line (from the end) that parses as JSON with an `error` string;
/// else the last non-empty trimmed line of `err + "\n" + output`; else `"claude " + command`.
pub fn error(output: &str, err: &str, command: &str) -> String {
    todo!("port: CliEdit.Error")
}

/// `{"op","source","event","matcher"[,"hook"][,"target":{"event","matcher","hook"}]}` in exactly that key order.
/// op = add (no target) | replace (hook + target) | remove (target only). `target` = (row event, row edit).
pub fn hook(op: &str, source: &str, event: &str, matcher: &str, hook: Option<Value>, target: Option<(&str, &HookEdit)>) -> Value {
    todo!("port: CliEdit.Hook")
}

/// Clone of the stored hook (or `{}`) with type = "command", command, timeout (> 0 else removed),
/// onFailure = "block" when block (else removed).
pub fn command_hook(stored: Option<&Value>, command: &str, timeout: i32, block: bool) -> Value {
    todo!("port: CliEdit.CommandHook")
}
