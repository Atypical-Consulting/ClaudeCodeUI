//! Port of the records and pure rules of `Core/WorktreeService.cs` (classify, cleanable, plan, anchor…) — spec 04
//! §1.2, §1.7 (pure helpers), §1.8-1.10. Owner: core-worktrees-files. Scans and git I/O are `ccui_core::worktrees`.
//! INVARIANT: no plan ever contains `--force`, `-f`, `-D`, `--delete-force`.

use crate::model::Time;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WtState {
    Safe,
    Check,
    Active,
    Orphan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    /// Normalized repo root.
    pub repo: String,
    /// Normalized worktree path; branch-only rows: "{repo}/refs/heads/{branch}".
    pub path: String,
    /// Folder name; branch-only rows: the full branch name.
    pub name: String,
    /// None = detached HEAD.
    pub branch: Option<String>,
    pub head: Option<String>,
    pub state: WtState,
    /// HTML (`<b>…</b>`; interpolated values encoded) — rendered raw.
    pub why: String,
    /// Always None from a scan (measured by `worktrees_size`).
    pub size_bytes: Option<i64>,
    /// max(index mtime, branch committerdate); None when unknown (C# DateTime.MinValue).
    pub last_activity: Option<Time>,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub dirty: i32,
    pub unpushed: i32,
    pub merged: bool,
    pub squash_merged: bool,
    pub upstream_gone: bool,
    pub branch_only: bool,
    pub unreadable: Option<String>,
    /// Remote ref holding every commit, confirmed live on origin.
    pub pushed: Option<String>,
    pub recent_min: Option<i32>,
    /// Ignored files that are not build output ("a, b +3").
    pub kept: Option<String>,
}

