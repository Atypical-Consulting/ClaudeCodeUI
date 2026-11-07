//! Port of `TranscriptStore.Load / Replay / ActivePrompts / Apply / Step` — rebuild a thread from a transcript
//! through the same parser and reducer as a live session — spec 03 §1.11-1.12. Owner: core-store-sessions.
//! One bad line never loses the session (catch-all, logged `transcript: line skipped (…)`).
//! Tests: the replay fixtures of `TranscriptStore.Check()` verbatim.

use crate::events::{self, ClaudeEvent};
use crate::live::state::SessionState;
use crate::transcript_store::hidden;
use ccui_shared::json::{self, prop, str as jstr};
use ccui_shared::model::{Item, Time, ToolState};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Items of transcript `id` (Find → read lines → replay); unknown id → [].
pub fn load(id: &str) -> Vec<Item> {
    let Some(path) = crate::transcript_store::find(id) else { return Vec::new() };
    match crate::transcript_store::read_text(&path) {
        Ok(t) => replay(&t.lines().collect::<Vec<_>>()),
        Err(_) => Vec::new(),
    }
}

/// The system lines the thread shows (compaction boundary, model fallback notices), and messages.
fn shown(e: &Value) -> bool {
    match jstr(e, "type") {
        Some("user" | "assistant") => true,
        Some("system") => matches!(
            jstr(e, "subtype"),
            Some(
                "compact_boundary"
                    | "model_fallback"
                    | "model_refusal_fallback"
                    | "model_refusal_no_fallback"
                    | "model_consent_fallback"
            )
        ),
        _ => false,
    }
}

/// Replay raw .jsonl lines (prefilter, `toolUseResult` → `tool_use_result`, rewind chain, attachments, Shown/Hidden,
/// timestamps), then unfinished Running/Waiting tools → Error with ended_at = ended_at ?? started_at.
pub fn replay(lines: &[&str]) -> Vec<Item> {
    let active = active_prompts(lines);
    let mut keep = true; // the last prompt before this line is on the active branch
    let mut s = SessionState::new("", "", "", "default", chrono::Utc::now());
    for line in lines {
        if !line.contains("\"type\":\"user\"")
            && !line.contains("\"type\":\"assistant\"")
            && !line.contains("\"queued_command\"")
            && !line.contains("\"type\":\"system\"")
        {
            continue;
        }
        // The file names the diff field toolUseResult; the stream (and Events) say tool_use_result.
        let Ok(e) = serde_json::from_str::<Value>(&line.replace("\"toolUseResult\":", "\"tool_use_result\":")) else {
            continue;
        };
        if let (Some(active), true, Some(id)) = (&active, is_prompt(&e), jstr(&e, "uuid")) {
            keep = active.contains(id);
        }
        if !keep {
            continue;
        }
        let at = json::time(&e);
        if json::bool(&e, "isSidechain") {
            continue;
        }
        // A message folded mid-turn is written as this attachment (CLI 2.1.296); a background agent's
        // task-notification can arrive the same way.
        if jstr(&e, "type") == Some("attachment") {
            if let Some(qa) = prop(&e, "attachment") {
                if jstr(qa, "type") == Some("queued_command") && matches!(jstr(qa, "commandMode"), Some("prompt" | "task-notification")) {
                    if let Some(qp) = jstr(qa, "prompt") {
                        let ev = events::notification(qp)
                            .map(ClaudeEvent::TaskDone)
                            .unwrap_or_else(|| ClaudeEvent::UserText { text: qp.to_string(), at, images: None });
                        guarded(|| step(&mut s, &ev, at));
                        continue;
                    }
                }
            }
        }
        // The compaction summary fills its "Compacted chat" card; a scheduled task's prompt shows as its marker.
        if !shown(&e) || (hidden(&e) && !json::bool(&e, "isCompactSummary") && jstr(&e, "turnOrigin") != Some("scheduled")) {
            continue;
        }
        guarded(|| apply(&mut s, &e, at));
    }
    // No result in the file: the turn was cut short.
    s.items()
        .iter()
        .map(|i| match i {
            Item::Tool(t) if matches!(t.state, ToolState::Running | ToolState::Waiting) => {
                let mut t = t.clone();
                t.state = ToolState::Error;
                t.ended_at = t.ended_at.or(Some(t.started_at));
                Item::Tool(t)
            }
            i => i.clone(),
        })
        .collect()
}

