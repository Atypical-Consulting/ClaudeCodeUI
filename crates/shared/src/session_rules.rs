//! Port of the pure helpers of `Core/LiveSession.cs` both sides need — spec 01 §2.4 (constants), §6.2, §6.4, §6.5.
//! Owner: core-live-session. The reducer and commands are `ccui_core::live`.

use crate::json;
use crate::model::{HookEvt, Item, PendingPermission, SessionStatus, ToolItem, ToolState};
use crate::session::{SessionMeta, SessionsSummary};
use regex::Regex;
use serde_json::Value;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Modes offered by the UI (bypassPermissions / dontAsk deliberately not offered).
pub const MODES: [&str; 4] = ["default", "acceptEdits", "plan", "auto"];
/// Protocol sentinel for an interrupted turn (never displayed).
pub const INTERRUPTED: &str = "interrupted";
pub const PUSH_TOOL: &str = "PushNotification";
pub const CONFIG_USAGE: &str = "Usage: /config key=value";

/// auto | acceptEdits: the UI confirms these when not isolated.
pub fn risky(mode: &str) -> bool {
    matches!(mode, "auto" | "acceptEdits")
}

/// The text a user line shows as a prompt: None for interrupts, local-command echoes, task notifications and
/// subagent hand-backs; `<command-name>/x</command-name><command-args>a</command-args>` → "/x a"; else t.
pub fn user_text(t: &str) -> Option<String> {
    static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<command-name>(.*?)</command-name>").unwrap());
    static ARGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<command-args>(.*?)</command-args>").unwrap());
    if t.starts_with("[Request interrupted by user")
        || t.starts_with("<local-command-")
        || t.starts_with("<task-notification>")
        || t.starts_with("<agent-message")
    {
        return None;
    }
    let Some(name) = NAME.captures(t).and_then(|c| c.get(1)) else { return Some(t.to_string()) };
    let args = ARGS.captures(t).and_then(|c| c.get(1)).map_or("", |m| m.as_str().trim());
    Some(if args.is_empty() { name.as_str().to_string() } else { format!("{} {args}", name.as_str()) })
}

/// First line that is non-empty after trimming `" ' \` * .` → `text::trunc`.
pub fn title_from(output: &str) -> Option<String> {
    output
        .split('\n')
        .map(|l| l.trim().trim_matches(['"', '\'', '`', '*', '.']).trim())
        .find(|l| !l.is_empty())
        .and_then(|t| crate::text::trunc(Some(t)))
}

/// `Path.TrimEndingDirectorySeparator`: one trailing separator off, a root ("/", "C:\") kept.
fn trim_sep(d: &str) -> &str {
    let root = d.len() == 1 || (d.len() == 3 && d.as_bytes()[1] == b':');
    if !root && (d.ends_with('/') || d.ends_with('\\')) {
        &d[..d.len() - 1]
    } else {
        d
    }
}

/// `--add-dir` candidate: trimmed, unquoted, trailing separator dropped ("/" stays); None when empty, equal to cwd or
/// already listed. `case_sensitive` = Linux.
pub fn extra_dir(raw: &str, cwd: &str, dirs: &[String], case_sensitive: bool) -> Option<String> {
    let d = raw.trim().trim_matches('"').trim();
    if d.is_empty() {
        return None;
    }
    let d = trim_sep(d);
    let same = |a: &str, b: &str| if case_sensitive { a == b } else { a.to_uppercase() == b.to_uppercase() };
    (!same(d, trim_sep(cwd)) && !dirs.iter().any(|x| same(x, d))).then(|| d.to_string())
}

/// `effective.maxEffortLevel`, overridden by the first `modelSettings` key the model starts with (case-insensitive);
/// "max" → None (exempt).
pub fn effort_cap(effective: &Value, model: Option<&str>) -> Option<String> {
    let mut cap = json::str(effective, "maxEffortLevel");
    if let (Some(m), Some(Value::Object(ms))) = (model, json::prop(effective, "modelSettings")) {
        let m = m.to_lowercase();
        for (k, v) in ms {
            if m.starts_with(&k.to_lowercase()) {
                if let Some(own) = json::str(v, "maxEffortLevel") {
                    cap = Some(own);
                }
            }
        }
    }
    cap.filter(|c| *c != "max").map(str::to_string)
}

/// Levels up to and including cap when cap ∈ levels, else all.
pub fn capped<'a>(levels: &[&'a str], cap: Option<&str>) -> Vec<&'a str> {
    match cap.and_then(|c| levels.iter().position(|l| *l == c)) {
        Some(i) => levels[..=i].to_vec(),
        None => levels.to_vec(),
    }
}

