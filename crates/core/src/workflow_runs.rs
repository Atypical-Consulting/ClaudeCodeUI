//! Port of the disk side of `Core/WorkflowRuns.cs` — read a Workflow run from its receipt, final file, live
//! snapshot or journal + agent logs; path safety; incremental tails — spec 05 §4. Owner: core-extensions.
//! Pure parts (meta, phases, snapshot agents, json_result) are `ccui_shared::workflow`.
//!
//! Sources, best first (claude 2.1.295/296): `<session>/workflows/<runId>.json`, written at completion only; the live
//! snapshot (`ToolItem.progress`); while running with no snapshot (a transcript opened from disk),
//! `subagents/workflows/<runId>/journal.jsonl` + `agent-<id>.jsonl`, read incrementally. The run folder comes from the
//! launch receipt (`tool_use_result.transcriptDir`, or the "Transcript dir:" line), never built from ids.

use crate::live::state::SessionState;
use crate::{paths, replay, transcript_store};
use ccui_shared::api::WorkflowAgentLog;
use ccui_shared::json;
use ccui_shared::model::{Item, Time, ToolItem, ToolState};
use ccui_shared::workflow::{self as wf, WfAgent, WfPhase, WfState, WorkflowRun};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::sync::{Arc, LazyLock};

type Cache<K, V> = LazyLock<Mutex<HashMap<K, V>>>;

/// Per tool id: (read at, the ToolItem read, the run). ToolItem is replaced on every change, so equality is the
/// revision. Kept 500 ms: each card or panel asks twice per render, which would otherwise stat and tail the folder.
static RECENT: Cache<String, (Time, ToolItem, WorkflowRun)> = LazyLock::new(Default::default);

/// `Read(tool, now)` (spec 05 §4 algorithm, exact). Cached per (tool id, ended_at/progress revision) for 500 ms.
pub fn read(tool: &ToolItem, now: Time) -> WorkflowRun {
    if let Some((at, t, run)) = RECENT.lock().get(&tool.id) {
        let age = (now - *at).num_milliseconds();
        if t == tool && (0..500).contains(&age) {
            return run.clone();
        }
    }
    let run = compute(tool, now);
    RECENT.lock().insert(tool.id.clone(), (now, tool.clone(), run.clone()));
    run
}

/// The fields of the final `.json` that `read` uses.
struct FinalRun {
    status: Option<String>,
    duration_ms: Option<i64>,
    tokens: Option<i64>,
    tool_calls: Option<i64>,
    phases: Vec<(String, Option<String>)>,
    progress: Option<Value>,
    model: Option<String>,
}

impl FinalRun {
    fn of(r: &Value) -> FinalRun {
        FinalRun {
            status: json::str(r, "status").map(str::to_string),
            duration_ms: json::num(r, "durationMs"),
            tokens: json::num(r, "totalTokens"),
            tool_calls: json::num(r, "totalToolCalls"),
            phases: match json::prop(r, "phases") {
                Some(Value::Array(ps)) => ps.iter().map(|x| (json::string(x, "title"), json::str(x, "detail").map(str::to_string))).collect(),
                _ => Vec::new(),
            },
            progress: json::prop(r, "workflowProgress").filter(|p| p.is_array()).cloned(),
            model: json::str(r, "defaultModel").map(str::to_string),
        }
    }
}