/// One bad line must not lose the session.
fn guarded(f: impl FnOnce()) {
    if let Err(p) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
        eprintln!("transcript: line skipped ({msg})");
    }
}

/// None when no line contains `"rewound":true`; else the uuids on the chain from the last leaf.
pub fn active_prompts(lines: &[&str]) -> Option<HashSet<String>> {
    if !lines.iter().any(|l| l.contains("\"rewound\":true")) {
        return None;
    }
    let mut parent: HashMap<String, Option<String>> = HashMap::new();
    let mut leaf: Option<String> = None;
    for line in lines {
        if !line.contains("\"uuid\":") && !line.contains("\"rewound\":true") {
            continue;
        }
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if jstr(&e, "type") == Some("last-prompt") {
            if json::bool(&e, "rewound") {
                leaf = jstr(&e, "leafUuid").map(str::to_string);
            }
        } else if let Some(id) = jstr(&e, "uuid").filter(|_| !json::bool(&e, "isSidechain")) {
            parent.insert(id.to_string(), jstr(&e, "parentUuid").or_else(|| jstr(&e, "logicalParentUuid")).map(str::to_string));
            leaf = Some(id.to_string());
        }
    }
    let mut chain = HashSet::new();
    let mut id = leaf;
    while let Some(i) = id {
        if !chain.insert(i.clone()) {
            break;
        }
        id = parent.get(&i).cloned().flatten();
    }
    Some(chain)
}

/// Every event of one parsed line through `step` (exported: WorkflowRuns' agent log uses it).
pub fn apply(s: &mut SessionState, line: &Value, at: Option<Time>) {
    for ev in events::parse_all(line) {
        step(s, &ev, at);
    }
}

/// Reduce, then timestamp fix-ups (tool started_at, ended_at; TaskDone without duration).
pub fn step(s: &mut SessionState, ev: &ClaudeEvent, at: Option<Time>) {
    s.reduce(ev, at.unwrap_or_else(chrono::Utc::now));
    let Some(at) = at else { return };
    let fix = |s: &mut SessionState, i: Option<usize>, f: &dyn Fn(&mut ccui_shared::model::ToolItem)| {
        if let Some(i) = i {
            if let Some(Item::Tool(t)) = s.items().get(i) {
                let mut t = t.clone();
                f(&mut t);
                s.set_item(i, Item::Tool(t));
            }
        }
    };
    match ev {
        ClaudeEvent::ToolUse { id, .. } => {
            let i = s.tool_index(id);
            fix(s, i, &|t| t.started_at = at);
        }
        ClaudeEvent::ToolResult { tool_use_id, .. } => {
            let i = s.tool_index(tool_use_id);
            fix(s, i, &|t| t.ended_at = if t.background { None } else { Some(at) });
        }
        // No duration tag: the timestamp decides.
        ClaudeEvent::TaskDone(td) if td.duration_ms == 0 => {
            let i = s.tool_index(&td.tool_use_id).or_else(|| {
                s.items().iter().rposition(|x| x.as_tool().is_some_and(|t| t.task_id.as_deref() == Some(td.task_id.as_str())))
            });
            fix(s, i, &|t| t.ended_at = Some(at));
        }
        _ => {}
    }
}