/// `/config` usage text → `(key, allowed values)` (value starting with '<' → free text = []); else None.
pub fn config_keys(text: &str) -> Option<Vec<(String, Vec<String>)>> {
    if !text.starts_with(CONFIG_USAGE) {
        return None;
    }
    Some(
        text.split('\n')
            .skip(1)
            .map(str::trim)
            .filter_map(|l| {
                let i = l.find('=').filter(|i| *i > 0)?;
                let v = &l[i + 1..];
                let choices = if v.starts_with('<') { Vec::new() } else { v.split('|').map(str::to_string).collect() };
                Some((l[..i].to_string(), choices))
            })
            .collect(),
    )
}

/// EnterWorktree → worktreePath; ExitWorktree → restoredCwd ?? originalCwd; else None.
pub fn worktree_move(tool: &str, structured: Option<&Value>) -> Option<String> {
    let r = structured.filter(|r| r.is_object())?;
    match tool {
        "EnterWorktree" => json::str(r, "worktreePath"),
        "ExitWorktree" => json::str(r, "restoredCwd").or_else(|| json::str(r, "originalCwd")),
        _ => None,
    }
    .map(str::to_string)
}

/// Fork history: up to and including the TextItem with uuid `at`, else all.
pub fn upto(mut items: Vec<Item>, at: Option<&str>) -> Vec<Item> {
    let cut = at.and_then(|at| items.iter().position(|x| matches!(x, Item::Text(t) if t.uuid.as_deref() == Some(at))));
    if let Some(i) = cut {
        items.truncate(i + 1);
    }
    items
}

// Item-slice helpers are generic over `I: Borrow<Item>` so the backend passes `&[Item]` and the UI passes the
// `&[&Item]` of its per-row signals (`SessionHandle::with_items`) without cloning.

/// uuids of the target and every UserItem after it, newest first (rewind_conversation takes only the latest).
pub fn rewind_steps<I: Borrow<Item>>(items: &[I], uuid: &str) -> Vec<String> {
    let mut steps: Vec<String> = items
        .iter()
        .filter_map(|i| i.borrow().as_user())
        .skip_while(|u| u.uuid.as_deref() != Some(uuid))
        .filter_map(|u| u.uuid.clone())
        .collect();
    steps.reverse();
    steps
}

/// `CanRewind(u)`: u.uuid ∈ meta.sent && has_process && Idle && no running background ToolItem.
pub fn can_rewind<I: Borrow<Item>>(meta: &SessionMeta, items: &[I], uuid: Option<&str>) -> bool {
    uuid.is_some_and(|id| meta.sent.iter().any(|s| s == id))
        && meta.has_process
        && meta.status == SessionStatus::Idle
        && !background_running(items)
}

/// A background ToolItem still Running (an agent may still write files).
pub fn background_running<I: Borrow<Item>>(items: &[I]) -> bool {
    items.iter().any(|i| i.borrow().as_tool().is_some_and(|t| t.background && t.state == ToolState::Running))
}

/// Last ToolItem with that id.
pub fn tool<'a, I: Borrow<Item>>(items: &'a [I], id: &str) -> Option<&'a ToolItem> {
    items.iter().rev().find_map(|i| i.borrow().as_tool().filter(|t| t.id == id))
}

/// parent_tool_use_id → indexes of its Tool/Text items, in order (`Children`).
pub fn children<I: Borrow<Item>>(items: &[I]) -> HashMap<String, Vec<usize>> {
    let mut by: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        if let Some(p) = it.borrow().parent() {
            by.entry(p.to_string()).or_default().push(i);
        }
    }
    by
}

/// `AskingAgent(p)`: last ToolItem with task_id == info.agent_id, else the parent tool of tool(p.tool_use_id).
pub fn asking_agent<'a, I: Borrow<Item>>(items: &'a [I], p: &PendingPermission) -> Option<&'a ToolItem> {
    let a = p.info.as_ref()?.agent_id.as_deref()?;
    items.iter().rev().find_map(|i| i.borrow().as_tool().filter(|t| t.task_id.as_deref() == Some(a))).or_else(|| {
        let parent = tool(items, p.tool_use_id.as_deref()?)?.parent_tool_use_id.as_deref()?;
        tool(items, parent)
    })
}

/// `PermissionCard.AgentLabel(a)`: input.description ?? input.subagent_type ?? name. Used by the PermissionCard and
/// by the backend for `DecisionEntry.agent_label`.
pub fn agent_label(agent: &ToolItem) -> String {
    json::str(&agent.input, "description")
        .or_else(|| json::str(&agent.input, "subagent_type"))
        .unwrap_or(&agent.name)
        .to_string()
}