fn compute(t: &ToolItem, now: Time) -> WorkflowRun {
    let receipt = t.structured.as_ref().unwrap_or(&json::UNDEFINED);
    let script = json::str(&t.input, "script");
    let dir_text = json::str(receipt, "transcriptDir").map(str::to_string).or_else(|| wf::receipt_line(t.result_text.as_deref(), "Transcript dir:"));
    let dir = inside(dir_text.as_deref());
    let fin = dir.as_deref().and_then(final_file).map(|v| FinalRun::of(&v));

    let status = fin.as_ref().and_then(|f| f.status.clone()).or_else(|| t.task_status.clone()); // the .json, else the task-notification
    let state = match status.as_deref() {
        Some("completed") => WfState::Done,
        Some(_) => WfState::Failed, // never observed: the raw word is shown
        None => match t.state {
            ToolState::Running => WfState::Running,
            ToolState::Waiting => WfState::Waiting,
            ToolState::Done => WfState::Done,
            ToolState::Error if t.result_text.is_none() => WfState::Unknown, // launched, then the transcript or process stopped
            _ => WfState::Failed, // the tool call itself failed or was denied
        },
    };

    let snapshot = fin.as_ref().and_then(|f| f.progress.as_ref()).or(t.progress.as_ref()).filter(|s| s.is_array());
    let agents = match snapshot {
        Some(snap) => wf::agents(snap, now),
        None => dir
            .as_deref()
            .and_then(|d| journal(d, !matches!(state, WfState::Running | WfState::Unknown), now))
            .unwrap_or_default(),
    };

    // Declared phases first (meta.phases), then any the run named that the script did not declare.
    let declared = match &fin {
        Some(f) => f.phases.clone(),
        None => wf::meta_phases(script.unwrap_or_default()),
    };
    let named = match fin.as_ref().and_then(|f| f.progress.as_ref()).or(t.progress.as_ref()) {
        Some(p) => wf::phase_titles(p),
        None => Vec::new(),
    };
    let mut titles: Vec<String> = Vec::new();
    for title in declared.iter().map(|p| p.0.clone()).chain(named).chain(agents.iter().map(|a| a.phase.clone())) {
        if !title.is_empty() && !titles.contains(&title) {
            titles.push(title);
        }
    }
    let phases = titles
        .into_iter()
        .map(|title| {
            let of: Vec<&WfAgent> = agents.iter().filter(|a| a.phase == title).collect();
            let count = |f: &dyn Fn(WfState) -> bool| of.iter().filter(|a| f(a.state)).count() as i32;
            WfPhase {
                detail: declared.iter().find(|p| p.0 == title).and_then(|p| p.1.clone()),
                total: of.len() as i32,
                done: count(&|s| s == WfState::Done),
                running: count(&|s| s == WfState::Running),
                failed: count(&|s| matches!(s, WfState::Failed | WfState::Unknown)),
                title,
            }
        })
        .collect();

    let written = if state == WfState::Unknown { dir.as_deref().and_then(last_write) } else { None };
    // ponytail: a time heuristic (2 min since the last write), only used to keep polling; replace with the CLI's own
    // liveness signal if one appears.
    let live = state == WfState::Running || written.is_some_and(|w| (now - w).num_milliseconds() < 120_000);
    // An unfinished run ends, as far as anyone knows, at the last write in its folder.
    let end = if live { now } else { written.or(t.ended_at).unwrap_or(now) };
    let elapsed_ms = fin.as_ref().and_then(|f| f.duration_ms).unwrap_or_else(|| (end - t.started_at).num_milliseconds());

    WorkflowRun {
        name: json::str(receipt, "workflowName")
            .map(str::to_string)
            .or_else(|| script.and_then(|s| wf::meta(s, "name")))
            .unwrap_or_else(|| wf::TOOL.to_lowercase()),
        description: json::str(receipt, "summary").map(str::to_string).or_else(|| script.and_then(|s| wf::meta(s, "description"))),
        run_dir: dir.as_ref().map(|d| d.to_string_lossy().into_owned()),
        tokens: fin.as_ref().and_then(|f| f.tokens).unwrap_or_else(|| if t.tokens > 0 { t.tokens } else { agents.iter().map(|a| a.tokens).sum() }),
        tool_calls: fin
            .as_ref()
            .and_then(|f| f.tool_calls)
            .map(|n| n as i32)
            .unwrap_or_else(|| if t.sub_tool_uses > 0 { t.sub_tool_uses } else { agents.iter().map(|a| a.tool_calls).sum() }),
        elapsed_ms: elapsed_ms.max(0),
        // Still being written: it reads as running, not as "no end".
        state: if live && state == WfState::Unknown { WfState::Running } else { state },
        raw_status: status.filter(|s| s != "completed"),
        model: fin.and_then(|f| f.model),
        live,
        phases,
        agents,
    }
}

// ---------- path safety: the run folder comes from CLI output ----------

static INSIDE: Cache<String, Option<PathBuf>> = LazyLock::new(Default::default);

/// Real path of `path` when it is inside `Real(projects root)` (every symlink resolved), else None. Cached.
pub fn inside(path: Option<&str>) -> Option<PathBuf> {
    let path = path.filter(|p| !p.trim().is_empty())?;
    let root = paths::projects_root().to_string_lossy().into_owned();
    let key = format!("{root}\n{path}");
    if let Some(hit) = INSIDE.lock().get(&key) {
        return hit.clone();
    }
    let got = inside_of(path, &root, &transcript_store::real);
    INSIDE.lock().insert(key, got.clone());
    got
}