/// type user, not meta/sidechain, has content that is a string or an array with no tool_result.
pub fn is_prompt(line: &Value) -> bool {
    if jstr(line, "type") != Some("user") || json::bool(line, "isMeta") || json::bool(line, "isSidechain") {
        return false;
    }
    match prop(line, "message").and_then(|m| prop(m, "content")) {
        Some(Value::String(_)) => true,
        Some(Value::Array(a)) => !a.iter().any(|b| jstr(b, "type") == Some("tool_result")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccui_shared::model::*;
    use serde_json::json;

    #[test]
    fn rewind_chain() {
        let branched = branched();
        let lines: Vec<&str> = branched.iter().map(String::as_str).collect();
        assert!(active_prompts(&lines).is_none(), "no rewind: replay whole");
        let mut cut = lines.clone();
        cut.push(CUT);
        let chain = active_prompts(&cut).unwrap();
        assert!(chain.contains("u1") && chain.contains("cb") && chain.contains("a2") && !chain.contains("u2"));
        let mut first = lines.clone();
        first.push(r#"{"type":"last-prompt","lastPrompt":"un","explicit":true,"rewound":true,"sessionId":"s"}"#);
        assert!(active_prompts(&first).unwrap().is_empty());
        let cyc = [r#"{"uuid":"x","parentUuid":"y"}"#, r#"{"uuid":"y","parentUuid":"x"}"#, r#"{"type":"last-prompt","rewound":true,"leafUuid":"x"}"#];
        assert_eq!(active_prompts(&cyc).unwrap().len(), 2, "a cycle stops the walk");
    }

    #[test]
    fn prompts_detected() {
        assert!(is_prompt(&json!({"type":"user","message":{"content":"x"}})));
        assert!(is_prompt(&json!({"type":"user","message":{"content":[{"type":"text","text":"x"}]}})));
        assert!(!is_prompt(&json!({"type":"user","message":{"content":[{"type":"tool_result"}]}})));
        assert!(!is_prompt(&json!({"type":"user","isMeta":true,"message":{"content":"x"}})));
        assert!(!is_prompt(&json!({"type":"assistant","message":{"content":"x"}})));
    }

    const CUT: &str = r#"{"type":"last-prompt","lastPrompt":"deux","leafUuid":"cb","explicit":true,"rewound":true,"sessionId":"s"}"#;

    fn branched() -> Vec<String> {
        [
            r#"{"parentUuid":null,"type":"user","message":{"role":"user","content":"un"},"uuid":"u1"}"#,
            r#"{"parentUuid":"u1","type":"assistant","message":{"id":"m1","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]},"uuid":"a1"}"#,
            r#"{"parentUuid":"a1","type":"assistant","message":{"id":"m1","role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Read","input":{}}]},"uuid":"a1b"}"#,
            r#"{"parentUuid":"a1b","type":"user","message":{"role":"user","content":[{"tool_use_id":"t2","type":"tool_result","content":"ok"}]},"uuid":"r2"}"#,
            r#"{"parentUuid":"a1","type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"ok"}]},"uuid":"r1"}"#,
            r#"{"parentUuid":"r1","type":"assistant","message":{"id":"m2","role":"assistant","content":[{"type":"text","text":"OK"}]},"uuid":"a2"}"#,
            r#"{"parentUuid":null,"logicalParentUuid":"a2","type":"system","subtype":"compact_boundary","uuid":"cb"}"#,
            r#"{"parentUuid":"cb","type":"user","message":{"role":"user","content":"deux"},"uuid":"u2"}"#,
            r#"{"parentUuid":"u2","type":"assistant","message":{"id":"m3","role":"assistant","content":[{"type":"tool_use","id":"t3","name":"Write","input":{}}]},"uuid":"b2"}"#,
        ]
        .map(String::from)
        .to_vec()
    }

    fn run(lines: &[&str]) -> Vec<Item> {
        replay(lines)
    }

    fn tool(i: &Item) -> &ToolItem {
        i.as_tool().expect("a ToolItem")
    }

    fn secs(a: Time, b: Option<Time>) -> i64 {
        (b.unwrap() - a).num_milliseconds()
    }

    // The reducer cases below need core-live-session's `SessionState::{new, reduce}`; run with
    // `cargo test -p ccui-core -- --include-ignored` once that unit is merged.

    #[test]
    #[ignore = "needs core-live-session reducer"]
    fn replay_basic() {
        let items = run(&[
            r#"{"type":"permission-mode","permissionMode":"default","sessionId":"s"}"#,
            r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"<command-name>/x</command-name>"},"cwd":"C:\\repo\\api","gitBranch":"main","timestamp":"2026-10-09T12:00:00Z"}"#,
            r#"{"parentUuid":"a","isSidechain":false,"type":"user","message":{"role":"user","content":"corrige   le test\nd'auth"},"cwd":"C:\\repo\\api","gitBranch":"main","timestamp":"2026-10-09T12:00:01Z"}"#,
            r#"{"parentUuid":"b","isSidechain":false,"type":"assistant","message":{"model":"claude-haiku-5-5","id":"m1","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"C:\\repo\\api\\a.txt","old_string":"a","new_string":"b"}}]},"timestamp":"2026-10-09T12:00:02Z"}"#,
            r#"{"parentUuid":"c","isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"ok"}]},"toolUseResult":{"filePath":"C:\\repo\\api\\a.txt","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-a","+b"]}]},"timestamp":"2026-10-09T12:00:04Z"}"#,
            r#"{"parentUuid":"d","isSidechain":false,"type":"assistant","message":{"model":"claude-haiku-5-5","id":"m2","role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"C:\\repo\\api\\b.txt"}}]},"timestamp":"2026-10-09T12:00:05Z"}"#,
            r#"{"type":"cost-state","sessionId":"s","totalCostUSD":0.66}"#,
        ]);
        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], Item::User(_)) && matches!(items[1], Item::User(_)));
        let (t1, t2) = (tool(&items[2]), tool(&items[3]));
        assert!(t1.state == ToolState::Done && t1.structured.is_some() && secs(t1.started_at, t1.ended_at) == 2000);
        assert!(t2.state == ToolState::Error && t2.ended_at == Some(t2.started_at));
    }

    #[test]
    #[ignore = "needs core-live-session reducer"]
    fn replay_compaction() {
        let items = run(&[
            r#"{"parentUuid":"b","isSidechain":false,"type":"system","subtype":"stop_hook_summary","hookCount":2,"level":"suggestion","uuid":"e464"}"#,
            r#"{"parentUuid":null,"logicalParentUuid":"e464","isSidechain":false,"type":"system","subtype":"compact_boundary","content":"Conversation compacted","isMeta":false,"timestamp":"2026-10-11T00:26:19.756Z","uuid":"9b6e","level":"info","compactMetadata":{"trigger":"manual","preTokens":37317,"durationMs":3207,"postTokens":8995}}"#,
            r#"{"parentUuid":"9b6e","isSidechain":false,"type":"user","message":{"role":"user","content":"This session is being continued from a previous conversation that ran out of context. Summary: hi"},"isVisibleInTranscriptOnly":true,"isCompactSummary":true,"uuid":"ec30","timestamp":"2026-10-11T00:26:19.754Z"}"#,
            r#"{"parentUuid":"ec30","isSidechain":false,"type":"user","message":{"role":"user","content":"<command-name>/compact</command-name>\n            <command-message>compact</command-message>\n            <command-args></command-args>"},"uuid":"c0c1","timestamp":"2026-10-11T00:26:19.990Z"}"#,
            r#"{"parentUuid":"c0c1","isSidechain":false,"type":"user","message":{"role":"user","content":"<local-command-stdout>Compacted </local-command-stdout>"},"uuid":"aed4","timestamp":"2026-10-11T00:26:19.997Z"}"#,
        ]);
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Item::User(u) if u.text == "/compact"));
        assert!(matches!(&items[1], Item::Compact(c) if c.pre_tokens == 37317 && c.post_tokens == 8995 && c.summary.as_deref().is_some_and(|s| s.ends_with("Summary: hi"))));
    }

    const BG0: &str = r#"{"type":"assistant","message":{"id":"m","role":"assistant","content":[{"type":"tool_use","id":"toolu_A","name":"Agent","input":{}}]},"timestamp":"2026-10-09T12:00:00Z"}"#;
    const BG1: &str = r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_A","type":"tool_result","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"aa5"},"timestamp":"2026-10-09T12:00:01Z"}"#;

    #[test]
    #[ignore = "needs core-live-session reducer"]
    fn replay_misc() {
        let bg = run(&[BG0, BG1, r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>aa5</task-id>\n<tool-use-id>toolu_A</tool-use-id>\n<status>completed</status>\n<result>pong</result>\n<usage><subagent_tokens>31599</subagent_tokens></usage>\n</task-notification>"},"timestamp":"2026-10-09T12:00:05Z"}"#]);
        assert_eq!(bg.len(), 1);
        let a = tool(&bg[0]);
        assert!(a.state == ToolState::Done && a.result_text.as_deref() == Some("pong") && a.tokens == 31599 && secs(a.started_at, a.ended_at) == 5000);

        let fold = run(&[
            r#"{"type":"assistant","message":{"id":"m","role":"assistant","content":[{"type":"tool_use","id":"toolu_B","name":"Bash","input":{"command":"sleep 3; echo one"}}]},"timestamp":"2026-10-09T22:00:53.000Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_B","type":"tool_result","content":"one"}]},"timestamp":"2026-10-09T22:00:56.100Z"}"#,
            r#"{"parentUuid":"644fcf74-989d-49d2-85dd-a53edb85c9f0","isSidechain":false,"attachment":{"type":"queued_command","prompt":"Reply with exactly the word PINEAPPLE.","source_uuid":"f2b3dcee-0514-4a2f-850e-255f0515e69a","delivery_id":"8d074104-35d2-4b55-875a-8546782a5e81","commandMode":"prompt","timestamp":"2026-10-09T22:00:52.990Z"},"type":"attachment","uuid":"70d92a4a-e751-4a97-bbe8-9a670ac4819e","timestamp":"2026-10-09T22:00:52.990Z","renderedRole":"system","sessionId":"c682d2a7-66b5-4452-a11d-ee02b7f1589e","version":"2.1.296"}"#,
            r#"{"parentUuid":"x","isSidechain":false,"type":"queue-operation","operation":"remove","content":"Reply with exactly the word PINEAPPLE.","reason":"absorbed_mid_turn"}"#,
            r#"{"type":"assistant","message":{"id":"m2","role":"assistant","content":[{"type":"text","text":"PINEAPPLE"}]},"timestamp":"2026-10-09T22:00:57.000Z"}"#,
        ]);
        assert_eq!(fold.len(), 3);
        assert!(tool(&fold[0]).state == ToolState::Done);
        assert!(matches!(&fold[1], Item::User(u) if u.text == "Reply with exactly the word PINEAPPLE."));
        assert!(matches!(fold[2], Item::Text(_)));

        let api = run(&[r#"{"type":"assistant","isApiErrorMessage":true,"message":{"model":"<synthetic>","id":"m3","role":"assistant","content":[{"type":"text","text":"API Error: Output blocked by content filtering policy"}]},"timestamp":"2026-10-09T12:00:00Z"}"#]);
        assert!(matches!(api[..], [Item::ApiError(_)]));

        let todo = run(&[
            r#"{"type":"assistant","message":{"id":"m","role":"assistant","content":[{"type":"tool_use","id":"toolu_C","name":"TaskCreate","input":{"subject":"alpha","description":"Task alpha"}}]},"timestamp":"2026-10-09T12:00:00Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_C","type":"tool_result","content":"Task #1 created successfully: alpha"}]},"toolUseResult":{"task":{"id":"1","subject":"alpha"}},"timestamp":"2026-10-09T12:00:01Z"}"#,
            r#"{"type":"assistant","message":{"id":"m2","role":"assistant","content":[{"type":"tool_use","id":"toolu_U","name":"TaskUpdate","input":{"taskId":"1","status":"in_progress"}}]},"timestamp":"2026-10-09T12:00:02Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_U","type":"tool_result","content":"Updated task #1 status"}]},"toolUseResult":{"success":true,"taskId":"1","updatedFields":["status"],"statusChange":{"from":"pending","to":"in_progress"}},"timestamp":"2026-10-09T12:00:03Z"}"#,
        ]);
        let list = ccui_shared::todo_list::from(&todo);
        assert!(list.len() == 1 && list[0].key == "1" && list[0].content == "alpha" && list[0].status == ccui_shared::todo_list::TodoStatus::InProgress);

        let bgq = run(&[
            BG0,
            BG1,
            r#"{"isSidechain":false,"attachment":{"type":"queued_command","prompt":"<task-notification>\n<task-id>aa5</task-id>\n<tool-use-id>toolu_A</tool-use-id>\n<status>completed</status>\n<result>pong</result>\n<usage><duration_ms>5000</duration_ms></usage>\n</task-notification>","commandMode":"task-notification"},"type":"attachment","timestamp":"2026-10-09T12:00:09Z"}"#,
            r#"{"isSidechain":false,"attachment":{"type":"queued_command","prompt":"<agent-message from=\"aa5\">\n[Subagent hand-back] report\n</agent-message>","commandMode":"prompt"},"type":"attachment","timestamp":"2026-10-09T12:00:09Z"}"#,
        ]);
        assert!(bgq.len() == 1 && tool(&bgq[0]).state == ToolState::Done && tool(&bgq[0]).result_text.as_deref() == Some("pong"));

        let ask = run(&[
            r#"{"type":"assistant","message":{"id":"m","role":"assistant","content":[{"type":"tool_use","id":"toolu_Q","name":"AskUserQuestion","input":{"questions":[{"question":"Which color do you prefer?","header":"Color","options":[{"label":"Red","description":"Choose red"},{"label":"Blue","description":"Choose blue"}],"multiSelect":false}]}}]},"timestamp":"2026-10-09T12:00:00Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"The user answered: \"Which color do you prefer?\"=\"Chartreuse\".","tool_use_id":"toolu_Q"}]},"toolUseResult":{"questions":[{"question":"Which color do you prefer?","header":"Color","options":[{"label":"Red","description":"Choose red"},{"label":"Blue","description":"Choose blue"}],"multiSelect":false}],"answers":{"Which color do you prefer?":"Chartreuse"}},"timestamp":"2026-10-09T12:00:09Z"}"#,
        ]);
        let q = tool(&ask[0]);
        assert_eq!(q.state, ToolState::Done);
        assert_eq!(ccui_shared::ask_user::answered(q.structured.as_ref()), [("Which color do you prefer?".to_string(), "Chartreuse".to_string())]);
    }

    #[test]
    #[ignore = "needs core-live-session reducer"]
    fn replay_workflow() {
        let wf = [
            r#"{"type":"assistant","message":{"id":"m","role":"assistant","content":[{"type":"tool_use","id":"toolu_W","name":"Workflow","input":{"script":"export const meta = { name: 'p', description: 'd', phases: [{ title: 'Ping' }] }"}}]},"timestamp":"2026-10-10T09:00:00Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_W","type":"tool_result","content":"Workflow launched in background. Task ID: wjft8xn1u"}]},"toolUseResult":{"status":"async_launched","taskId":"wjft8xn1u","taskType":"local_workflow","workflowName":"impeccable-v020-screen-review","runId":"wf_a6cc7d95-465","summary":"Capture every screen","transcriptDir":"/nowhere/subagents/workflows/wf_a6cc7d95-465"},"timestamp":"2026-10-10T09:00:01Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>wjft8xn1u</task-id>\n<tool-use-id>toolu_W</tool-use-id>\n<status>completed</status>\n<summary>Dynamic workflow \"Capture every screen\" completed</summary>\n<result>{\"ok\":1}</result>\n<usage><agent_count>2</agent_count><agents_done>2</agents_done><agents_error>0</agents_error><subagent_tokens>49076</subagent_tokens><tool_uses>3</tool_uses><duration_ms>3493</duration_ms></usage>\n</task-notification>"},"origin":{"kind":"task-notification","producer":"session-task","runId":"0mv248zke-2d993936"},"turnOrigin":"task_notification","timestamp":"2026-10-10T09:10:01Z"}"#,
        ];
        let items = run(&wf);
        let w = tool(&items[0]);
        assert!(items.len() == 1 && w.state == ToolState::Done && w.task_status.as_deref() == Some("completed") && w.tokens == 49076 && w.sub_tool_uses == 3);
        assert_eq!(secs(w.started_at, w.ended_at), 3493);
        let cut = run(&wf[..2]);
        assert!(cut.len() == 1 && tool(&cut[0]).state == ToolState::Error && tool(&cut[0]).task_status.is_none());
    }

    #[test]
    #[ignore = "needs core-live-session reducer"]
    fn replay_rewinds() {
        let b = branched();
        let lines: Vec<&str> = b.iter().map(String::as_str).collect();
        let whole = run(&lines);
        assert!(matches!(&whole[whole.len() - 2], Item::User(u) if u.text == "deux") && tool(&whole[whole.len() - 1]).id == "t3");
        let mut cut = lines.clone();
        cut.push(CUT);
        let items = run(&cut);
        assert_eq!(items.len(), 4);
        assert!(matches!(&items[0], Item::User(u) if u.text == "un"));
        assert!(tool(&items[1]).id == "t1" && tool(&items[1]).state == ToolState::Done);
        assert!(tool(&items[2]).id == "t2" && tool(&items[2]).state == ToolState::Done);
        assert!(matches!(&items[3], Item::Text(t) if t.markdown == "OK"));
        cut.push(r#"{"parentUuid":"cb","type":"user","message":{"role":"user","content":"trois"},"uuid":"u3"}"#);
        let items = run(&cut);
        assert!(items.len() == 5 && matches!(&items[4], Item::User(u) if u.text == "trois"));
        let mut first = lines.clone();
        first.push(r#"{"type":"last-prompt","lastPrompt":"un","explicit":true,"rewound":true,"sessionId":"s"}"#);
        assert!(run(&first).is_empty());
    }
}
