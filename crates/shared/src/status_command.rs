//! Port of the pure parts of `Core/StatusCommand.cs` (the settings' statusLine command) — spec 04 §9.
//! Owner: core-live-session. `payload` / `run` are `ccui_core::status_command`.

use crate::json;
use serde_json::Value;

/// `settings.effective.statusLine.command` when type == "command" and non-empty (input: get_settings response).
pub fn of(settings: &Value) -> Option<String> {
    let sl = json::prop(json::prop(settings, "effective")?, "statusLine")?;
    (json::str(sl, "type") == Some("command")).then(|| json::non_empty(sl, "command")).flatten()
}

/// `PermissionRules::plain(stdout)`, lines trimmed at the end, blanks dropped, at most 3, joined `\n`; none → None.
pub fn text(stdout: &str) -> Option<String> {
    let plain = crate::permission_rules::plain(stdout);
    let lines: Vec<&str> = plain.split('\n').map(str::trim_end).filter(|l| !l.trim().is_empty()).take(3).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_line_from_settings() {
        assert_eq!(of(&json!({"effective":{"statusLine":{"type":"command","command":"~/.claude/sl.sh","padding":0}}})).as_deref(), Some("~/.claude/sl.sh"));
        assert_eq!(of(&json!({"effective":{}})), None);
        assert_eq!(of(&json!({"effective":{"statusLine":{"type":"x","command":"y"}}})), None);
    }

    #[test]
    fn printed_lines() {
        assert_eq!(text("\u{1b}[1;32mmain\u{1b}[0m · opus\n\n  ctx 12%\nthree\nfour\n").as_deref(), Some("main · opus\n  ctx 12%\nthree"));
        assert_eq!(text(" \n"), None);
    }
}