impl WorktreeInfo {
    /// merged || squash_merged ? 0 : unpushed.
    pub fn at_risk(&self) -> i32 {
        if self.merged || self.squash_merged {
            0
        } else {
            self.unpushed
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CleanupStep {
    pub repo: String,
    /// "git " + args, repo-prefixed args shown relative.
    pub display: String,
    pub git_args: Vec<String>,
    pub failure_is_fatal: bool,
    /// "Delete anyway" acknowledged for this path's kept files.
    pub kept_ok: bool,
}

/// Input of `classify` (all default 0/false/None except exists = true).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WtFacts {
    pub dirty: i32,
    pub ahead: i32,
    pub merged: bool,
    pub squash_merged: bool,
    pub upstream_gone: bool,
    pub locked: bool,
    pub lock_pid: Option<i32>,
    pub pid_alive: bool,
    pub exists: bool,
    pub prunable: bool,
    pub active_session_name: Option<String>,
    pub detached: bool,
    pub no_worktree: bool,
    pub unreadable: Option<String>,
    pub on_remote: Option<String>,
    pub in_use_by: Option<String>,
    pub recent_min: Option<i32>,
    pub twin_of: Option<String>,
    pub twin_active: bool,
    pub kept: Option<String>,
    pub on_remote_stale: Option<String>,
    pub remote_gone: Option<String>,
    pub merged_pr: Option<i32>,
    pub dirty_names: Option<String>,
}

impl Default for WtFacts {
    fn default() -> Self {
        WtFacts {
            dirty: 0,
            ahead: 0,
            merged: false,
            squash_merged: false,
            upstream_gone: false,
            locked: false,
            lock_pid: None,
            pid_alive: false,
            exists: true,
            prunable: false,
            active_session_name: None,
            detached: false,
            no_worktree: false,
            unreadable: None,
            on_remote: None,
            in_use_by: None,
            recent_min: None,
            twin_of: None,
            twin_active: false,
            kept: None,
            on_remote_stale: None,
            remote_gone: None,
            merged_pr: None,
            dirty_names: None,
        }
    }
}

fn enc(s: &str) -> String {
    crate::fmt::enc(s)
}

/// `(state, why_html)` with the recency wrapper (spec 04 §1.8). `{base}` stays literal in the why.
pub fn classify(f: &WtFacts) -> (WtState, String) {
    let c = classify_core(f);
    let Some(m) = f.recent_min else { return c };
    // A stale lock is cleanable once unlocked: when recency is what still holds it back, the reason must say recency.
    let held = c.0 == WtState::Safe
        || c.0 == WtState::Check
            && f.locked
            && !f.pid_alive
            && classify_core(&WtFacts { locked: false, ..f.clone() }).0 == WtState::Safe;
    if held {
        (WtState::Check, crate::t!("Wt.Why.Recent", m.max(1)))
    } else {
        c
    }
}

/// Pushed / Unconfirmed / RemoteGone / BranchUnmerged chain of branch-only rows.
fn branch_check(f: &WtFacts) -> (WtState, String) {
    let why = if let Some(r) = &f.on_remote {
        crate::t!("Wt.Why.Pushed", enc(r))
    } else if let Some(s) = &f.on_remote_stale {
        crate::t!("Wt.Why.Unconfirmed", enc(s))
    } else if let Some(g) = &f.remote_gone {
        crate::t!("Wt.Why.RemoteGone", enc(g))
    } else {
        crate::t!("Wt.Why.BranchUnmerged")
    };
    (WtState::Check, why)
}

/// The 24-rule core, first match wins.
pub fn classify_core(f: &WtFacts) -> (WtState, String) {
    use crate::t;
    use WtState::*;
    if let Some(s) = &f.active_session_name {
        return (Active, t!("Wt.Why.Running", enc(s)));
    }
    if let Some(u) = &f.in_use_by {
        return (Active, t!("Wt.Why.InUse", enc(u)));
    }
    if let (true, Some(pid), true) = (f.locked, f.lock_pid, f.pid_alive) {
        return (Active, t!("Wt.Why.External", pid));
    }
    if let Some(err) = &f.unreadable {
        return (Check, t!("Wt.Why.Unreadable", enc(err)));
    }
    if f.no_worktree {
        if let Some(n) = f.merged_pr {
            return (Check, t!("Wt.Why.MergedPrBranch", n)); // -d refuses a squashed branch: named, not offered
        }
        if !f.merged && f.squash_merged {
            return (Check, t!("Wt.Why.SquashBranch"));
        }
        if let Some(tw) = &f.twin_of {
            return if f.twin_active {
                (Active, t!("Wt.Why.TwinActive", enc(tw)))
            } else if f.merged {
                (Orphan, t!("Wt.Why.TwinMerged", enc(tw)))
            } else {
                branch_check(f)
            };
        }
        return if f.merged { (Orphan, t!("Wt.Why.BranchMerged")) } else { branch_check(f) };
    }
    if !f.exists || f.prunable {
        return (Orphan, t!("Wt.Why.Deleted"));
    }
    let unpushed = f.ahead > 0 && !f.merged && !f.squash_merged;
    if f.dirty > 0 {
        let mut why = if f.merged {
            t!("Wt.Why.DirtyMerged")
        } else if let Some(dp) = f.merged_pr {
            t!("Wt.Why.DirtyPr", dp)
        } else if f.squash_merged {
            t!("Wt.Why.DirtySquashed")
        } else {
            String::new()
        };
        if unpushed {
            why += &crate::i18n::plural(f.ahead as i64, "Wt.Why.Ahead");
            why += " · ";
        }
        why += &crate::i18n::plural(f.dirty as i64, "Wt.Why.Dirty");
        if let Some(dn) = &f.dirty_names {
            why += ": ";
            why += &enc(dn);
        }
        return (Check, why);
    }
    if unpushed {
        return (Check, crate::i18n::plural(f.ahead as i64, "Wt.Why.Ahead"));
    }
    if f.locked {
        return match f.lock_pid {
            Some(dead) => (Check, t!("Wt.Why.StaleLock", dead)),
            None => (Check, t!("Wt.Why.Locked")),
        };
    }
    if let Some(k) = &f.kept {
        return (Check, t!("Wt.Why.Kept", enc(k))); // removing the folder would delete them
    }
    if let Some(n) = f.merged_pr {
        return (Safe, t!(if f.detached { "Wt.Why.DetachedPr" } else { "Wt.Why.MergedPr" }, n));
    }
    if f.detached && !f.merged {
        if f.squash_merged {
            return (Safe, t!("Wt.Why.DetachedSquashed")); // no branch to keep
        }
        return if let Some(d) = &f.on_remote {
            (Safe, t!("Wt.Why.DetachedPushed", enc(d)))
        } else if let Some(ds) = &f.on_remote_stale {
            (Check, t!("Wt.Why.Unconfirmed", enc(ds)))
        } else if let Some(dg) = &f.remote_gone {
            (Check, t!("Wt.Why.DetachedGone", enc(dg)))
        } else {
            (Check, t!("Wt.Why.Detached"))
        };
    }
    if f.merged && f.upstream_gone {
        return (Safe, t!("Wt.Why.MergedGone"));
    }
    if f.merged {
        return (Safe, t!("Wt.Why.Merged"));
    }
    if f.squash_merged {
        return (Safe, t!("Wt.Why.Squashed"));
    }
    if let Some(o) = &f.on_remote {
        return (Safe, t!("Wt.Why.Pushed", enc(o))); // the folder goes, the branch stays (plan)
    }
    if let Some(os) = &f.on_remote_stale {
        return (Check, t!("Wt.Why.Unconfirmed", enc(os)));
    }
    if let Some(og) = &f.remote_gone {
        return (Check, t!("Wt.Why.RemoteGone", enc(og)));
    }
    (Check, t!("Wt.Why.NotMerged"))
}

/// Rebuild facts from a row (exact quirks of spec 04 §1.9).
pub fn facts_of(w: &WorktreeInfo) -> WtFacts {
    WtFacts {
        dirty: w.dirty,
        ahead: w.unpushed,
        merged: w.merged,
        squash_merged: w.squash_merged,
        upstream_gone: w.upstream_gone,
        locked: w.locked,
        lock_pid: pid_of(w.lock_reason.as_deref()),
        pid_alive: false,
        exists: w.state != WtState::Orphan,
        prunable: w.state == WtState::Orphan,
        detached: w.branch.is_none(),
        unreadable: w.unreadable.clone(),
        on_remote: w.pushed.clone(),
        recent_min: w.recent_min,
        kept: w.kept.clone(),
        ..WtFacts::default()
    }
}

/// Safe/Orphan, or a stale lock that classifies Safe unlocked, or (kept_ok) Check held only by kept files.
pub fn cleanable(w: &WorktreeInfo, kept_ok: bool) -> bool {
    let lock_pid = pid_of(w.lock_reason.as_deref());
    matches!(w.state, WtState::Safe | WtState::Orphan)
        || w.state == WtState::Check
            && w.locked
            && lock_pid.is_some()
            && classify(&WtFacts { locked: false, ..facts_of(w) }).0 == WtState::Safe
        || kept_ok
            && w.kept.is_some()
            && w.state == WtState::Check
            && classify(&WtFacts { kept: None, locked: w.locked && lock_pid.is_none(), ..facts_of(w) }).0 == WtState::Safe
}

/// Non-destructive git steps for the cleanable rows, grouped by repo (spec 04 §1.10). `kept_ok` = acknowledged paths.
pub fn plan(rows: &[WorktreeInfo], kept_ok: &[String]) -> Vec<CleanupStep> {
    let acked = |p: &str| kept_ok.iter().any(|k| k == p);
    // Group by repo (case-insensitive), in order of first appearance.
    let mut groups: Vec<(String, Vec<&WorktreeInfo>)> = Vec::new();
    for w in rows.iter().filter(|w| cleanable(w, acked(&w.path))) {
        match groups.iter_mut().find(|(r, _)| r.to_lowercase() == w.repo.to_lowercase()) {
            Some((_, g)) => g.push(w),
            None => groups.push((w.repo.clone(), vec![w])),
        }
    }
    let mut steps = Vec::new();
    for (repo, g) in groups {
        let step = |fatal: bool, args: &[&str]| {
            // An arg starting with the repo is shown relative to it.
            let shown: Vec<&str> =
                args.iter().map(|a| a.strip_prefix(repo.as_str()).and_then(|r| r.strip_prefix('/')).unwrap_or(a)).collect();
            CleanupStep {
                repo: repo.clone(),
                display: format!("git {}", shown.join(" ")),
                git_args: args.iter().map(|a| a.to_string()).collect(),
                failure_is_fatal: fatal,
                kept_ok: false,
            }
        };
        for w in g.iter().filter(|w| w.branch_only) {
            if let Some(b) = &w.branch {
                steps.push(step(false, &["branch", "-d", b])); // no folder: just the branch
            }
        }
        let wts: Vec<&WorktreeInfo> = g.iter().copied().filter(|w| !w.branch_only).collect();
        for w in wts.iter().filter(|w| w.locked && w.state == WtState::Orphan) {
            steps.push(step(true, &["worktree", "unlock", &w.path]));
        }
        for w in wts.iter().filter(|w| w.state != WtState::Orphan) {
            if w.locked {
                steps.push(step(true, &["worktree", "unlock", &w.path]));
            }
            steps.push(step(true, &["worktree", "remove", &w.path]));
            if let (Some(b), true) = (&w.branch, w.merged) {
                steps.push(step(false, &["branch", "-d", b])); // squash or only pushed: the branch is kept
            }
        }
        if wts.iter().any(|w| w.state == WtState::Orphan) {
            steps.push(step(true, &["worktree", "prune", "-v"]));
            for w in wts.iter().filter(|w| w.state == WtState::Orphan && w.merged) {
                if let Some(b) = &w.branch {
                    steps.push(step(false, &["branch", "-d", b]));
                }
            }
        }
    }
    // The acknowledgement travels with the steps, so the run re-validates the row under the same terms.
    for st in &mut steps {
        if st.git_args.len() == 3 && st.git_args[0] == "worktree" && acked(&st.git_args[2]) {
            st.kept_ok = true;
        }
    }
    steps
}

/// branch ?? name.
pub fn display_name(w: &WorktreeInfo) -> &str {
    w.branch.as_deref().unwrap_or(&w.name)
}

/// `"wt-" + sha1_hex(path)[..12]` (stable DOM id).
pub fn anchor(path: &str) -> String {
    use sha1::{Digest, Sha1};
    let hex = hex::encode(Sha1::digest(path.as_bytes()));
    format!("wt-{}", &hex[..12])
}

/// `Wt.State.{Safe,Orphan,Active,Check}`.
pub fn label(state: WtState) -> String {
    crate::t!(match state {
        WtState::Safe => "Wt.State.Safe",
        WtState::Orphan => "Wt.State.Orphan",
        WtState::Active => "Wt.State.Active",
        WtState::Check => "Wt.State.Check",
    })
}

/// Basenames (rename → right side), deduped; first 2 joined ", " + " +N".
pub fn names_of(paths: &[String]) -> Option<String> {
    let mut names: Vec<&str> = Vec::new();
    for p in paths {
        let right = p.rsplit(" -> ").next().unwrap_or(p).trim_end_matches('/');
        let base = right.rsplit(['/', '\\']).next().unwrap_or("");
        let n = if base.is_empty() { p.as_str() } else { base };
        if !names.contains(&n) {
            names.push(n);
        }
    }
    if names.is_empty() {
        return None;
    }
    let head = names[..names.len().min(2)].join(", ");
    Some(if names.len() > 2 { format!("{head} +{}", names.len() - 2) } else { head })
}

static REGENERABLE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(^|/)(bin|obj|target|node_modules|dist|build|out|\.build|\.next|\.nuxt|\.cache|__pycache__|\.venv|venv|\.vs|\.idea|coverage|TestResults|\.gradle|\.dart_tool|\.turbo|\.parcel-cache|\.pytest_cache|\.mypy_cache|\.swiftpm|src-tauri/gen|src-tauri/server|\.nuke/temp)/$|(^|/)\.DS_Store$|\.pyc$|\.cache\.json$").unwrap()
});

