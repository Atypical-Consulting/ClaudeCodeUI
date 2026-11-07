//! Port of `Core/TranscriptStore.cs` (metadata reader, recents, search, endings, prompts, held processes, ledgers)
//! — spec 03 §1. Owner: core-store-sessions. Replay lives in `replay.rs`; text helpers in `ccui_shared::text`.
//!
//! Reads the CLI's own transcripts `projects/<slug>/<id>.jsonl` (top-level files only). Opens share
//! read+write+delete on Windows (the CLI appends concurrently). File-scanning functions are blocking: commands wrap
//! them in `tokio::task::spawn_blocking`.

use crate::paths;
use ccui_shared::api::{Ending, PastSession, SearchResult, Stop, TranscriptHit};
use ccui_shared::json::{self, prop, str as jstr};
use ccui_shared::model::Time;
use ccui_shared::text;
use parking_lot::Mutex;
use regex::Regex;
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime};

/// Recent(take = 30) default.
pub const RECENT_TAKE: usize = 30;
/// Search budget (bytes read before giving up: partial result).
pub const SEARCH_BUDGET: u64 = 256 * 1024 * 1024;

const CHUNK: u64 = 64 * 1024;

/// Current id per palette search (`search_transcripts(search_id, …)`): a scan stops when its id is no longer current.
#[derive(Default)]
pub struct SearchRegistry {
    pub current: AtomicU64,
}

// ------------------------------------------------------------------------------------------------ file access

/// Shared open: the CLI appends concurrently (Windows also needs FILE_SHARE_DELETE, std's default lacks it).
fn open_shared(path: &Path) -> std::io::Result<File> {
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(0x1 | 0x2 | 0x4); // FILE_SHARE_READ | WRITE | DELETE
    }
    o.open(path)
}

/// `count` bytes at `at`, decoded lossily (chunk edges can split a code point).
fn read_at(f: &mut File, at: u64, count: u64) -> std::io::Result<String> {
    f.seek(SeekFrom::Start(at))?;
    let mut buf = Vec::with_capacity(count as usize);
    f.take(count).read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Whole file, lossily decoded (`File.ReadLines` replaces invalid UTF-8).
pub(crate) fn read_text(path: &Path) -> std::io::Result<String> {
    let mut buf = Vec::new();
    open_shared(path)?.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

struct Transcript {
    path: PathBuf,
    mtime: SystemTime,
    len: u64,
}

fn jsonl_in(dir: &Path, out: &mut Vec<Transcript>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "jsonl") {
            if let Ok(m) = e.metadata() {
                if m.is_file() {
                    out.push(Transcript { path, mtime: m.modified().unwrap_or(SystemTime::UNIX_EPOCH), len: m.len() });
                }
            }
        }
    }
}

