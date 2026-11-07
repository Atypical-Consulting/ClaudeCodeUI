//! Port of the records and pure parts of `Core/WorkflowRuns.cs` (ultracode `Workflow` tool runs) — spec 05 §4.
//! Owner: core-extensions. Disk reads (final file, journal, agent logs, path safety) are `ccui_core::workflow_runs`.

use crate::json;
use crate::model::Time;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

pub const TOOL: &str = "Workflow";

/// Unknown: no end recorded (transcript stops mid-run).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WfState {
    Waiting,
    Running,
    Done,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WfAgent {
    pub label: String,
    pub phase: String,
    pub state: WfState,
    pub model: Option<String>,
    pub tokens: i64,
    /// Rebuilt from agent-*.jsonl.
    pub approx: bool,
    pub tool_calls: i32,
    pub duration_ms: Option<i64>,
    pub agent_id: Option<String>,
    pub result: Option<String>,
    /// The CLI's raw state word when Failed (shown verbatim).
    pub raw_state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WfPhase {
    pub title: String,
    pub detail: Option<String>,
    pub total: i32,
    pub done: i32,
    pub running: i32,
    pub failed: i32,
}

impl WfPhase {
    /// Failed>0 → Failed; Running>0 || (0<Done<Total) → Running; Total>0 && Done==Total → Done; else Waiting.
    pub fn state(&self) -> WfState {
        if self.failed > 0 {
            WfState::Failed
        } else if self.running > 0 || (self.done > 0 && self.done < self.total) {
            WfState::Running
        } else if self.total > 0 && self.done == self.total {
            WfState::Done
        } else {
            WfState::Waiting
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub name: String,
    pub description: Option<String>,
    pub run_dir: Option<String>,
    pub phases: Vec<WfPhase>,
    pub agents: Vec<WfAgent>,
    pub tokens: i64,
    pub tool_calls: i32,
    pub elapsed_ms: i64,
    pub state: WfState,
    pub raw_status: Option<String>,
    pub model: Option<String>,
    /// Re-read once a second while live (Ticker).
    pub live: bool,
}

impl WorkflowRun {
    /// Count of agents Done.
    pub fn done(&self) -> usize {
        self.agents.iter().filter(|a| a.state == WfState::Done).count()
    }
}

/// `Meta(script, key)`: first `\b{key}\s*:\s*{Q}` after "meta" (Q = '…' | "…" | `…`, backslash escapes unescaped).
pub fn meta(script: &str, key: &str) -> Option<String> {
    let i = script.find("meta")?;
    let re = Regex::new(&format!(r"\b{}\s*:\s*{Q}", regex::escape(key))).ok()?;
    re.captures(&script[i..]).map(|m| unq(&m, 1))
}

/// Quoted JS string: single, double or backtick, with backslash escapes (groups n, n+1, n+2).
const Q: &str = r#"(?:'((?:\\.|[^'\\])*)'|"((?:\\.|[^"\\])*)"|`((?:\\.|[^`\\])*)`)"#;

fn unq(m: &regex::Captures, from: usize) -> String {
    unescape((from..from + 3).find_map(|g| m.get(g)).map(|g| g.as_str()).unwrap_or_default())
}

/// .NET `Regex.Unescape`: `\n \t \r \f \v \a \e \b`, `\xHH`, `\uHHHH`, octal `\NNN`; any other `\c` is `c`.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(e) = it.next() else {
            out.push('\\');
            break;
        };
        let hex = |it: &mut std::iter::Peekable<std::str::Chars>, n: usize| {
            let h: String = (0..n).filter_map(|_| it.next_if(|c| c.is_ascii_hexdigit())).collect();
            u32::from_str_radix(&h, 16).ok().and_then(char::from_u32)
        };
        match e {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'f' => out.push('\u{0C}'),
            'v' => out.push('\u{0B}'),
            'a' => out.push('\u{07}'),
            'e' => out.push('\u{1B}'),
            'b' => out.push('\u{08}'),
            'x' => out.extend(hex(&mut it, 2)),
            'u' => out.extend(hex(&mut it, 4)),
            '0'..='7' => {
                let mut v = e.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match it.next_if(|c| c.is_digit(8)) {
                        Some(d) => v = v * 8 + d.to_digit(8).unwrap_or(0),
                        None => break,
                    }
                }
                out.extend(char::from_u32(v));
            }
            other => out.push(other),
        }
    }
    out
}