/// Ignored entries that are not regenerable build output (`ls-files --others --ignored --directory` output).
pub fn kept_of(ls: &str) -> Vec<String> {
    ls.split('\n').map(str::trim).filter(|l| !l.is_empty() && !REGENERABLE.is_match(l)).map(String::from).collect()
}

/// "a, b" / "a, b +N" / None.
pub fn kept_summary(kept: &[String]) -> Option<String> {
    match kept.len() {
        0 => None,
        1 | 2 => Some(kept.join(", ")),
        n => Some(format!("{}, {} +{}", kept[0], kept[1], n - 2)),
    }
}

/// `lsof -Fpcn` output → `(pid, name, path)` users (not self, not lsof/git, absolute paths).
pub fn parse_lsof(o: &str, self_pid: u32) -> Vec<(u32, String, String)> {
    let mut list = Vec::new();
    let (mut pid, mut name) = (0u32, String::new());
    for l in o.split('\n') {
        let mut cs = l.chars();
        let (Some(k), Some(second)) = (cs.next(), cs.next()) else { continue };
        let rest = &l[k.len_utf8()..];
        match k {
            'p' => {
                pid = rest.parse().unwrap_or(0);
                name.clear();
            }
            'c' => name = rest.to_string(),
            // our own probes are not users
            'n' if pid != 0 && pid != self_pid && second == '/' && name != "lsof" && name != "git" => {
                list.push((pid, name.clone(), rest.to_string()))
            }
            _ => {}
        }
    }
    list
}

