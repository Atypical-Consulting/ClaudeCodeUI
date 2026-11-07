//! Port of `Core/TranscriptStore.cs` (metadata reader, recents, search, endings, prompts, held processes, ledgers)
//! — spec 03 §1. Owner: core-store-sessions. Replay lives in `replay.rs`; text helpers in `ccui_shared::text`.
//!
//! Reads the CLI's own transcripts `projects/<slug>/<id>.jsonl` (top-level files only). Opens share
//! read+write+delete on Windows (the CLI appends concurrently). File-scanning functions are blocking: commands wrap
//! them in `tokio::task::spawn_blocking`.

use ccui_shared::api::{Ending, PastSession, SearchResult, TranscriptHit};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};

/// Recent(take = 30) default.
pub const RECENT_TAKE: usize = 30;
/// Search budget (bytes read before giving up: partial result).
pub const SEARCH_BUDGET: u64 = 256 * 1024 * 1024;

/// Current id per palette search (`search_transcripts(search_id, …)`): a scan stops when its id is no longer current.
#[derive(Default)]
pub struct SearchRegistry {
    pub current: AtomicU64,
}

/// Newest transcripts of the last 30 days (cached 30 s), skipping unreadable / cwd-less files.
pub fn recent(take: usize) -> Vec<PastSession> {
    todo!("port: TranscriptStore.Recent")
}

/// Every transcript modified since `since` (Overview week; cached 60 s).
pub fn since(since: ccui_shared::Time) -> Vec<PastSession> {
    todo!("port: TranscriptStore.Since")
}

/// Metadata of one transcript (head 64 KiB + tail 64 KiB when big). None = unreadable or no cwd yet.
pub fn read(file: &Path, ledger: bool) -> Option<PastSession> {
    todo!("port: TranscriptStore.Read")
}

/// First `"NAME":"…"` string anywhere in the text, JSON-decoded; empty or bad escape → None.
pub fn field(text: &str, name: &str) -> Option<String> {
    todo!("port: TranscriptStore.Field")
}

/// `projects/<dir>/<id>.jsonl` of any project dir.
pub fn find(id: &str) -> Option<PathBuf> {
    todo!("port: TranscriptStore.Find")
}

/// Any transcript by id; id must be non-empty `[A-Za-z0-9-]+` (it comes from the URL).
pub fn by_id(id: &str) -> Option<PastSession> {
    todo!("port: TranscriptStore.ById")
}

/// How the conversation ended (last 64 KiB, then 512 KiB when only tool traffic).
pub fn end_of(id: &str) -> Option<Ending> {
    todo!("port: TranscriptStore.EndOf(id)")
}

/// Walk the lines backwards (spec 03 §1.8 rules).
pub fn end_of_lines(lines: &[&str]) -> Ending {
    todo!("port: TranscriptStore.EndOf(lines)")
}

/// Raw cost-state of a transcript (no ledger).
pub fn cost_state(file: &Path) -> Option<Decimal> {
    todo!("port: TranscriptStore.CostState")
}

/// `CostState(Find(id)) ?? 0` — where `--resume` makes total_cost_usd start again.
pub fn persisted_cost(id: &str) -> Decimal {
    todo!("port: TranscriptStore.PersistedCost")
}

/// "Has claude ever run in this folder?" (`projects/slug(cwd)` exists; no realpath).
pub fn used(cwd: &str) -> bool {
    todo!("port: TranscriptStore.Used")
}

/// Resolved path (the CLI keys transcripts by the realpath): canonicalize the longest existing ancestor.
pub fn real(path: &str) -> String {
    todo!("port: TranscriptStore.Real")
}

/// Session ids named on any running claude command line (cached 5 s, stale-while-revalidate).
pub fn held_by_claude() -> HashSet<String> {
    todo!("port: TranscriptStore.HeldByClaude")
}

/// Clear the held cache (a scan started before never writes back).
pub fn forget_held() {
    todo!("port: TranscriptStore.ForgetHeld")
}

/// `ps -Ao args=` output → ids after `--session-id` / `--resume` on lines containing "claude".
pub fn held_ids(ps: &str) -> HashSet<String> {
    todo!("port: TranscriptStore.HeldIds")
}

/// Folders where a plain `claude` (no session id) runs (`ps` + `lsof`); uncached; empty on Windows.
pub fn plain_claude_cwds() -> HashSet<String> {
    todo!("port: TranscriptStore.PlainClaudeCwds")
}

pub fn plain_claude_pids(ps: &str) -> Vec<u32> {
    todo!("port: TranscriptStore.PlainClaudePids")
}

pub fn cwds_from_lsof(o: &str) -> HashSet<String> {
    todo!("port: TranscriptStore.CwdsFromLsof")
}

/// Composer ↑ history: typed prompts of the 10 newest transcripts of the folder (except `except_id`), newest first.
pub fn prompts(cwd: &str, except_id: Option<&str>, sessions: usize) -> Vec<String> {
    todo!("port: TranscriptStore.Prompts")
}

pub fn prompts_in(lines: &[&str]) -> Vec<String> {
    todo!("port: TranscriptStore.PromptsIn")
}

/// Ctrl K message search (one hit per session, newest first). `cancelled` is polled per line.
pub fn search(query: &str, take: usize, cancelled: &dyn Fn() -> bool, budget: u64) -> SearchResult {
    todo!("port: TranscriptStore.Search")
}

/// One line's first visible text containing the query → (snippet, at).
pub fn match_line(line: &str, query: &str) -> Option<(String, Option<ccui_shared::Time>)> {
    todo!("port: TranscriptStore.Match")
}

/// `--from-pr` lookup: the repo's (and its worktrees') transcript linked to that PR.
pub fn linked_to(pr: &str, cwd: &str) -> Option<PastSession> {
    todo!("port: TranscriptStore.LinkedTo")
}

/// `costs/<id>.txt`, invariant decimal, no newline; errors swallowed.
pub fn record_cost(id: &str, usd: Decimal) {
    todo!("port: TranscriptStore.RecordCost")
}

pub fn recorded_cost(id: &str) -> Option<Decimal> {
    todo!("port: TranscriptStore.RecordedCost")
}

pub fn delete_cost(id: &str) {
    todo!("port: TranscriptStore.DeleteCost")
}

/// `dirs/<id>.txt`, one folder per line (reader accepts CRLF).
pub fn record_dirs(id: &str, dirs: &[String]) {
    todo!("port: TranscriptStore.RecordDirs")
}

pub fn recorded_dirs(id: &str) -> Vec<String> {
    todo!("port: TranscriptStore.RecordedDirs")
}

/// Clear the Recent cache and bump the generation (returned; the caller emits TranscriptsInvalidated).
pub fn invalidate() -> u64 {
    todo!("port: TranscriptStore.Invalidate")
}

pub fn generation() -> u64 {
    todo!("port: TranscriptStore.Generation")
}
