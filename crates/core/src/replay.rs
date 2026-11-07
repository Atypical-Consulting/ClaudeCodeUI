//! Port of `TranscriptStore.Load / Replay / ActivePrompts / Apply / Step` — rebuild a thread from a transcript
//! through the same parser and reducer as a live session — spec 03 §1.11-1.12. Owner: core-store-sessions.
//! One bad line never loses the session (catch-all, logged `transcript: line skipped (…)`).
//! Tests: the replay fixtures of `TranscriptStore.Check()` verbatim.

use crate::events::ClaudeEvent;
use crate::live::state::SessionState;
use ccui_shared::model::{Item, Time};
use serde_json::Value;
use std::collections::HashSet;

/// Items of transcript `id` (Find → read lines → replay); unknown id → [].
pub fn load(id: &str) -> Vec<Item> {
    todo!("port: TranscriptStore.Load")
}

/// Replay raw .jsonl lines (prefilter, `toolUseResult` → `tool_use_result`, rewind chain, attachments, Shown/Hidden,
/// timestamps), then unfinished Running/Waiting tools → Error with ended_at = ended_at ?? started_at.
pub fn replay(lines: &[&str]) -> Vec<Item> {
    todo!("port: TranscriptStore.Replay")
}

/// None when no line contains `"rewound":true`; else the uuids on the chain from the last leaf.
pub fn active_prompts(lines: &[&str]) -> Option<HashSet<String>> {
    todo!("port: TranscriptStore.ActivePrompts")
}

/// Every event of one parsed line through `step` (exported: WorkflowRuns' agent log uses it).
pub fn apply(s: &mut SessionState, line: &Value, at: Option<Time>) {
    todo!("port: TranscriptStore.Apply")
}

/// Reduce, then timestamp fix-ups (tool started_at, ended_at; TaskDone without duration).
pub fn step(s: &mut SessionState, ev: &ClaudeEvent, at: Option<Time>) {
    todo!("port: TranscriptStore.Step")
}

/// type user, not meta/sidechain, has content that is a string or an array with no tool_result.
pub fn is_prompt(line: &Value) -> bool {
    todo!("port: TranscriptStore.IsPrompt")
}