fn inside_of(path: &str, root: &str, real: &dyn Fn(&str) -> String) -> Option<PathBuf> {
    let full = real(path);
    let r = format!("{}{MAIN_SEPARATOR}", real(root).trim_end_matches(['/', MAIN_SEPARATOR]));
    let under = if cfg!(target_os = "linux") { full.starts_with(&r) } else { full.to_lowercase().starts_with(&r.to_lowercase()) };
    under.then(|| PathBuf::from(full))
}

// ---------- the final .json (<session>/workflows/<runId>.json) ----------

static FINALS: Cache<PathBuf, (u64, Option<std::time::SystemTime>, Option<Value>)> = LazyLock::new(Default::default);

/// `<session>/workflows/<runId>.json` for a run dir shaped `<session>/subagents/workflows/<runId>`; cached by
/// (length, mtime); parse/IO errors → None (retried on change).
pub fn final_file(dir: &Path) -> Option<Value> {
    let wf_dir = dir.parent()?;
    let sub = wf_dir.parent()?;
    if wf_dir.file_name()? != "workflows" || sub.file_name()? != "subagents" {
        return None;
    }
    let file = sub.parent()?.join("workflows").join(format!("{}.json", dir.file_name()?.to_string_lossy()));
    let meta = std::fs::metadata(&file).ok().filter(|m| m.is_file())?;
    let (len, at) = (meta.len(), meta.modified().ok());
    if let Some((l, a, v)) = FINALS.lock().get(&file) {
        if *l == len && *a == at {
            return v.clone();
        }
    }
    // A partial write fails to parse: cached as None, retried on the next change.
    let v = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    FINALS.lock().insert(file, (len, at, v.clone()));
    v
}

/// Max mtime of the files directly in dir.
pub fn last_write(dir: &Path) -> Option<Time> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.metadata().ok().filter(|m| m.is_file()).and_then(|m| m.modified().ok()))
        .max()
        .map(Time::from)
}

/// `dir/agent-<id>.jsonl` when the id is valid.
pub fn agent_file(dir: &Path, agent_id: &str) -> Option<PathBuf> {
    wf::valid_agent_id(agent_id).then(|| dir.join(format!("agent-{agent_id}.jsonl")))
}

// ---------- the journal and agent transcripts, while no .json exists ----------

struct JournalState {
    tail: Tail,
    /// (agent id, label, phase), first `started` wins, order kept.
    started: Vec<(String, String, String)>,
    results: HashMap<String, Option<String>>,
    /// Per agent-<id>.jsonl, folded line by line: never re-read.
    stats: HashMap<String, (Tail, AgentStats)>,
}

static JOURNALS: Cache<PathBuf, Arc<Mutex<JournalState>>> = LazyLock::new(Default::default);

/// Agents from `journal.jsonl` (+ agent stats), incremental, one state per dir. None when the journal is missing.
/// ended: the run is over, so an agent started without a result never finished.
pub fn journal(dir: &Path, ended: bool, now: Time) -> Option<Vec<WfAgent>> {
    let file = dir.join("journal.jsonl");
    if !file.is_file() {
        return None;
    }
    let state = JOURNALS
        .lock()
        .entry(dir.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(JournalState { tail: Tail::new(file), started: Vec::new(), results: HashMap::new(), stats: HashMap::new() })))
        .clone();
    let mut j = state.lock();
    let lines = j.tail.next();
    if j.tail.reset {
        j.started.clear();
        j.results.clear();
    }
    for l in lines {
        let Ok(e) = serde_json::from_str::<Value>(&l) else { continue };
        let Some(id) = json::str(&e, "agentId") else { continue }; // "launched" and unknown types carry none
        match json::str(&e, "type") {
            Some("started") if !j.started.iter().any(|s| s.0 == id) => {
                j.started.push((id.to_string(), json::string(&e, "label"), json::string(&e, "phase")));
            }
            Some("result") => {
                j.results.insert(id.to_string(), json::prop(&e, "result").map(json::to_text));
            }
            _ => {}
        }
    }
    let j = &mut *j;
    let agents = j
        .started
        .iter()
        .map(|(id, label, phase)| {
            let st = agent_file(dir, id).map(|f| {
                let (tail, stats) = j.stats.entry(id.clone()).or_insert_with(|| (Tail::new(f), AgentStats::default()));
                let lines = tail.next();
                if tail.reset {
                    *stats = AgentStats::default();
                }
                for l in lines {
                    if let Ok(v) = serde_json::from_str::<Value>(&l) {
                        stats.fold(&v);
                    }
                }
                &*stats
            });
            let done = j.results.contains_key(id);
            let state = if done {
                WfState::Done
            } else if ended {
                WfState::Unknown // started, then no result: no end recorded
            } else {
                WfState::Running
            };
            let duration_ms = st.and_then(|s| s.first.map(|first| ((if done || ended { s.last.unwrap_or(first) } else { now }) - first).num_milliseconds()));
            WfAgent {
                label: label.clone(),
                phase: phase.clone(),
                state,
                model: st.and_then(|s| s.model.clone()),
                tokens: st.map_or(0, |s| s.tokens),
                approx: true,
                tool_calls: st.map_or(0, |s| s.tool_calls),
                duration_ms,
                agent_id: Some(id.clone()),
                result: j.results.get(id).cloned().flatten(),
                raw_state: None,
            }
        })
        .collect();
    Some(agents)
}