/// `worktree-*` branches no worktree checks out (case-insensitive), sorted ordinally.
pub fn orphan_branches(branches: &[String], checked_out: &[String]) -> Vec<String> {
    let used: Vec<String> = checked_out.iter().map(|b| b.to_lowercase()).collect();
    let mut out: Vec<String> =
        branches.iter().filter(|h| h.starts_with("worktree-") && !used.contains(&h.to_lowercase())).cloned().collect();
    out.sort();
    out
}

static PID: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"pid (\d+)").unwrap());

/// `pid (\d+)` in a lock reason.
pub fn pid_of(reason: Option<&str>) -> Option<i32> {
    PID.captures(reason?).and_then(|c| c[1].parse().ok())
}

/// Is a step forbidden (`--force`, `-f`, `-D`, `--delete-force`)?
pub fn forbidden(args: &[String]) -> bool {
    args.iter().any(|a| matches!(a.as_str(), "--force" | "-f" | "-D" | "--delete-force"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set_thread_lang, Lang};

    fn f() -> WtFacts {
        WtFacts::default()
    }

    fn is(facts: WtFacts, s: WtState, what: &str) {
        assert_eq!(classify(&facts).0, s, "{what}");
    }

    #[test]
    fn classify_cases() {
        set_thread_lang(Some(Lang::Fr));
        use WtState::*;
        let some = |s: &str| Some(s.to_string());
        is(WtFacts { merged: true, active_session_name: some("x"), ..f() }, Active, "active session");
        is(WtFacts { locked: true, lock_pid: Some(42), pid_alive: true, merged: true, ..f() }, Active, "live lock");
        is(WtFacts { exists: false, merged: true, ..f() }, Orphan, "folder gone");
        is(WtFacts { prunable: true, ..f() }, Orphan, "prunable");
        is(WtFacts { dirty: 3, merged: true, ..f() }, Check, "dirty");
        is(WtFacts { ahead: 4, ..f() }, Check, "unpushed");
        is(WtFacts { locked: true, lock_pid: Some(42), merged: true, ..f() }, Check, "dead lock");
        is(WtFacts { merged: true, ..f() }, Safe, "clean merged");
        is(WtFacts { ahead: 2, squash_merged: true, ..f() }, Safe, "squash");
        is(WtFacts { merged: true, upstream_gone: true, ..f() }, Safe, "remote deleted");
        is(WtFacts { detached: true, ..f() }, Check, "detached");
        is(f(), Check, "unknown");
        is(WtFacts { no_worktree: true, merged: true, ..f() }, Orphan, "merged branch-only");
        is(WtFacts { no_worktree: true, ahead: 2, ..f() }, Check, "unmerged branch-only");
        is(WtFacts { no_worktree: true, ahead: 2, squash_merged: true, ..f() }, Check, "squash branch-only");
        is(WtFacts { merged: true, unreadable: some("fatal: detected dubious ownership"), ..f() }, Check, "unreadable");
        is(WtFacts { detached: true, on_remote: some("origin/x"), ..f() }, Safe, "detached pushed");
        is(WtFacts { on_remote: some("origin/x"), ..f() }, Safe, "pushed");
        is(WtFacts { on_remote: some("origin/x"), dirty: 1, ..f() }, Check, "pushed dirty");
        is(WtFacts { merged: true, recent_min: Some(5), ..f() }, Check, "fresh");
        is(WtFacts { merged: true, in_use_by: some("zsh (pid 9)"), ..f() }, Active, "shell inside");
        is(WtFacts { ahead: 3, recent_min: Some(5), ..f() }, Check, "recent unpushed");
        is(WtFacts { no_worktree: true, merged: true, twin_of: some("agent-a"), ..f() }, Orphan, "twin merged");
        is(WtFacts { no_worktree: true, merged: true, twin_of: some("agent-a"), twin_active: true, ..f() }, Active, "twin active");
        is(WtFacts { no_worktree: true, on_remote: some("origin/x"), ..f() }, Check, "branch-only pushed");
        is(WtFacts { merged: true, kept: some(".env"), ..f() }, Check, "kept .env");
        is(WtFacts { detached: true, on_remote_stale: some("origin/x"), ..f() }, Check, "detached stale");
        is(WtFacts { on_remote_stale: some("origin/x"), ..f() }, Check, "stale");
        is(WtFacts { remote_gone: some("origin/x"), ..f() }, Check, "remote gone");
        is(WtFacts { remote_gone: some("origin/x"), merged_pr: Some(52), ..f() }, Safe, "gone but PR merged");
        is(WtFacts { detached: true, merged_pr: Some(718), ..f() }, Safe, "detached PR head");
        is(WtFacts { merged_pr: Some(52), dirty: 1, ..f() }, Check, "PR but dirty");
        is(WtFacts { no_worktree: true, merged_pr: Some(52), ..f() }, Check, "branch-only PR");
        is(WtFacts { detached: true, squash_merged: true, ..f() }, Safe, "detached squash");
        is(WtFacts { detached: true, on_remote: some("origin/x"), kept: some("a.jsonl"), ..f() }, Check, "detached pushed kept");
        is(WtFacts { no_worktree: true, merged: true, unreadable: some("x"), ..f() }, Check, "branch-only unreadable");

        assert_eq!(
            classify(&WtFacts { detached: true, remote_gone: some("origin/x"), ..f() }).1,
            crate::t!("Wt.Why.DetachedGone", "origin/x")
        );
        assert_eq!(classify(&WtFacts { no_worktree: true, squash_merged: true, ..f() }).1, crate::t!("Wt.Why.SquashBranch"));
        assert_eq!(
            classify(&WtFacts { merged: true, locked: true, lock_pid: Some(1), recent_min: Some(4), ..f() }).1,
            crate::t!("Wt.Why.Recent", 4)
        );
        // {base} survives both raw and formatted strings; interpolated values are encoded.
        assert!(classify(&WtFacts { merged: true, ..f() }).1.contains("{base}"));
        assert!(classify(&WtFacts { dirty: 2, merged: true, dirty_names: some("<a>"), ..f() }).1.ends_with(": &lt;a&gt;"));
        set_thread_lang(None);
    }

    fn row(name: &str, facts: WtFacts, branch: Option<&str>) -> WorktreeInfo {
        WorktreeInfo {
            repo: "C:/r".into(),
            path: format!("C:/r/.claude/worktrees/{name}"),
            name: name.into(),
            branch: branch.map(|b| format!("{b}{name}")),
            head: Some("h".into()),
            state: classify(&facts).0,
            why: String::new(),
            size_bytes: None,
            last_activity: None,
            locked: facts.locked,
            lock_reason: facts.locked.then(|| format!("claude session {name} (pid {})", facts.lock_pid.unwrap_or(0))),
            dirty: facts.dirty,
            unpushed: facts.ahead,
            merged: facts.merged,
            squash_merged: facts.squash_merged,
            upstream_gone: facts.upstream_gone,
            branch_only: false,
            unreadable: None,
            pushed: None,
            recent_min: None,
            kept: None,
        }
    }

    #[test]
    fn cleanable_cases() {
        let mut kept = row("k", WtFacts { merged: true, ..f() }, Some("b"));
        kept.state = WtState::Check;
        kept.kept = Some(".env".into());
        assert!(!cleanable(&kept, false) && cleanable(&kept, true));
        assert!(!cleanable(&WorktreeInfo { dirty: 1, ..kept.clone() }, true));
        let mut u = row("u", WtFacts { merged: true, ..f() }, Some("b"));
        u.state = WtState::Check;
        u.locked = true;
        u.lock_reason = Some("claude session u (pid 7)".into());
        u.unreadable = Some("x".into());
        assert!(!cleanable(&u, false));
        assert_eq!(kept.at_risk(), 0);
    }

    #[test]
    fn plan_cases() {
        let b = Some("b");
        let mut rows = vec![
            row("safe", WtFacts { merged: true, ..f() }, b),
            row("squash", WtFacts { ahead: 2, squash_merged: true, ..f() }, b),
            row("gone", WtFacts { merged: true, upstream_gone: true, ..f() }, b),
            row("stale", WtFacts { locked: true, lock_pid: Some(7), merged: true, ..f() }, b),
            row("orph", WtFacts { exists: false, merged: true, locked: true, lock_pid: Some(8), ..f() }, b),
            row("dirty", WtFacts { dirty: 1, merged: true, ..f() }, b),
            row("ahead", WtFacts { ahead: 1, ..f() }, b),
            row("live", WtFacts { locked: true, lock_pid: Some(9), pid_alive: true, merged: true, ..f() }, b),
            row("act", WtFacts { active_session_name: Some("s".into()), ..f() }, b),
            row("det", WtFacts { detached: true, ..f() }, None),
            row("detm", WtFacts { detached: true, merged: true, ..f() }, None),
            row("unk", f(), b),
        ];
        let mut bo = row("gone2", WtFacts { merged: true, ..f() }, None);
        bo.path = "C:/r/refs/heads/worktree-gone".into();
        bo.branch = Some("worktree-gone".into());
        bo.head = None;
        bo.state = WtState::Orphan;
        bo.branch_only = true;
        rows.push(bo);
        let steps = plan(&rows, &[]);
        let all = steps.iter().map(|s| s.git_args.join(" ")).collect::<Vec<_>>().join("\n");
        assert!(steps.iter().all(|s| !forbidden(&s.git_args) && !s.git_args.iter().any(|a| a.starts_with("--force"))));
        assert!(!steps.iter().any(|s| {
            let l = s.git_args.last().unwrap();
            ["/dirty", "/ahead", "/live", "/act", "/det", "/unk"].iter().any(|e| l.ends_with(e))
        }));
        assert!(all.contains("worktree unlock C:/r/.claude/worktrees/stale") && all.contains("worktree remove C:/r/.claude/worktrees/stale"));
        assert!(all.contains("worktree remove C:/r/.claude/worktrees/squash") && !all.contains("branch -d bsquash"));
        assert!(all.contains("branch -d bsafe") && all.contains("worktree prune") && all.contains("worktree unlock C:/r/.claude/worktrees/orph"));
        assert!(all.contains("branch -d worktree-gone") && !all.contains("remove C:/r/refs/heads/worktree-gone") && !all.contains("unlock C:/r/refs"));
        assert!(all.contains("worktree remove C:/r/.claude/worktrees/detm"));
        let first = steps.iter().find(|s| s.git_args[0] == "worktree" && s.git_args[1] == "remove").unwrap();
        assert_eq!(first.display, "git worktree remove .claude/worktrees/safe");
        assert!(steps.iter().all(|s| !s.kept_ok));

        // "Delete anyway" travels with the steps of that path only.
        let mut k = row("k", WtFacts { merged: true, ..f() }, b);
        k.state = WtState::Check;
        k.kept = Some(".env".into());
        assert!(plan(&[k.clone()], &[]).is_empty());
        let p = plan(&[k.clone()], &[k.path.clone()]);
        assert!(p.iter().any(|s| s.git_args[1] == "remove" && s.kept_ok));
        assert!(p.iter().filter(|s| s.git_args[0] == "branch").all(|s| !s.kept_ok));
    }

    #[test]
    fn helpers() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(names_of(&v(&["src/a.cs", "b.razor", "c/d.js"])).as_deref(), Some("a.cs, b.razor +1"));
        assert_eq!(names_of(&v(&["old -> new.cs"])).as_deref(), Some("new.cs"));
        assert_eq!(names_of(&[]), None);
        assert_eq!(
            kept_of("bin/\nnode_modules/\n.env\nsrc/obj/\n.DS_Store\ndesktop/src-tauri/gen/\n.claude/decision-events.jsonl\nx.cache.json\n"),
            v(&[".env", ".claude/decision-events.jsonl"])
        );
        assert_eq!(kept_summary(&v(&["a", "b", "c", "d"])).as_deref(), Some("a, b +2"));
        assert_eq!(kept_summary(&v(&["a"])).as_deref(), Some("a"));
        assert_eq!(kept_summary(&[]), None);
        assert_eq!(
            parse_lsof("p1\nczsh\nn/a/b\np2\ncme\nn/self\np3\ncx\nn ignored\np4\nclsof\nn/a/b\n", 2),
            vec![(1, "zsh".to_string(), "/a/b".to_string())]
        );
        assert_eq!(orphan_branches(&v(&["main", "worktree-a", "worktree-b", "feat/x"]), &v(&["main", "worktree-b"])), v(&["worktree-a"]));
        assert_eq!(pid_of(Some("claude session u (pid 7)")), Some(7));
        assert_eq!(pid_of(None), None);
        assert!(forbidden(&v(&["branch", "-D", "x"])) && !forbidden(&v(&["branch", "-d", "x"])));
        assert_eq!(anchor("abc"), "wt-a9993e364706"); // sha1("abc") = a9993e364706816a…
    }
}

