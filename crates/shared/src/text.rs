//! Port of the text/path helpers of `Core/TranscriptStore.cs` (shared with Overview, CommandPalette, NewSession) —
//! spec 03 §1.9, plus `NewSession.Chip` (spec 06 §6.1). Owner: core-store-sessions.
//! Lengths are in chars (C# counted UTF-16 units); always slice on char boundaries.

/// Collapse whitespace runs; > 60 chars → first 59 + "…"; blank → None.
pub fn trunc(s: Option<&str>) -> Option<String> {
    todo!("port: TranscriptStore.Trunc")
}

/// ≤ 220 chars → s, else first 219 trimmed + "…".
pub fn cut(s: &str) -> String {
    todo!("port: TranscriptStore.Cut")
}

/// Markdown → plain one line (fences, link labels, `plain`, list/quote markers, emoji incl. every char > U+FFFF).
pub fn prose(md: &str) -> String {
    todo!("port: TranscriptStore.Prose")
}

/// Markdown as read: headings, `**`, backticks and table rules removed; table rows → cells joined " · ".
pub fn plain(t: &str) -> String {
    todo!("port: TranscriptStore.Plain")
}

/// The last ~max chars started at a sentence (or the trailing question) when possible.
pub fn tail(s: &str, max: usize) -> String {
    todo!("port: TranscriptStore.Tail")
}

/// Whitespace-collapsed `text[at-ctx .. at+length+ctx]` with "…" on cut sides (`at`, `length` in chars).
pub fn snippet(text: &str, at: usize, length: usize, ctx: usize) -> String {
    todo!("port: TranscriptStore.Snippet")
}

/// Every char not `[A-Za-z0-9]` → '-' (the CLI's project-dir naming).
pub fn slug(cwd: &str) -> String {
    todo!("port: TranscriptStore.Slug")
}

/// Last segment, cut at `/.claude/worktrees/` (both separators accepted).
pub fn repo_name(cwd: &str) -> String {
    todo!("port: TranscriptStore.RepoName")
}

/// cwd before `[\\/]\.claude[\\/]worktrees[\\/]` (ignore case), else cwd.
pub fn root_of(cwd: &str) -> String {
    todo!("port: TranscriptStore.RootOf")
}

/// `home + sep` prefix (case-insensitive) → "~" + rest.
pub fn short(path: &str, home: &str, sep: &str) -> String {
    todo!("port: TranscriptStore.Short")
}

/// NFD, marks dropped, lowercase, `[a-z0-9]+` words, first `words`, joined '-'.
pub fn name_from(text: &str, words: usize) -> String {
    todo!("port: TranscriptStore.NameFrom")
}

/// `name_from(typed, 8)` or None when empty.
pub fn worktree_name(typed: &str) -> Option<String> {
    todo!("port: TranscriptStore.WorktreeName")
}

/// `adj-ing-animal` from the three fixed 20-word lists; `seed` picks (the UI passes a random u64).
pub fn random_name(seed: u64) -> String {
    todo!("port: TranscriptStore.RandomName")
}

/// PR number of `42`, `#42`, `…/pull/42/`, `…/-/merge_requests/7`; None for issues or `v42`.
pub fn pr_number(s: &str) -> Option<String> {
    todo!("port: TranscriptStore.PrNumber")
}

/// `NewSession.Chip`: `short(dir)` with `\` → `/`; ≤ 32 chars as is, else "…/" + last two segments.
pub fn chip(dir: &str, home: &str, sep: &str) -> String {
    todo!("port: NewSession.Chip")
}