/// `MetaPhases(script)`: `{ title: Q [, detail: Q] }` entries within 8 KB after "phases" (after "meta").
// ponytail: a regex over the JS literal, bounded to the meta block's first 8 KB; only a fallback until the .json exists.
pub fn meta_phases(script: &str) -> Vec<(String, Option<String>)> {
    let Some(i) = script.find("meta") else { return Vec::new() };
    let Some(p) = script[i..].find("phases").map(|p| p + i) else { return Vec::new() };
    let end = script[p..].char_indices().nth(8192).map(|(n, _)| p + n).unwrap_or(script.len());
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(&format!(r"\{{\s*title\s*:\s*{Q}\s*(?:,\s*detail\s*:\s*{Q}\s*)?,?\s*\}}")).expect("valid regex"));
    re.captures_iter(&script[p..end])
        .map(|m| (unq(&m, 1), (4..7).any(|g| m.get(g).is_some()).then(|| unq(&m, 4))))
        .collect()
}

fn items<'a>(snapshot: &'a Value, kind: &'a str) -> impl Iterator<Item = &'a Value> {
    snapshot.as_array().into_iter().flatten().filter(move |x| json::str(x, "type") == Some(kind))
}

/// Titles of `workflow_phase` items of a snapshot.
pub fn phase_titles(snapshot: &Value) -> Vec<String> {
    items(snapshot, "workflow_phase").map(|x| json::string(x, "title")).collect()
}

/// `workflow_agent` items of a snapshot → agents (done → Done; start → Waiting/Running; else Failed + raw word).
pub fn agents(snapshot: &Value, now: Time) -> Vec<WfAgent> {
    items(snapshot, "workflow_agent")
        .map(|a| {
            let started = json::num(a, "startedAt");
            let raw = json::str(a, "state");
            // Seen: "start" (queued until it has a startedAt) and "done". Anything else is shown with its own word.
            let state = match raw {
                Some("done") => WfState::Done,
                Some("start") if started.is_none() => WfState::Waiting,
                Some("start") => WfState::Running,
                _ => WfState::Failed,
            };
            let duration_ms = json::num(a, "durationMs").or_else(|| match (state, started) {
                (WfState::Running, Some(s)) => Some(now.timestamp_millis() - s),
                _ => None,
            });
            WfAgent {
                label: json::string(a, "label"),
                phase: json::string(a, "phaseTitle"),
                state,
                model: json::str(a, "model").map(str::to_string),
                tokens: json::num(a, "tokens").unwrap_or(0),
                approx: false,
                tool_calls: json::num(a, "toolCalls").unwrap_or(0) as i32,
                duration_ms,
                agent_id: json::str(a, "agentId").map(str::to_string),
                result: json::str(a, "resultPreview").map(str::to_string),
                raw_state: if state == WfState::Failed { raw.map(str::to_string) } else { None },
            }
        })
        .collect()
}

/// The text after the line starting `Transcript dir:` in a receipt text.
pub fn receipt_line(text: Option<&str>, prefix: &str) -> Option<String> {
    text?.split('\n').find(|l| l.starts_with(prefix)).map(|l| l[prefix.len()..].trim().to_string())
}