/// Incremental line reader (offset; truncation → reset; a trailing line without '\n' waits).
pub struct Tail {
    path: PathBuf,
    offset: u64,
    /// The file shrank since the last read: consumers clear their state.
    pub reset: bool,
}

impl Tail {
    pub fn new(path: PathBuf) -> Tail {
        Tail { path, offset: 0, reset: false }
    }

    /// New complete lines (trimmed `\r`, empties dropped); IO errors → [].
    pub fn next(&mut self) -> Vec<String> {
        self.reset = false;
        // std opens with FILE_SHARE_READ | WRITE | DELETE on Windows: the CLI keeps writing and rotating.
        let Ok(mut f) = std::fs::File::open(&self.path) else { return Vec::new() }; // missing yet: partial data
        let Ok(len) = f.metadata().map(|m| m.len()) else { return Vec::new() };
        if len < self.offset {
            self.offset = 0;
            self.reset = true;
        }
        if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::new();
        if (&mut f).take(len - self.offset).read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        // Truncated between the length and the read (nothing read), or no complete line yet: wait.
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return Vec::new() };
        self.offset += end as u64 + 1;
        String::from_utf8_lossy(&buf[..end]).split('\n').map(|l| l.trim_end_matches('\r')).filter(|l| !l.is_empty()).map(str::to_string).collect()
    }
}

/// One agent's stats folded line by line: model, LAST message's tokens, distinct tool_use ids, first/last timestamps.
/// Tokens follow the last message's context, which is what the CLI shows to within ~1.5k; the sum over messages
/// would be 15-35 times too high.
#[derive(Default)]
pub struct AgentStats {
    pub model: Option<String>,
    pub tokens: i64,
    pub tool_calls: i32,
    pub first: Option<Time>,
    pub last: Option<Time>,
    tools: HashSet<String>,
}

impl AgentStats {
    pub fn fold(&mut self, line: &Value) {
        if let Some(at) = json::time(line) {
            self.first.get_or_insert(at);
            self.last = Some(at);
        }
        let Some(m) = json::prop(line, "message").filter(|_| json::str(line, "type") == Some("assistant")) else { return };
        if let Some(model) = json::str(line, "requestedModel").or_else(|| json::str(m, "model")) {
            self.model = Some(model.to_string());
        }
        if let Some(u) = json::prop(m, "usage") {
            self.tokens = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens"]
                .iter()
                .map(|k| json::num(u, k).unwrap_or(0))
                .sum();
        }
        if let Some(Value::Array(c)) = json::prop(m, "content") {
            for b in c {
                if json::str(b, "type") == Some("tool_use") {
                    if let Some(id) = json::str(b, "id") {
                        self.tools.insert(id.to_string());
                    }
                }
            }
            self.tool_calls = self.tools.len() as i32;
        }
    }
}

/// One agent's log for the inspector: tails `agent-<id>.jsonl` through a scratch reducer.
pub struct AgentLog {
    tail: Tail,
    state: SessionState,
}

fn scratch() -> SessionState {
    SessionState::new("", "", "", "default", chrono::Utc::now())
}