/// Hook shown in the thread: outcome != success, or output non-empty and not SessionStart*/SubagentStart* and not
/// `hooks::context_only`.
pub fn shown(hook: &HookEvt) -> bool {
    hook.outcome != "success"
        || (!hook.output.is_empty()
            && !hook.name.starts_with("SessionStart")
            && !hook.name.starts_with("SubagentStart")
            && !crate::hooks::context_only(&hook.output))
}

/// `SessionManager.RunningElsewhere(id)`: no live session with a process for id, and id held by some claude.
pub fn running_elsewhere(summary: &SessionsSummary, id: &str) -> bool {
    !summary.sessions.iter().any(|s| s.id == id && s.has_process) && summary.held.iter().any(|h| h == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use serde_json::json;

    fn user(text: &str, uuid: Option<&str>) -> Item {
        Item::User(UserItem { text: text.into(), at: Default::default(), ultracode: false, images: None, uuid: uuid.map(Into::into) })
    }

    fn text(md: &str, uuid: Option<&str>) -> Item {
        Item::Text(TextItem { markdown: md.into(), parent_tool_use_id: None, uuid: uuid.map(Into::into), at: None })
    }

    fn tool_item(id: &str, name: &str, parent: Option<&str>) -> ToolItem {
        ToolItem {
            id: id.into(),
            name: name.into(),
            input: json!({}),
            parent_tool_use_id: parent.map(Into::into),
            state: ToolState::Running,
            result_text: None,
            structured: None,
            started_at: Default::default(),
            ended_at: None,
            task_id: None,
            task_status: None,
            progress: None,
            tokens: 0,
            sub_tool_uses: 0,
            background: false,
            model: None,
        }
    }

    #[test]
    fn haiku_title_cleaned() {
        assert_eq!(title_from("\n\"Pagination liste vols Steward #2520.\"\n").as_deref(), Some("Pagination liste vols Steward #2520"));
        assert_eq!(title_from(" \n"), None);
    }

    #[test]
    fn fork_history_cut() {
        let h = vec![user("a", None), text("b", Some("u1")), user("c", None), text("d", Some("u2"))];
        let cut = upto(h.clone(), Some("u1"));
        assert!(matches!(&cut[..], [Item::User(_), Item::Text(t)] if t.uuid.as_deref() == Some("u1")));
        assert_eq!(upto(h, None).len(), 4);
    }

    #[test]
    fn extra_dir_rules() {
        assert_eq!(extra_dir(" \"/x/y/\" ", "/w", &[], false).as_deref(), Some("/x/y"));
        assert_eq!(extra_dir("/w/", "/w", &[], false), None);
        assert_eq!(extra_dir("/A", "/w", &["/a".into()], false), None);
        assert_eq!(extra_dir("/A", "/w", &["/a".into()], true).as_deref(), Some("/A"));
        assert_eq!(extra_dir("  ", "/w", &[], false), None);
        assert_eq!(extra_dir("/", "/w", &[], false).as_deref(), Some("/"));
    }

    #[test]
    fn config_usage_keys() {
        // claude 2.1.296's answer to a bare /config (trimmed).
        let keys = config_keys("Usage: /config key=value [key=value ...]\n  autoCompact=true|false\n  language=<value>\n  switchModelsOnFlag=Switch automatically|Ask each time").unwrap();
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                ("autoCompact".into(), s(&["true", "false"])),
                ("language".into(), vec![]),
                ("switchModelsOnFlag".into(), s(&["Switch automatically", "Ask each time"]))
            ]
        );
        assert_eq!(config_keys("Set Language to French"), None);
    }

    #[test]
    fn effort_capping() {
        assert_eq!(capped(&["low", "medium", "high", "xhigh", "max"], Some("high")), vec!["low", "medium", "high"]);
        assert_eq!(capped(&["low", "high"], None).len(), 2);
        assert_eq!(capped(&["low", "high"], Some("auto")).len(), 2);
        let eff = json!({"maxEffortLevel":"high","modelSettings":{"claude-fable-5-1":{"maxEffortLevel":"max"},"claude-opus-5-5":{"maxEffortLevel":"medium"}}});
        assert_eq!(effort_cap(&eff, Some("claude-haiku-5-5")).as_deref(), Some("high"));
        assert_eq!(effort_cap(&eff, Some("claude-fable-5-1")), None);
        assert_eq!(effort_cap(&eff, Some("claude-opus-5-5[1m]")).as_deref(), Some("medium"));
        assert_eq!(effort_cap(&json!({}), Some("x")), None);
    }

    #[test]
    fn worktree_moves() {
        assert_eq!(worktree_move("EnterWorktree", None), None);
        assert_eq!(worktree_move("Bash", Some(&json!({"worktreePath":"/x"}))), None);
        assert_eq!(worktree_move("EnterWorktree", Some(&json!({"worktreePath":"/x"}))).as_deref(), Some("/x"));
        assert_eq!(worktree_move("ExitWorktree", Some(&json!({"originalCwd":"/o","restoredCwd":"/r"}))).as_deref(), Some("/r"));
        assert_eq!(worktree_move("ExitWorktree", Some(&json!({"originalCwd":"/o"}))).as_deref(), Some("/o"));
    }

    #[test]
    fn user_text_rules() {
        assert_eq!(user_text("<command-message>cost</command-message>\n<command-name>/cost</command-name>\n<command-args></command-args>").as_deref(), Some("/cost"));
        assert_eq!(user_text("<local-command-stdout>Set model</local-command-stdout>"), None);
        assert_eq!(user_text("<agent-message from=\"a\">\nreport\n</agent-message>"), None);
        assert_eq!(user_text("salut").as_deref(), Some("salut"));
        assert_eq!(
            user_text("<command-message>compact</command-message>\n<command-name>/compact</command-name>\n<command-args>keep tests</command-args>").as_deref(),
            Some("/compact keep tests")
        );
        assert_eq!(user_text("[Request interrupted by user]"), None);
    }

    #[test]
    fn rewind_steps_newest_first() {
        let items = vec![user("un", Some("u1")), text("ok", None), user("deux", Some("u2")), user("trois", Some("u3"))];
        assert_eq!(rewind_steps(&items, "u2"), vec!["u3", "u2"]);
        assert_eq!(rewind_steps(&items, "u1"), vec!["u3", "u2", "u1"]);
        assert_eq!(rewind_steps(&items, "u3"), vec!["u3"]);
        let meta = SessionMeta { sent: vec!["u1".into()], has_process: true, status: SessionStatus::Idle, ..Default::default() };
        assert!(can_rewind(&meta, &items, Some("u1")));
        assert!(!can_rewind(&meta, &items, Some("u2")), "not sent by this process");
        assert!(!can_rewind(&SessionMeta { has_process: false, ..meta.clone() }, &items, Some("u1")));
        let mut bg = tool_item("t", "Agent", None);
        bg.background = true;
        let mut with_bg = items.clone();
        with_bg.push(Item::Tool(bg));
        assert!(!can_rewind(&meta, &with_bg, Some("u1")), "a background agent still writes");
    }

    #[test]
    fn asking_agent_and_children() {
        let mut agent = tool_item("toolu_A", "Agent", None);
        agent.task_id = Some("aa5".into());
        agent.input = json!({"subagent_type":"general-purpose"});
        let child = tool_item("toolu_X", "Write", Some("toolu_A"));
        let items = vec![Item::Tool(agent), Item::Tool(child)];
        let p = |agent_id: Option<&str>, tuid: &str| PendingPermission {
            request_id: "r".into(),
            tool: "Write".into(),
            input: json!({}),
            description: None,
            tool_use_id: Some(tuid.into()),
            suggestions: None,
            at: Default::default(),
            info: Some(PermissionInfo { agent_id: agent_id.map(Into::into), ..Default::default() }),
            target_exists: None,
        };
        assert_eq!(asking_agent(&items, &p(Some("aa5"), "zz")).map(|t| t.id.as_str()), Some("toolu_A"));
        assert_eq!(asking_agent(&items, &p(Some("other"), "toolu_X")).map(|t| t.id.as_str()), Some("toolu_A"));
        assert!(asking_agent(&items, &p(None, "toolu_X")).is_none(), "main thread: no asking agent");
        assert_eq!(agent_label(items[0].as_tool().unwrap()), "general-purpose");
        assert_eq!(children(&items).get("toolu_A"), Some(&vec![1]));
    }

    #[test]
    fn hooks_shown() {
        let h = |name: &str, outcome: &str, output: &str| HookEvt { name: name.into(), outcome: outcome.into(), exit_code: Some(0), output: output.into(), id: None };
        assert!(!shown(&h("Stop", "success", "")));
        assert!(shown(&h("UserPromptSubmit", "success", "context")));
        assert!(!shown(&h("SessionStart:startup", "success", "plugin prompt")));
        assert!(shown(&h("PreToolUse:Write", "error", "nope")));
        assert!(!shown(&h("SubagentStart:x", "success", "plain")));
        let ctx = r#"{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"P"}}"#;
        assert!(!shown(&h("UserPromptSubmit", "success", ctx)));
        assert!(risky("auto") && risky("acceptEdits") && !risky("plan"));
    }

    #[test]
    fn elsewhere() {
        let s = SessionsSummary { held: vec!["a".into()], ..Default::default() };
        assert!(running_elsewhere(&s, "a"));
        assert!(!running_elsewhere(&s, "b"));
    }
}