/// Agent id usable as a file name: `^[A-Za-z0-9_-]{1,80}$`.
pub fn valid_agent_id(id: &str) -> bool {
    (1..=80).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Trimmed `{…}` / `[…]` that parses → Some(JSON) (shown pretty-printed), else None.
pub fn json_result(s: &str) -> Option<Value> {
    if !s.trim_start().starts_with(['{', '[']) {
        return None;
    }
    serde_json::from_str(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn json_results() {
        assert!(json_result(" {\"ok\":true}").is_some_and(|v| v.is_object()));
        assert!(json_result("[1]").is_some());
        assert!(json_result("pong").is_none() && json_result("{not json").is_none() && json_result("").is_none());
    }

    #[test]
    fn snapshot_agents() {
        let now = chrono::Utc.timestamp_millis_opt(1791626222733).unwrap();
        // claude 2.1.296 live snapshot (task_progress.workflow_progress), trimmed: pong:a done, pong:b queued.
        let snap: Value = serde_json::from_str(r#"[{"type":"workflow_phase","index":1,"title":"Ping"},{"type":"workflow_agent","index":1,"label":"pong:a","phaseIndex":1,"phaseTitle":"Ping","agentId":"aea012a4f32b49114","model":"claude-haiku-5-5","state":"done","startedAt":1791626220733,"queuedAt":1791626220731,"attempt":1,"tokens":24538,"toolCalls":0,"durationMs":2045,"resultPreview":"pong"},{"type":"workflow_agent","index":2,"label":"pong:b","phaseIndex":1,"phaseTitle":"Ping","model":"claude-haiku-5-5","state":"start","queuedAt":1791626220731}]"#).unwrap();
        let a = agents(&snap, now);
        assert_eq!(a.len(), 2);
        assert!(a[0].label == "pong:a" && a[0].phase == "Ping" && a[0].state == WfState::Done && a[0].tokens == 24538);
        assert!(a[0].result.as_deref() == Some("pong") && a[0].model.as_deref() == Some("claude-haiku-5-5") && a[0].duration_ms == Some(2045));
        assert!(a[1].state == WfState::Waiting && a[1].agent_id.is_none() && a[1].duration_ms.is_none());
        assert_eq!(phase_titles(&snap), ["Ping"]);
        let running = serde_json::json!([{"type":"workflow_agent","state":"start","startedAt":1791626220733i64},{"type":"workflow_agent","state":"odd"}]);
        let r = agents(&running, now);
        assert!(r[0].state == WfState::Running && r[0].duration_ms == Some(2000));
        assert!(r[1].state == WfState::Failed && r[1].raw_state.as_deref() == Some("odd"));
    }

    #[test]
    fn script_meta() {
        let script = "export const meta = { name: 'pong-probe', description: 'Two agents \\'reply\\' pong', phases: [{ title: 'Ping', detail: 'two pong agents' }, { title: \"Check\" }] }\nphase('Ping')";
        assert_eq!(meta(script, "name").as_deref(), Some("pong-probe"));
        assert_eq!(meta(script, "description").as_deref(), Some("Two agents 'reply' pong"));
        assert_eq!(meta("phase('x')", "name"), None);
        assert_eq!(meta_phases(script), vec![("Ping".to_string(), Some("two pong agents".to_string())), ("Check".to_string(), None)]);
        assert!(meta_phases("phase('x')").is_empty());
        assert_eq!(unescape(r"a\nb\x41é\\\q"), "a\nbAé\\q");
    }

    #[test]
    fn misc() {
        assert_eq!(
            receipt_line(Some("Workflow launched in background. Task ID: w0\nTranscript dir: /r/wf_1\nScript file: /s"), "Transcript dir:").as_deref(),
            Some("/r/wf_1")
        );
        assert_eq!(receipt_line(None, "x"), None);
        assert!(valid_agent_id("a1b2") && valid_agent_id("a-_Z") && !valid_agent_id("../x") && !valid_agent_id("") && !valid_agent_id(&"a".repeat(81)));
        let p = |total, done, running, failed| WfPhase { title: String::new(), detail: None, total, done, running, failed };
        assert_eq!(p(2, 1, 0, 0).state(), WfState::Running);
        assert_eq!(p(2, 2, 0, 0).state(), WfState::Done);
        assert_eq!(p(2, 2, 0, 1).state(), WfState::Failed);
        assert_eq!(p(0, 0, 0, 0).state(), WfState::Waiting);
        assert_eq!(p(2, 0, 1, 0).state(), WfState::Running);
    }
}