/// Project dirs of the root whose name passes `keep`.
fn project_dirs(keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(paths::projects_root()) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && keep(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect()
}

/// `<root>/*/*.jsonl` (one level: `<id>/subagents` excluded by construction), newest first.
fn transcripts(keep_dir: impl Fn(&str) -> bool) -> Vec<Transcript> {
    let mut all = Vec::new();
    for d in project_dirs(keep_dir) {
        jsonl_in(&d, &mut all);
    }
    all.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    all
}

fn to_time(t: SystemTime) -> Time {
    t.into()
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `Path.TrimEndingDirectorySeparator`: one trailing separator dropped, roots kept.
fn trim_sep(s: &str) -> &str {
    let seps: &[char] = if cfg!(windows) { &['\\', '/'] } else { &['/'] };
    let root = s.len() <= 1 || (cfg!(windows) && s.len() == 3 && s.as_bytes()[1] == b':');
    if !root && s.ends_with(seps) {
        &s[..s.len() - 1]
    } else {
        s
    }
}

/// `Path.GetFileName`: after the last native separator.
fn file_name(s: &str) -> &str {
    let seps: &[char] = if cfg!(windows) { &['\\', '/'] } else { &['/'] };
    s.rsplit(seps).next().unwrap_or(s)
}

fn parse(line: &str) -> Option<Value> {
    serde_json::from_str(line).ok()
}

/// `Hidden(e)`: sub-agent, injected context, compaction summary, transcript-only lines.
pub(crate) fn hidden(e: &Value) -> bool {
    ["isSidechain", "isMeta", "isCompactSummary", "isVisibleInTranscriptOnly"].iter().any(|k| json::bool(e, k))
}

/// `Uri.TryCreate(u, Absolute)` with an http/https scheme.
fn http_url(u: &str) -> bool {
    let lower = u.get(..8).unwrap_or(u).to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &u[8..]
    } else if lower.starts_with("http://") {
        &u[7..]
    } else {
        return false;
    };
    rest.chars().next().is_some_and(|c| !matches!(c, '/' | '?' | '#') && !c.is_whitespace())
}

// ------------------------------------------------------------------------------------------------ recent / since

/// (root, at, take | since, list): keyed by root so a changed CLAUDE_CONFIG_DIR never serves another tree.
static RECENT: Mutex<Option<(PathBuf, Instant, usize, Vec<PastSession>)>> = Mutex::new(None);
static SINCE: Mutex<Option<(PathBuf, Instant, Time, Vec<PastSession>)>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Newest transcripts of the last 30 days (cached 30 s), skipping unreadable / cwd-less files.
pub fn recent(take: usize) -> Vec<PastSession> {
    let root = paths::projects_root();
    if let Some((r, at, t, list)) = &*RECENT.lock() {
        if *r == root && *t >= take && at.elapsed() < Duration::from_secs(30) {
            return list.iter().take(take).cloned().collect();
        }
    }
    let since = SystemTime::now() - Duration::from_secs(30 * 86_400);
    let mut list = Vec::new();
    for f in transcripts(|_| true).into_iter().filter(|f| f.mtime >= since) {
        if list.len() >= take {
            break;
        }
        if let Some(p) = read(&f.path, true) {
            list.push(p);
        }
    }
    *RECENT.lock() = Some((root, Instant::now(), take, list.clone()));
    list
}

/// Every transcript modified since `since` (Overview week; cached 60 s).
pub fn since(since: ccui_shared::Time) -> Vec<PastSession> {
    let root = paths::projects_root();
    if let Some((r, at, s, list)) = &*SINCE.lock() {
        if *r == root && *s <= since && at.elapsed() < Duration::from_secs(60) {
            return list.iter().filter(|p| p.last_write >= since).cloned().collect();
        }
    }
    let list: Vec<PastSession> =
        transcripts(|_| true).into_iter().filter(|f| to_time(f.mtime) >= since).filter_map(|f| read(&f.path, true)).collect();
    *SINCE.lock() = Some((root, Instant::now(), since, list.clone()));
    list
}

// ------------------------------------------------------------------------------------------------ metadata

/// Metadata of one transcript (head 64 KiB + tail 64 KiB when big). None = unreadable or no cwd yet.
pub fn read(file: &Path, ledger: bool) -> Option<PastSession> {
    let (text, mtime) = (|| -> std::io::Result<(String, SystemTime)> {
        let mut f = open_shared(file)?;
        let meta = f.metadata()?;
        let len = meta.len();
        let text = if len <= 2 * CHUNK {
            read_at(&mut f, 0, len)?
        } else {
            format!("{}\n{}", read_at(&mut f, 0, CHUNK)?, read_at(&mut f, len - CHUNK, CHUNK)?)
        };
        Ok((text, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
    })()
    .ok()?;

    let cwd = field(&text, "cwd")?; // no message yet
    let branch = field(&text, "gitBranch");
    let (mut custom, mut agent, mut ai, mut last, mut first_user, mut worktree, mut pr) =
        (None, None, None, None, None::<String>, None, None);
    let mut cost: Option<Decimal> = None;
    let s = |e: &Value, k: &str| jstr(e, k).map(str::to_string);
    for line in text.split('\n') {
        if !line.starts_with("{\"type\":\"") && !(first_user.is_none() && line.contains("\"type\":\"user\"")) {
            continue;
        }
        let Some(e) = parse(line) else { continue };
        match jstr(&e, "type") {
            Some("custom-title") => custom = s(&e, "customTitle").or(custom),
            Some("agent-name") => agent = s(&e, "agentName").or(agent),
            Some("ai-title") => ai = s(&e, "aiTitle").or(ai),
            Some("last-prompt") => last = s(&e, "lastPrompt").or(last),
            Some("cost-state") => {
                if let Some(n) = json::decimal(&e, "totalCostUSD") {
                    cost = Some(n);
                }
            }
            Some("worktree-state") => {
                if let Some(w) = prop(&e, "worktreeSession") {
                    worktree = s(w, "worktreePath").or(worktree);
                }
            }
            Some("pr-link") => {
                if let Some(u) = jstr(&e, "prUrl").filter(|u| http_url(u)) {
                    pr = Some(u.to_string());
                }
            }
            Some("user") if first_user.is_none() && !json::bool(&e, "isMeta") => {
                if let Some(c) = prop(&e, "message").and_then(|m| jstr(m, "content")) {
                    if !c.is_empty() && !c.starts_with('<') {
                        first_user = Some(c.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    let title = custom
        .or(agent)
        .or(ai)
        .or_else(|| text::trunc(last.as_deref()))
        .or_else(|| text::trunc(first_user.as_deref()))
        .unwrap_or_else(|| file_name(trim_sep(&cwd)).to_string());
    let id = stem(file);
    if ledger {
        if let Some(rec) = recorded_cost(&id) {
            cost = Some(cost.unwrap_or_default().max(rec));
        }
    }
    // Latest permissionMode of the file (permission-mode records and user lines both carry it).
    static MODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""permissionMode":"([A-Za-z]+)""#).unwrap());
    let mode = MODE.captures_iter(&text).last().map(|c| c[1].to_string());
    Some(PastSession { id, cwd, branch, title, cost_usd: cost, last_write: to_time(mtime), worktree_path: worktree, mode, pr_url: pr })
}

/// First `"NAME":"…"` string anywhere in the text, JSON-decoded; empty or bad escape → None.
pub fn field(text: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#""{}":("(?:[^"\\]|\\.)*")"#, regex::escape(name))).ok()?;
    let lit = re.captures(text)?.get(1)?.as_str();
    serde_json::from_str::<String>(lit).ok().filter(|v| !v.is_empty())
}

/// `projects/<dir>/<id>.jsonl` of any project dir.
pub fn find(id: &str) -> Option<PathBuf> {
    project_dirs(|_| true).into_iter().map(|d| d.join(format!("{id}.jsonl"))).find(|p| p.is_file())
}

/// Any transcript by id; id must be non-empty `[A-Za-z0-9-]+` (it comes from the URL).
pub fn by_id(id: &str) -> Option<PastSession> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    read(&find(id)?, true)
}

// ------------------------------------------------------------------------------------------------ endings

/// How the conversation ended (last 64 KiB, then 512 KiB when only tool traffic).
pub fn end_of(id: &str) -> Option<Ending> {
    let path = find(id)?;
    let mut f = open_shared(&path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut tail = |n: u64| -> Option<Ending> {
        let t = read_at(&mut f, len.saturating_sub(n), len.min(n)).ok()?;
        Some(end_of_lines(&t.split('\n').collect::<Vec<_>>()))
    };
    let end = tail(CHUNK)?;
    if end.text.is_some() || len <= CHUNK {
        return Some(end);
    }
    // Only tool traffic in the last 64 KB (big tool results): look 8× further back for words.
    let wide = tail(8 * CHUNK)?;
    Some(if wide.text.is_some() { Ending { stop: end.stop, unanswered: end.unanswered, interrupted: end.interrupted, ..wide } } else { end })
}

/// Walk the lines backwards (spec 03 §1.8 rules).
pub fn end_of_lines(lines: &[&str]) -> Ending {
    let (mut said_text, mut prompt, mut tool, mut target) = (None::<String>, None::<String>, None::<String>, None::<String>);
    let mut stop: Option<Stop> = None;
    let (mut unanswered, mut interrupted) = (false, false);
    for line in lines.iter().rev() {
        if said_text.is_some() && stop.is_some() {
            break;
        }
        if !line.contains("\"type\":\"assistant\"") && !line.contains("\"type\":\"user\"") {
            continue;
        }
        let Some(e) = parse(line) else { continue };
        let Some(m) = prop(&e, "message") else { continue };
        if json::bool(&e, "isSidechain") || json::bool(&e, "isMeta") {
            continue;
        }
        let blocks: &[Value] = m.get("content").and_then(Value::as_array).map_or(&[], Vec::as_slice);
        let content = jstr(m, "content");
        match jstr(&e, "type") {
            Some("user") => {
                // A typed prompt (not a <command> echo) or a tool result Claude never followed up.
                if content.is_some_and(|c| c.starts_with('<')) {
                    continue;
                }
                if stop.is_none() && blocks.last().and_then(|b| jstr(b, "text")).is_some_and(|t| t.starts_with("[Request interrupted")) {
                    interrupted = true;
                    stop = Some(Stop::Cut);
                    continue;
                }
                let typed = content.filter(|c| !c.is_empty());
                if stop.is_none() && typed.is_some() {
                    unanswered = true; // a typed message, the last word
                }
                stop.get_or_insert(Stop::Cut);
                if let Some(t) = typed {
                    prompt.get_or_insert_with(|| t.to_string());
                }
            }
            Some("assistant") if !blocks.is_empty() => {
                let last = &blocks[blocks.len() - 1];
                match jstr(last, "type") {
                    Some("text") => {
                        if let Some(t) = jstr(last, "text").filter(|t| !t.is_empty()) {
                            let plain = text::prose(t);
                            if !plain.is_empty() {
                                said_text.get_or_insert_with(|| text::tail(&plain, 220));
                                // A closing "?" is an offer, not a pending question: only AskUserQuestion waits.
                                stop.get_or_insert(Stop::Done);
                            }
                        }
                    }
                    Some("tool_use") => {
                        let name = jstr(last, "name");
                        stop.get_or_insert(if name == Some(ccui_shared::ask_user::TOOL) { Stop::Asked } else { Stop::Cut });
                        if let (None, Some(n)) = (&tool, name) {
                            target = Some(text::cut(&ccui_shared::tool_kinds::target(n, json::get(last, "input"), "")));
                            tool = Some(n.to_string());
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let said = match (&prompt, unanswered) {
        (Some(p), true) => Some(format!("› {}", text::tail(&text::prose(p), 220))),
        _ => said_text.or_else(|| prompt.map(|p| format!("› {}", text::cut(&text::prose(&p))))),
    };
    let none = said.is_none();
    Ending {
        text: said,
        stop: stop.unwrap_or(Stop::Done),
        tool: tool.filter(|_| none),
        target: target.filter(|_| none),
        unanswered,
        interrupted,
    }
}

// ------------------------------------------------------------------------------------------------ cost

/// Raw cost-state of a transcript (no ledger).
pub fn cost_state(file: &Path) -> Option<Decimal> {
    read(file, false)?.cost_usd
}

/// `CostState(Find(id)) ?? 0` — where `--resume` makes total_cost_usd start again.
pub fn persisted_cost(id: &str) -> Decimal {
    find(id).and_then(|p| cost_state(&p)).unwrap_or_default()
}

/// "Has claude ever run in this folder?" (`projects/slug(cwd)` exists; no realpath).
pub fn used(cwd: &str) -> bool {
    paths::projects_root().join(text::slug(trim_sep(cwd))).is_dir()
}

/// Resolved path (the CLI keys transcripts by the realpath): canonicalize the longest existing ancestor.
pub fn real(path: &str) -> String {
    let Ok(abs) = std::path::absolute(path) else { return path.to_string() };
    let mut base = abs.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(c) = dunce::canonicalize(base) {
            let mut p = c;
            for r in rest.iter().rev() {
                p.push(r);
            }
            return trim_sep(&p.to_string_lossy()).to_string();
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                base = parent;
            }
            _ => return trim_sep(&abs.to_string_lossy()).to_string(),
        }
    }
}

// ------------------------------------------------------------------------------------------------ held processes

static HELD: Mutex<Option<(Instant, HashSet<String>)>> = Mutex::new(None);
static REFRESHING: AtomicBool = AtomicBool::new(false);
static HELD_GEN: AtomicU64 = AtomicU64::new(0);

/// Session ids named on any running claude command line (cached 5 s, stale-while-revalidate).
pub fn held_by_claude() -> HashSet<String> {
    let cached = HELD.lock().clone();
    let Some((at, ids)) = cached else { return scan_held() };
    if at.elapsed() >= Duration::from_secs(5) && !REFRESHING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            scan_held();
            REFRESHING.store(false, Ordering::SeqCst);
        });
    }
    ids
}

fn scan_held() -> HashSet<String> {
    let generation = HELD_GEN.load(Ordering::SeqCst);
    let ids = run("ps", &["-Ao", "args="]).map(|o| held_ids(&o)).unwrap_or_default();
    // A scan started before a Stop never writes its stale ps back.
    let mut held = HELD.lock();
    if generation == HELD_GEN.load(Ordering::SeqCst) {
        *held = Some((Instant::now(), ids.clone()));
    }
    ids
}

/// stdout of a short helper command. ponytail: `ps`/`lsof` only, so empty on Windows (Win32_Process if needed).
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let out = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Clear the held cache (a scan started before never writes back).
pub fn forget_held() {
    let mut held = HELD.lock();
    HELD_GEN.fetch_add(1, Ordering::SeqCst);
    *held = None;
}

/// `ps -Ao args=` output → ids after `--session-id` / `--resume` on lines containing "claude".
pub fn held_ids(ps: &str) -> HashSet<String> {
    static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--(?:session-id|resume)[ =]([0-9a-fA-F-]{36})").unwrap());
    ps.split('\n').filter(|l| l.contains("claude")).filter_map(|l| ID.captures(l)).map(|c| c[1].to_string()).collect()
}

/// Folders where a plain `claude` (no session id) runs (`ps` + `lsof`); uncached; empty on Windows.
pub fn plain_claude_cwds() -> HashSet<String> {
    let Some(ps) = run("ps", &["-Ao", "pid=,args="]) else { return HashSet::new() };
    let pids = plain_claude_pids(&ps);
    if pids.is_empty() {
        return HashSet::new();
    }
    let list = pids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    run("lsof", &["-a", "-d", "cwd", "-Fn", "-p", &list]).map(|o| cwds_from_lsof(&o)).unwrap_or_default()
}

pub fn plain_claude_pids(ps: &str) -> Vec<u32> {
    static LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(\d+)\s+(.*)$").unwrap());
    static CLAUDE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[\s/])claude(\s|$)").unwrap());
    static SESSION: LazyLock<Regex> = LazyLock::new(|| Regex::new("--(session-id|resume)").unwrap());
    ps.split('\n')
        .filter_map(|l| LINE.captures(l))
        .filter(|c| CLAUDE.is_match(&c[2]) && !SESSION.is_match(&c[2]))
        .filter_map(|c| c[1].parse().ok())
        .collect()
}

pub fn cwds_from_lsof(o: &str) -> HashSet<String> {
    o.split('\n').filter(|l| l.starts_with('n') && l.len() > 1).map(|l| trim_sep(&l[1..]).to_string()).collect()
}

// ------------------------------------------------------------------------------------------------ prompts

/// Composer ↑ history: typed prompts of the 10 newest transcripts of the folder (except `except_id`), newest first.
pub fn prompts(cwd: &str, except_id: Option<&str>, sessions: usize) -> Vec<String> {
    if cwd.trim().is_empty() {
        return Vec::new(); // empty cwd: no past prompts
    }
    let dir = paths::projects_root().join(text::slug(&real(cwd)));
    let mut files = Vec::new();
    jsonl_in(&dir, &mut files);
    files.retain(|f| Some(stem(&f.path).as_str()) != except_id);
    files.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    let mut list = Vec::new();
    for f in files.iter().take(sessions) {
        // Being written on Windows: skip it.
        if let Ok(t) = read_text(&f.path) {
            list.extend(prompts_in(&t.lines().collect::<Vec<_>>()).into_iter().rev());
        }
    }
    list
}

pub fn prompts_in(lines: &[&str]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.contains("\"type\":\"user\""))
        .filter_map(|l| parse(l))
        .filter(|e| jstr(e, "type") == Some("user") && !hidden(e))
        .flat_map(|e| crate::events::parse_all(&e))
        .filter_map(|ev| match ev {
            crate::events::ClaudeEvent::UserText { text, .. } => ccui_shared::session_rules::user_text(&text),
            _ => None,
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ search

/// The query as written inside a raw line: the CLI's JSON escapes quotes and backslashes; control chars → None
/// (no prefilter).
fn raw(query: &str) -> Option<String> {
    (!query.chars().any(char::is_control)).then(|| query.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Ordinal ignore-case `contains`; ASCII needles (the usual case) compare bytes.
fn contains_ci(hay: &str, needle: &str) -> bool {
    if needle.is_ascii() {
        let n = needle.as_bytes();
        n.is_empty() || hay.as_bytes().windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
    } else {
        text::find_ci(hay, needle).is_some()
    }
}

/// Ctrl K message search (one hit per session, newest first). `cancelled` is polled per line.
pub fn search(query: &str, take: usize, cancelled: &dyn Fn() -> bool, budget: u64) -> SearchResult {
    let query = query.trim();
    let mut hits = Vec::new();
    if query.is_empty() {
        return SearchResult { hits, complete: true };
    }
    let raw = raw(query);
    let mut budget = budget as i64;
    for f in transcripts(|_| true) {
        budget -= f.len as i64;
        if hits.len() >= take || budget < 0 {
            return SearchResult { hits, complete: false };
        }
        let Ok(file) = open_shared(&f.path) else { continue };
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if cancelled() {
                return SearchResult { hits, complete: false };
            }
            let line = String::from_utf8_lossy(&buf);
            let line = line.trim_end_matches(['\n', '\r']);
            if raw.as_deref().is_some_and(|r| !contains_ci(line, r)) {
                continue;
            }
            let Some((snippet, at)) = match_line(line, query) else { continue };
            if let Some(p) = read(&f.path, true) {
                let at = at.unwrap_or(p.last_write);
                hits.push(TranscriptHit { session: p, snippet, at });
            }
            break; // one hit per session
        }
    }
    SearchResult { hits, complete: true }
}

/// One line's first visible text containing the query → (snippet, at).
pub fn match_line(line: &str, query: &str) -> Option<(String, Option<ccui_shared::Time>)> {
    let e = parse(line)?;
    let kind = jstr(&e, "type")?;
    if !matches!(kind, "user" | "assistant") || hidden(&e) {
        return None;
    }
    let m = prop(&e, "message")?;
    let texts: Vec<&str> = match m.get("content") {
        Some(Value::String(c)) if kind == "user" => vec![c.as_str()],
        Some(Value::Array(a)) => a.iter().filter(|b| jstr(b, "type") == Some("text")).filter_map(|b| jstr(b, "text")).collect(),
        _ => Vec::new(),
    };
    let qlen = query.chars().count();
    for t in texts {
        // Tagged texts are the CLI's own (slash commands, task notifications, IDE context).
        if t.starts_with('<') {
            continue;
        }
        if let Some(i) = text::find_ci(t, query) {
            // Shown as plain text, unless the query itself spells markdown.
            let p = text::plain(t);
            let snippet = match text::find_ci(&p, query) {
                Some(j) => text::snippet(&p, j, qlen, 40),
                None => text::snippet(t, i, qlen, 40),
            };
            return Some((snippet, json::time(&e)));
        }
    }
    None
}

/// `--from-pr` lookup: the repo's (and its worktrees') transcript linked to that PR.
pub fn linked_to(pr: &str, cwd: &str) -> Option<PastSession> {
    let pr = pr.trim().trim_end_matches('/');
    let n = text::pr_number(pr)?;
    let slug = text::slug(trim_sep(&text::root_of(cwd)));
    let wt = format!("{slug}--claude-worktrees-");
    transcripts(|d| d == slug || d.starts_with(&wt)).into_iter().filter_map(|f| read(&f.path, false)).find(|p| {
        p.pr_url.as_deref().is_some_and(|u| {
            if pr.contains('/') {
                u.trim_end_matches('/').eq_ignore_ascii_case(pr)
            } else {
                text::pr_number(u).as_deref() == Some(n.as_str())
            }
        })
    })
}

// ------------------------------------------------------------------------------------------------ ledgers

fn ledger(dir: &str, id: &str) -> PathBuf {
    paths::app_data().join(dir).join(format!("{id}.txt"))
}

/// `costs/<id>.txt`, invariant decimal, no newline; errors swallowed.
pub fn record_cost(id: &str, usd: Decimal) {
    let path = ledger("costs", id);
    let _ = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&path, usd.to_string()));
}

pub fn recorded_cost(id: &str) -> Option<Decimal> {
    let t = std::fs::read_to_string(ledger("costs", id)).ok()?;
    t.trim().replace(',', "").parse().ok()
}

pub fn delete_cost(id: &str) {
    let _ = std::fs::remove_file(ledger("costs", id));
}

/// `dirs/<id>.txt`, one folder per line (reader accepts CRLF).
pub fn record_dirs(id: &str, dirs: &[String]) {
    let path = ledger("dirs", id);
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };
    let body: String = dirs.iter().map(|d| format!("{d}{nl}")).collect();
    let _ = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&path, body));
}

pub fn recorded_dirs(id: &str) -> Vec<String> {
    std::fs::read_to_string(ledger("dirs", id)).map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default()
}

/// Clear the Recent cache and bump the generation (returned; the caller emits TranscriptsInvalidated).
pub fn invalidate() -> u64 {
    *RECENT.lock() = None;
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn a(block: &str) -> String {
        format!(r#"{{"type":"assistant","isSidechain":false,"message":{{"role":"assistant","content":[{block}]}}}}"#)
    }
    const PROMPT: &str = r#"{"type":"user","message":{"role":"user","content":"et maintenant ?"}}"#;

    fn end(lines: &[String]) -> Ending {
        end_of_lines(&lines.iter().map(String::as_str).collect::<Vec<_>>())
    }

    #[test]
    fn processes() {
        let ps = "  12 claude\n  34 node /opt/homebrew/bin/claude --model opus\n  56 claude --resume cf0a03e9-1111-2222-3333-444455556666\n  78 /tmp/claude-501/x/ClaudeCodeUI\n  90 vim claude.md";
        assert_eq!(plain_claude_pids(ps), [12, 34]);
        assert_eq!(
            cwds_from_lsof("p12\nfcwd\nn/Users/me/repo\np34\nfcwd\nn/Users/me/other/\n"),
            HashSet::from(["/Users/me/repo".to_string(), "/Users/me/other".to_string()])
        );
        assert_eq!(
            held_ids("node /opt/claude --permission-mode plan --session-id cf0a03e9-1111-2222-3333-444455556666 --name x\nclaude --resume=b45b0de6-1111-2222-3333-444455556666\nvim --resume 00000000-0000-0000-0000-000000000000"),
            HashSet::from(["cf0a03e9-1111-2222-3333-444455556666".to_string(), "b45b0de6-1111-2222-3333-444455556666".to_string()])
        );
    }

    #[test]
    fn endings() {
        let e = end(&[a(r###"{"type":"text","text":"## Fait\n\n- **PR #12** mergée, voir [la PR](https://x/12)."}"###), r#"{"type":"last-prompt","lastPrompt":"x"}"#.into()]);
        assert_eq!((e.text.as_deref(), e.stop), (Some("Fait PR #12 mergée, voir la PR."), Stop::Done));
        assert_eq!(end(&[a(r#"{"type":"text","text":"On merge ?"}"#)]).stop, Stop::Done);
        let e = end(&[a(r#"{"type":"text","text":"Je regarde."}"#), a(r#"{"type":"tool_use","id":"t","name":"AskUserQuestion","input":{}}"#)]);
        assert_eq!((e.text.as_deref(), e.stop), (Some("Je regarde."), Stop::Asked));
        let e = end(&[a(r#"{"type":"text","text":"Je lance."}"#), a(r#"{"type":"tool_use","id":"t","name":"Bash","input":{}}"#)]);
        assert_eq!(e.stop, Stop::Cut);
        let e = end(&[a(r#"{"type":"text","text":"Fini ?"}"#), PROMPT.into()]);
        assert_eq!((e.text.as_deref(), e.stop, e.unanswered), (Some("› et maintenant ?"), Stop::Cut, true));
        let e = end(&[a(r#"{"type":"text","text":"Fini."}"#), r#"{"type":"user","message":{"role":"user","content":"<local-command-stdout>x</local-command-stdout>"}}"#.into()]);
        assert_eq!(e.stop, Stop::Done);
        let e = end(&[]);
        assert_eq!((e.text, e.stop, e.tool), (None, Stop::Done, None));
        let e = end(&[
            a(r#"{"type":"text","text":"Je propose deux options."}"#),
            a(r#"{"type":"tool_use","id":"q","name":"AskUserQuestion","input":{}}"#),
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}"#.into(),
        ]);
        assert_eq!((e.text.as_deref(), e.stop, e.interrupted), (Some("Je propose deux options."), Stop::Cut, true));
        let e = end(&[
            a(r#"{"type":"tool_use","id":"t","name":"Bash","input":{"command":"dotnet test"}}"#),
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"ok"}]}}"#.into(),
        ]);
        assert_eq!((e.text, e.stop, e.tool.as_deref(), e.target.as_deref()), (None, Stop::Cut, Some("Bash"), Some("dotnet test")));
        let e = end(&[PROMPT.into(), a(r#"{"type":"tool_use","id":"t","name":"Bash","input":{}}"#)]);
        assert_eq!((e.text.as_deref(), e.stop), (Some("› et maintenant ?"), Stop::Cut));
    }

    fn write_lines(path: &Path, lines: &[&str], append: bool) {
        let mut f = std::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
    }

    #[test]
    fn read_metadata() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(Some(dir.path()));
        let path = dir.path().join("s.jsonl");
        write_lines(&path, &[
            r#"{"type":"permission-mode","permissionMode":"default","sessionId":"s"}"#,
            r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"<command-name>/x</command-name>"},"cwd":"C:\\repo\\api","gitBranch":"main","timestamp":"2026-10-09T12:00:00Z"}"#,
            r#"{"parentUuid":"a","isSidechain":false,"type":"user","message":{"role":"user","content":"corrige   le test\nd'auth"},"cwd":"C:\\repo\\api","gitBranch":"main","timestamp":"2026-10-09T12:00:01Z"}"#,
            r#"{"parentUuid":"b","isSidechain":false,"type":"assistant","message":{"model":"claude-haiku-5-5","id":"m1","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"C:\\repo\\api\\a.txt","old_string":"a","new_string":"b"}}]},"timestamp":"2026-10-09T12:00:02Z"}"#,
            r#"{"type":"last-prompt","lastPrompt":"corrige le test d'auth","sessionId":"s"}"#,
            r#"{"type":"cost-state","sessionId":"s","totalCostUSD":0.66}"#,
        ], false);
        let p = read(&path, true).unwrap();
        assert_eq!((p.cwd.as_str(), p.branch.as_deref(), p.title.as_str()), (r"C:\repo\api", Some("main"), "corrige le test d'auth"));
        assert_eq!((p.cost_usd, p.worktree_path.as_deref(), p.mode.as_deref()), (Some(Decimal::new(66, 2)), None, Some("default")));
        assert_eq!(p.id, "s");
        write_lines(&path, &[r#"{"type":"permission-mode","permissionMode":"plan","sessionId":"s"}"#], true);
        assert_eq!(read(&path, true).unwrap().mode.as_deref(), Some("plan"));
        write_lines(&path, &[r#"{"type":"custom-title","customTitle":"auth-fix","sessionId":"s"}"#], true);
        assert_eq!(read(&path, true).unwrap().title, "auth-fix");
        write_lines(&path, &[r#"{"type":"pr-link","sessionId":"s","prNumber":42,"prUrl":"https://github.com/o/api/pull/42","prRepository":"o/api","timestamp":"2026-10-09T12:00:09Z"}"#], true);
        assert_eq!(read(&path, true).unwrap().pr_url.as_deref(), Some("https://github.com/o/api/pull/42"));
        let kept: Vec<String> = std::fs::read_to_string(&path).unwrap().lines().filter(|l| !l.contains("-title") && !l.contains("last-prompt")).map(String::from).collect();
        write_lines(&path, &kept.iter().map(String::as_str).collect::<Vec<_>>(), false);
        assert_eq!(read(&path, true).unwrap().title, "corrige le test d'auth", "first user text, tags skipped");

        // The cost-state is stale (clean exits only); the UI ledger carries the real spend.
        let cid = uuid::Uuid::new_v4().to_string();
        let cpath = dir.path().join(format!("{cid}.jsonl"));
        write_lines(&cpath, &[
            r#"{"type":"user","message":{"role":"user","content":"x"},"cwd":"C:\\repo\\api","gitBranch":"main"}"#,
            r#"{"type":"cost-state","sessionId":"s","totalCostUSD":0.00062763}"#,
        ], false);
        record_cost(&cid, "0.01506324".parse().unwrap());
        assert_eq!(std::fs::read_to_string(ledger("costs", &cid)).unwrap(), "0.01506324", "C# format: invariant, no newline");
        assert_eq!(read(&cpath, true).unwrap().cost_usd, Some("0.01506324".parse().unwrap()), "ledger beats stale cost-state");
        assert_eq!(cost_state(&cpath), Some("0.00062763".parse().unwrap()), "cost-state stays raw");
        delete_cost(&cid);
        assert_eq!(recorded_cost(&cid), None);
        // No cwd yet → None.
        let empty = dir.path().join("e.jsonl");
        write_lines(&empty, &[r#"{"type":"cost-state","totalCostUSD":1}"#], false);
        assert!(read(&empty, true).is_none());
        paths::set_test_root(None);
    }

    #[test]
    fn big_file_reads_head_and_tail() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(Some(dir.path()));
        let path = dir.path().join("big.jsonl");
        let filler = format!(r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{}"}}]}}}}"#, "é".repeat(5000));
        let mut lines = vec![r#"{"type":"user","message":{"role":"user","content":"hello"},"cwd":"/r/app","gitBranch":"dev"}"#.to_string()];
        lines.extend(std::iter::repeat_n(filler, 40));
        lines.push(r#"{"type":"ai-title","aiTitle":"Big one"}"#.into());
        write_lines(&path, &lines.iter().map(String::as_str).collect::<Vec<_>>(), false);
        assert!(std::fs::metadata(&path).unwrap().len() > 2 * CHUNK);
        let p = read(&path, true).unwrap();
        assert_eq!((p.cwd.as_str(), p.title.as_str()), ("/r/app", "Big one"));
        paths::set_test_root(None);
    }

    #[test]
    fn store_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(Some(dir.path()));
        let proj = paths::projects_root().join(text::slug("/r/api"));
        let wt = paths::projects_root().join(text::slug("/r/api/.claude/worktrees/x"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(wt.join("abc").join("subagents")).unwrap();
        let line = |cwd: &str, text: &str| format!(r#"{{"type":"user","message":{{"role":"user","content":"{text}"}},"cwd":"{cwd}","timestamp":"2026-10-09T12:00:01Z"}}"#);
        write_lines(&proj.join("aaa-1.jsonl"), &[&line("/r/api", "fix the auth bug")], false);
        write_lines(&wt.join("bbb-2.jsonl"), &[&line("/r/api/.claude/worktrees/x", "other"), r#"{"type":"pr-link","prUrl":"https://github.com/o/api/pull/42/"}"#], false);
        write_lines(&wt.join("abc").join("subagents").join("ccc.jsonl"), &[&line("/r", "auth hidden")], false);

        assert!(find("aaa-1").is_some() && find("ccc").is_none(), "top-level files only");
        assert!(by_id("aaa-1").is_some() && by_id("../aaa-1").is_none() && by_id("").is_none());
        assert!(used("/r/api/") && !used("/r/nope"));
        assert_eq!(recent(30).len(), 2);
        let g = generation();
        assert_eq!(invalidate(), g + 1);
        let r = search("AUTH", 10, &|| false, SEARCH_BUDGET);
        assert!(r.complete && r.hits.len() == 1 && r.hits[0].session.id == "aaa-1" && r.hits[0].snippet == "fix the auth bug");
        assert!(!search("auth", 10, &|| false, 1).complete, "budget");
        assert!(search("  ", 10, &|| false, SEARCH_BUDGET).hits.is_empty());
        assert_eq!(linked_to("#42", "/r/api").map(|p| p.id).as_deref(), Some("bbb-2"));
        assert_eq!(linked_to("https://github.com/O/api/pull/42", "/r/api/.claude/worktrees/y").map(|p| p.id).as_deref(), Some("bbb-2"));
        assert!(linked_to("43", "/r/api").is_none() && linked_to("v42", "/r/api").is_none());
        assert_eq!(since(chrono::Utc::now() - chrono::Duration::days(1)).len(), 2);

        record_dirs("aaa-1", &["/a".into(), "/b c".into()]);
        assert_eq!(recorded_dirs("aaa-1"), ["/a", "/b c"]);
        std::fs::write(ledger("dirs", "win"), "C:\\a\r\nC:\\b\r\n").unwrap();
        assert_eq!(recorded_dirs("win"), ["C:\\a", "C:\\b"], "a C# file written on Windows");
        assert!(recorded_dirs("none").is_empty());
        paths::set_test_root(None);
    }

    #[test]
    fn matching() {
        let m = match_line(r#"{"type":"user","message":{"role":"user","content":"corrige le test d'AUTH"},"timestamp":"2026-10-09T12:00:01Z"}"#, "auth").unwrap();
        assert_eq!(m.0, "corrige le test d'AUTH");
        assert_eq!(m.1, json::parse_time("2026-10-09T12:00:01Z"));
        assert_eq!(match_line(r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"see auth"}]}}"#, "auth").unwrap().1, None);
        assert_eq!(
            match_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t","name":"Read","input":{"file_path":"auth.cs"}},{"type":"text","text":"Fixed auth."}]}}"#, "auth").unwrap().0,
            "Fixed auth."
        );
        assert!(match_line(r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"auth ok"}]}}"#, "auth").is_none());
        assert!(match_line(r#"{"type":"user","message":{"role":"user","content":"<command-name>/auth</command-name>"}}"#, "auth").is_none());
        assert!(match_line(r#"{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"auth"}]}}"#, "auth").is_none());
        assert!(match_line(r#"{"type":"last-prompt","lastPrompt":"auth"}"#, "auth").is_none());
        let quoted = r#"{"type":"user","message":{"role":"user","content":"run \"make\" in C:\\src"}}"#;
        assert!(contains_ci(quoted, &raw("\"MAKE\" in c:\\").unwrap()) && match_line(quoted, "\"make\" in C:\\").is_some());
        assert_eq!(raw("a\tb"), None);
        assert_eq!(field(r#"{"cwd":"\q"}"#, "cwd"), None, "bad escape in a chunk: no field, no throw");
        assert_eq!(field(r#"x {"cwd":"C:\\r\"q\""} "#, "cwd").as_deref(), Some(r#"C:\r"q""#));
        assert_eq!(
            match_line(r###"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"## Plan\n- **Dir:** `/a/b` use it\n\n| # | x |\n|---|---|\n| | 3 | Contrôle |"}]}}"###, "use").unwrap().0,
            "Plan - Dir: /a/b use it # · x 3 · Contrôle"
        );
        assert_eq!(match_line(r#"{"type":"user","message":{"role":"user","content":"run `make` now"}}"#, "`make`").unwrap().0, "run `make` now");
    }

    #[test]
    fn real_paths() {
        let dir = tempfile::tempdir().unwrap();
        let canon = dunce::canonicalize(dir.path()).unwrap();
        let p = dir.path().join("missing").join("deeper");
        assert_eq!(real(&p.to_string_lossy()), canon.join("missing").join("deeper").to_string_lossy());
        assert_eq!(real(&format!("{}/", dir.path().display())), canon.to_string_lossy());
    }

    #[test]
    fn prompts_in_lines() {
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"first"}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"x"}]}}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"}}"#,
            r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"side"}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-name>/compact</command-name>\n<command-message>compact</command-message>\n<command-args></command-args>"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":"second"}}"#,
        ];
        assert_eq!(prompts_in(&lines), ["first", "/compact", "second"]);
    }
}