impl AgentLog {
    pub fn new(path: PathBuf) -> AgentLog {
        AgentLog { tail: Tail::new(path), state: scratch() }
    }

    /// Fold new lines (`toolUseResult` → `tool_use_result`, `replay::apply`); returns (last text, top-level tools).
    pub fn read(&mut self) -> WorkflowAgentLog {
        let lines = self.tail.next();
        if self.tail.reset {
            self.state = scratch();
        }
        for l in lines {
            if !l.contains("\"type\":\"user\"") && !l.contains("\"type\":\"assistant\"") {
                continue;
            }
            // Transcript files use camelCase, the stream snake_case.
            let Ok(e) = serde_json::from_str::<Value>(&l.replace("\"toolUseResult\":", "\"tool_use_result\":")) else { continue };
            let at = json::time(&e);
            let state = &mut self.state;
            if let Err(p) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| replay::apply(state, &e, at))) {
                let why = p.downcast_ref::<String>().map(String::as_str).or_else(|| p.downcast_ref::<&str>().copied()).unwrap_or("panic");
                eprintln!("workflow agent log: line skipped ({why})");
            }
        }
        let items = self.state.items();
        WorkflowAgentLog {
            text: items.iter().rev().find_map(|i| match i {
                Item::Text(t) => Some(t.markdown.clone()),
                _ => None,
            }),
            tools: items.iter().filter_map(Item::as_tool).filter(|t| t.parent_tool_use_id.is_none()).cloned().collect(),
        }
    }
}

/// Agent logs kept per (session id, tool id, agent id) between `workflow_agent_log` calls.
#[derive(Default)]
pub struct AgentLogs {
    pub logs: Mutex<HashMap<(String, String, String), AgentLog>>,
}

impl AgentLogs {
    /// The log of that agent of that run (created on first call; recreated when its file key changes).
    // ponytail: one lock over every log while one is read (a few new lines); per-log locks if inspectors multiply.
    pub fn read(&self, session_id: &str, tool: &ToolItem, agent_id: &str) -> WorkflowAgentLog {
        let run = read(tool, chrono::Utc::now());
        let Some(file) = run.run_dir.as_deref().and_then(|d| agent_file(Path::new(d), agent_id)) else { return WorkflowAgentLog::default() };
        let mut logs = self.logs.lock();
        let key = (session_id.to_string(), tool.id.clone(), agent_id.to_string());
        if logs.get(&key).is_none_or(|l| l.tail.path != file) {
            logs.insert(key.clone(), AgentLog::new(file));
        }
        logs.get_mut(&key).map(AgentLog::read).unwrap_or_default()
    }

    /// The inspector stopped showing that agent (`workflow_agent_log_release`): drop its log. Spec 05/07: a log is
    /// kept only while selected.
    pub fn release(&self, session_id: &str, tool_id: &str, agent_id: &str) {
        self.logs.lock().remove(&(session_id.to_string(), tool_id.to_string(), agent_id.to_string()));
    }

    /// Drop every log of a session (called by the `session_stop` command after `SessionManager::stop`).
    pub fn forget_session(&self, session_id: &str) {
        self.logs.lock().retain(|k, _| k.0 != session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> Time {
        chrono::Utc.timestamp_millis_opt(1791626222733).unwrap()
    }

    const SCRIPT: &str = "export const meta = { name: 'pong-probe', description: 'Two agents \\'reply\\' pong', phases: [{ title: 'Ping', detail: 'two pong agents' }, { title: \"Check\" }] }\nphase('Ping')";
    const SNAP: &str = r#"[{"type":"workflow_phase","index":1,"title":"Ping"},{"type":"workflow_agent","index":1,"label":"pong:a","phaseIndex":1,"phaseTitle":"Ping","agentId":"aea012a4f32b49114","model":"claude-haiku-5-5","state":"done","startedAt":1791626220733,"queuedAt":1791626220731,"attempt":1,"tokens":24538,"toolCalls":0,"durationMs":2045,"resultPreview":"pong"},{"type":"workflow_agent","index":2,"label":"pong:b","phaseIndex":1,"phaseTitle":"Ping","model":"claude-haiku-5-5","state":"start","queuedAt":1791626220731}]"#;

    fn tool(id: &str, state: ToolState, started: Time) -> ToolItem {
        ToolItem {
            id: id.into(),
            name: wf::TOOL.into(),
            input: serde_json::json!({ "script": SCRIPT }),
            parent_tool_use_id: None,
            state,
            result_text: None,
            structured: None,
            started_at: started,
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
    fn live_run_from_the_snapshot() {
        let mut t = tool("w-live", ToolState::Running, now() - chrono::Duration::seconds(2));
        t.progress = Some(serde_json::from_str(SNAP).unwrap());
        let r = read(&t, now());
        assert!(r.name == "pong-probe" && r.state == WfState::Running && r.live && r.done() == 1 && r.agents.len() == 2);
        assert_eq!(r.phases.len(), 2);
        let (p, c) = (&r.phases[0], &r.phases[1]);
        assert!(p.title == "Ping" && p.total == 2 && p.done == 1 && p.state() == WfState::Running && p.detail.as_deref() == Some("two pong agents"));
        assert!(c.title == "Check" && c.total == 0 && c.state() == WfState::Waiting, "declared phases kept");
        assert_eq!(r.elapsed_ms, 2000);
        assert_eq!(r.tokens, 24538);
        assert_eq!(r.description.as_deref(), Some("Two agents 'reply' pong"));
        // cached for 500 ms for the same ToolItem
        assert_eq!(read(&t, now() + chrono::Duration::milliseconds(100)).elapsed_ms, 2000);
    }

    #[test]
    fn unfinished_vs_failed() {
        let r = read(&tool("w-u", ToolState::Error, now()), now());
        assert!(r.state == WfState::Unknown && r.agents.is_empty() && !r.live);
        let mut f = tool("w-f", ToolState::Error, now());
        f.result_text = Some("boom".into());
        assert_eq!(read(&f, now()).state, WfState::Failed);
        let mut d = tool("w-d", ToolState::Done, now());
        d.task_status = Some("killed".into());
        let r = read(&d, now());
        assert!(r.state == WfState::Failed && r.raw_status.as_deref() == Some("killed"));
    }

    #[cfg(unix)]
    #[test]
    fn run_dir_must_be_under_the_projects_root() {
        let real = |p: &str| ccui_shared::memory::normalize(p).to_string_lossy().into_owned();
        let root = "/home/u/.claude/projects";
        assert!(inside_of(&format!("{root}/p/s/subagents/workflows/wf_1"), root, &real).is_some());
        assert!(inside_of(&format!("{root}/../x"), root, &real).is_none());
        assert!(inside_of("/tmp", root, &real).is_none());
        assert!(inside_of(root, &format!("{root}/"), &real).is_none(), "the root itself is not inside");
        assert!(inside(None).is_none() && inside(Some("  ")).is_none());
    }

    #[test]
    fn agent_file_ids() {
        assert_eq!(agent_file(Path::new("/d"), "a1b2"), Some(Path::new("/d").join("agent-a1b2.jsonl")));
        assert_eq!(agent_file(Path::new("/d"), "../x"), None);
    }

    const JOURNAL: &str = "{\"type\":\"launched\"}\n\
{\"type\":\"started\",\"key\":\"v2:1\",\"agentId\":\"a1\",\"label\":\"probe\",\"phase\":\"Probe\"}\n\
{\"type\":\"weird\",\"agentId\":\"a1\"}\n\
{\"type\":\"started\",\"key\":\"v2:2\",\"agentId\":\"a2\",\"label\":\"build\",\"phase\":\"Build\"}\n\
{\"type\":\"result\",\"key\":\"v2:1\",\"agentId\":\"a1\",\"result\":{\"ok\":true}}\n\
{\"type\":\"result\",\"key\":\"v2:2\",\"agentId\":\"a2\",\"res";

    const AGENT: &str = r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"go"},"timestamp":"2026-10-10T09:00:00.000Z"}
{"type":"assistant","requestedModel":"claude-opus-5-5[1m]","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}],"usage":{"input_tokens":2,"cache_creation_input_tokens":10,"cache_read_input_tokens":100,"output_tokens":5}},"timestamp":"2026-10-10T09:00:01.000Z"}
{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"a.txt"}]},"timestamp":"2026-10-10T09:00:02.000Z"}
{"type":"assistant","requestedModel":"claude-opus-5-5[1m]","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":2,"cache_creation_input_tokens":20,"cache_read_input_tokens":110,"output_tokens":8}},"timestamp":"2026-10-10T09:00:04.500Z"}

"#;

    #[test]
    fn journal_and_agent_stats() {
        // Journal + agent transcript on disk, as 2.1.295 writes them; a partial last line, an unknown type, a truncation.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let jf = dir.join("journal.jsonl");
        std::fs::write(&jf, JOURNAL).unwrap();
        std::fs::write(dir.join("agent-a1.jsonl"), AGENT).unwrap();
        let j = journal(dir, false, now()).unwrap();
        assert_eq!(j.len(), 2);
        let p1 = &j[0];
        assert!(p1.label == "probe" && p1.phase == "Probe" && p1.state == WfState::Done && p1.result.as_deref() == Some(r#"{"ok":true}"#));
        assert!(p1.model.as_deref() == Some("claude-opus-5-5[1m]") && p1.tokens == 140 && p1.tool_calls == 1 && p1.approx);
        assert_eq!(p1.duration_ms, Some(4500));
        assert!(j[1].label == "build" && j[1].state == WfState::Running && j[1].model.is_none() && j[1].duration_ms.is_none());

        std::fs::OpenOptions::new().append(true).open(&jf).and_then(|mut f| std::io::Write::write_all(&mut f, b"ult\":\"pong\"}\n")).unwrap();
        let j = journal(dir, false, now()).unwrap();
        assert!(j[1].state == WfState::Done && j[1].result.as_deref() == Some("pong"), "the partial line completes");

        std::fs::write(&jf, "{\"type\":\"started\",\"agentId\":\"a3\",\"label\":\"x\",\"phase\":\"P\"}\n").unwrap();
        let j = journal(dir, true, now()).unwrap();
        assert!(j.len() == 1 && j[0].agent_id.as_deref() == Some("a3") && j[0].state == WfState::Unknown && j[0].raw_state.is_none(), "truncation starts over");
        assert!(journal(&dir.join("none"), false, now()).is_none(), "missing file");
    }

    #[test]
    fn final_file_and_last_write() {
        let tmp = tempfile::tempdir().unwrap();
        let run = tmp.path().join("s/subagents/workflows/wf_1");
        std::fs::create_dir_all(&run).unwrap();
        assert!(final_file(&run).is_none());
        std::fs::create_dir_all(tmp.path().join("s/workflows")).unwrap();
        let f = tmp.path().join("s/workflows/wf_1.json");
        std::fs::write(&f, r#"{"status":"comp"#).unwrap();
        assert!(final_file(&run).is_none(), "partial write");
        std::fs::write(&f, r#"{"status":"completed","durationMs":5000,"totalTokens":7}"#).unwrap();
        let v = final_file(&run).unwrap();
        let fr = FinalRun::of(&v);
        assert!(fr.status.as_deref() == Some("completed") && fr.duration_ms == Some(5000) && fr.tokens == Some(7) && fr.phases.is_empty());
        assert!(final_file(tmp.path()).is_none(), "not a run dir shape");
        assert!(last_write(&run).is_none(), "no files");
        std::fs::write(run.join("journal.jsonl"), "").unwrap();
        assert!(last_write(&run).is_some());
    }

    #[test]
    fn tail_semantics() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.jsonl");
        let mut t = Tail::new(p.clone());
        assert!(t.next().is_empty() && !t.reset, "missing file");
        std::fs::write(&p, "a\r\n\nb").unwrap();
        assert_eq!(t.next(), ["a"]);
        assert!(t.next().is_empty(), "the trailing line waits");
        std::fs::write(&p, "c\n").unwrap();
        assert_eq!(t.next(), ["c"]);
        assert!(t.reset, "shorter file starts over");
        assert!(t.next().is_empty() && !t.reset);
    }

    #[test]
    fn agent_log() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("agent-a1.jsonl");
        std::fs::write(&f, AGENT).unwrap();
        let mut log = AgentLog::new(f);
        let r = log.read();
        assert_eq!(r.text.as_deref(), Some("done"));
        assert!(r.tools.len() == 1 && r.tools[0].name == "Bash" && r.tools[0].state == ToolState::Done);
        assert_eq!(r.tools[0].ended_at.map(|e| (e - r.tools[0].started_at).num_milliseconds()), Some(1000));
        let again = log.read();
        assert!(again.text.as_deref() == Some("done") && again.tools.len() == 1, "nothing new, same state");
        assert_eq!(AgentLog::new(tmp.path().join("agent-none.jsonl")).read(), WorkflowAgentLog::default());
    }
}
