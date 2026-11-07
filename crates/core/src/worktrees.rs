//! Port of `Core/WorktreeService.cs` (I/O): discover repos, scan/classify worktrees, run a cleanup plan, push,
//! size — spec 04 §1. Owner: core-worktrees-files. Rules (classify, cleanable, plan…) are pure in
//! `ccui_shared::worktree_rules`. INVARIANT: never `--force`, `-f`, `-D`, `--delete-force` (run refuses them).
//!
//! Paths: `norm` = absolute, `.`/`..` collapsed lexically (no symlink resolution), trailing separator trimmed,
//! `\` → `/`; every comparison case-insensitive. Caches: live heads 2 min, merged PRs 10 min, behind forever,
//! lsof 5 s; global git semaphore (`git::run`). `repos.json` in app data (sorted, case-insensitive set).

use crate::git::{self, GitOut};
use crate::session_manager::SessionManager;
use crate::{CoreResult, UiSink};
use ccui_shared::api::{WorktreeCache, WorktreeRunStep};
use ccui_shared::fmt::enc;
use ccui_shared::model::{SessionStatus, Time};
use ccui_shared::push::Push;
use ccui_shared::t;
use ccui_shared::worktree_rules::{self as rules, CleanupStep, WorktreeInfo, WtFacts, WtState};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

/// What a scan needs to know of a session.
#[derive(Clone, Debug, Default)]
struct LiveInfo {
    cwd: String,
    name: String,
    has_process: bool,
    waiting: bool,
}

pub struct WorktreeService {
    /// None only in tests (SessionManager is another unit's).
    sessions: Option<Arc<SessionManager>>,
    sink: Arc<dyn UiSink>,
    repos_file: PathBuf,
    /// repo (lowercased) → Safe + Orphan count of its last scan.
    cleanable: Mutex<HashMap<String, i32>>,
    last_scan: Mutex<HashMap<String, Vec<WorktreeInfo>>>,
    cached_at: Mutex<Option<ccui_shared::Time>>,
    /// Transcript-discovered cwds (until refresh).
    repos: Mutex<Option<Vec<String>>>,
    /// repos.json (lazy-loaded), a case-insensitive set.
    remembered: Mutex<Option<Vec<String>>>,
    /// Cancel flags of running cleanup runs by run id.
    runs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    #[cfg(test)]
    test_live: Mutex<Vec<LiveInfo>>,
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn contains_ci(v: &[String], s: &str) -> bool {
    v.iter().any(|x| eq_ci(x, s))
}

fn sort_ci(v: &mut [String]) {
    v.sort_by_cached_key(|s| s.to_uppercase());
}

impl WorktreeService {
    pub fn new(sessions: Arc<SessionManager>, sink: Arc<dyn UiSink>) -> Arc<WorktreeService> {
        Arc::new(Self::build(Some(sessions), sink, crate::paths::app_data().join("repos.json")))
    }

    fn build(sessions: Option<Arc<SessionManager>>, sink: Arc<dyn UiSink>, repos_file: PathBuf) -> WorktreeService {
        WorktreeService {
            sessions,
            sink,
            repos_file,
            cleanable: Mutex::default(),
            last_scan: Mutex::default(),
            cached_at: Mutex::default(),
            repos: Mutex::default(),
            remembered: Mutex::default(),
            runs: Mutex::default(),
            #[cfg(test)]
            test_live: Mutex::default(),
        }
    }

    /// Every session of the app (all of them, not only those with a process).
    fn live(&self) -> Vec<LiveInfo> {
        #[cfg(test)]
        if self.sessions.is_none() {
            return self.test_live.lock().clone();
        }
        let Some(sm) = &self.sessions else { return Vec::new() };
        sm.all()
            .iter()
            .map(|s| {
                let has_process = s.has_process();
                s.with_state(|st| LiveInfo {
                    cwd: st.cwd.clone(),
                    name: st.name.clone(),
                    has_process,
                    waiting: st.status == SessionStatus::Waiting,
                })
            })
            .collect()
    }

    /// Roots of live sessions ∪ remembered ∪ recent transcripts (refresh re-reads transcripts), sorted.
    pub async fn discover_repos(&self, refresh: bool) -> CoreResult<Vec<String>> {
        let mut live: Vec<String> = Vec::new();
        for s in self.live() {
            let n = norm(&strip_worktree(&s.cwd));
            if !contains_ci(&live, &n) {
                live.push(n);
            }
        }
        let mem = self.remembered.lock().get_or_insert_with(|| load_remembered(&self.repos_file)).clone();
        let cached = self.repos.lock().clone();
        let repos = match cached {
            Some(r) if !refresh => r,
            _ => {
                let r = tokio::task::spawn_blocking(from_transcripts).await.unwrap_or_default();
                *self.repos.lock() = Some(r.clone());
                r
            }
        };
        let mut cands: Vec<String> = Vec::new();
        for c in live.iter().chain(&mem).chain(&repos) {
            if !contains_ci(&cands, c) {
                cands.push(c.clone());
            }
        }
        let resolved = futures::future::join_all(cands.iter().map(|c| root_of(c))).await;
        let (mut roots, mut live_roots, mut dead) = (Vec::new(), Vec::new(), Vec::new());
        for (cwd, r) in cands.iter().zip(resolved) {
            match r? {
                None => dead.push(norm(cwd)),
                Some(r) => {
                    if contains_ci(&live, &norm(cwd)) && !contains_ci(&live_roots, &r) {
                        live_roots.push(r.clone());
                    }
                    if !contains_ci(&roots, &r) {
                        roots.push(r);
                    }
                }
            }
        }
        {
            let mut guard = self.remembered.lock();
            let remembered = guard.get_or_insert_with(Vec::new);
            let before = remembered.len();
            let mut changed = false;
            for r in live_roots.iter().map(|r| norm(r)) {
                if !contains_ci(remembered, &r) {
                    remembered.push(r);
                }
            }
            changed |= remembered.len() != before;
            for d in &dead {
                // a folder that still exists may just be a transient git failure
                if !contains_ci(&live, d) && !Path::new(d).is_dir() {
                    let n = remembered.len();
                    remembered.retain(|x| !eq_ci(x, d));
                    changed |= remembered.len() != n;
                }
            }
            if changed {
                save_remembered(&self.repos_file, remembered);
            }
        }
        sort_ci(&mut roots);
        Ok(roots)
    }

    /// Rows of one repo (worktrees except the main one, then orphan `worktree-*` branches). With `only`, just that
    /// path's row and no cache/count update. A per-repo git failure → Err(stderr); git missing → Err(Wt.GitMissing).
    pub async fn scan(&self, repo: &str, only: Option<&str>) -> CoreResult<Vec<WorktreeInfo>> {
        let list = g(repo, &["worktree", "list", "--porcelain", "-z"]).await?;
        if list.code != 0 {
            return Err(list.stderr.trim().to_string());
        }
        let records: Vec<HashMap<String, String>> =
            list.stdout.split("\0\0").filter(|r| !r.is_empty()).map(parse_record).filter(|r| r.contains_key("worktree")).collect();
        if records.is_empty() {
            return Ok(Vec::new());
        }

        let bas = base_branch(repo, &records[0]).await?;
        let base_tree = g(repo, &["rev-parse", &format!("{bas}^{{tree}}")]).await?.stdout.trim().to_string();
        let has_remote_base = g(repo, &["rev-parse", "--verify", "-q", &format!("refs/remotes/origin/{bas}")]).await?.code == 0;
        // Unborn or missing base (repo with no commits): nothing is merged into it, and --merged would exit 128.
        let base_ok = g(repo, &["rev-parse", "--verify", "-q", &format!("{bas}^{{commit}}")]).await?.code == 0;
        // Two --merged filters are a union for git: in the local base or in origin's, the content is kept either way.
        let merged: HashSet<String> = if !base_ok {
            HashSet::new()
        } else {
            let origin = format!("origin/{bas}");
            let mut args = vec!["for-each-ref", "refs/heads", "--merged", &bas];
            if has_remote_base {
                args.extend(["--merged", &origin]);
            }
            args.push("--format=%(refname:short)");
            ok(g(repo, &args).await?)?.split('\n').map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
        };
        let refs: HashMap<String, [String; 4]> = ok(g(
            repo,
            &[
                "for-each-ref",
                "refs/heads",
                "--format=%(refname:short)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(committerdate:iso-strict)",
            ],
        )
        .await?)?
        .split('\n')
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let p: Vec<&str> = l.trim_end_matches('\r').split('\0').collect();
            (p.len() == 4).then(|| (p[0].to_string(), [p[0], p[1], p[2], p[3]].map(String::from)))
        })
        .collect();
        let ctx = ScanCtx {
            repo: repo.to_string(),
            live: self.live().into_iter().filter(|s| s.has_process).collect(),
            cwds: process_cwds().await,
            live_heads: live_heads(repo).await,
            prs: merged_prs(repo, &bas).await,
            bas,
            base_tree,
            base_ok,
            merged,
            refs,
            base_pids: Mutex::default(),
        };

        let picked = records[1..].iter().filter(|r| only.is_none_or(|o| eq_ci(&norm(&r["worktree"]), o)));
        let rows: Vec<WorktreeInfo> =
            futures::future::join_all(picked.map(|r| ctx.row(r))).await.into_iter().collect::<CoreResult<_>>()?;
        // worktree-* branches that no worktree checks out (main record included) have no row above: surface them.
        if only.is_some() {
            return Ok(rows);
        }
        let heads: Vec<String> = ctx.refs.keys().cloned().collect();
        let checked: Vec<String> = records.iter().filter_map(|r| r.get("branch").map(|b| b.replace("refs/heads/", ""))).collect();
        let orphans: Vec<WorktreeInfo> = futures::future::join_all(
            rules::orphan_branches(&heads, &checked).into_iter().map(|b| ctx.orphan_row(b, &rows)),
        )
        .await
        .into_iter()
        .collect::<CoreResult<_>>()?;
        let mut all = rows;
        all.extend(orphans);
        let root = norm(repo);
        self.set_cleanable(&root, all.iter().filter(|w| matches!(w.state, WtState::Safe | WtState::Orphan)).count() as i32);
        self.last_scan.lock().insert(root.to_lowercase(), all.clone());
        *self.cached_at.lock() = Some(Utc::now());
        Ok(all)
    }

    /// Last full scan (Overview / Worktrees reuse it under 2 min).
    pub fn cached(&self) -> WorktreeCache {
        let scans = self.last_scan.lock();
        let mut keys: Vec<&String> = scans.keys().collect();
        keys.sort();
        WorktreeCache { rows: keys.into_iter().flat_map(|k| scans[k].clone()).collect(), cached_at: *self.cached_at.lock() }
    }

    /// Sum of Safe + Orphan of the last scans; None when nothing was scanned.
    pub fn cleanable_count(&self) -> Option<i32> {
        let c = self.cleanable.lock();
        (!c.is_empty()).then(|| c.values().sum())
    }

    /// Emits `Push::WorktreeCount` only when the repo is new or its count changed.
    fn set_cleanable(&self, repo: &str, n: i32) {
        let old = self.cleanable.lock().insert(repo.to_lowercase(), n);
        if old != Some(n) {
            self.sink.emit(Push::WorktreeCount { count: self.cleanable_count() });
        }
    }

    /// Recursive size, links and reparse points skipped, inaccessible entries ignored (blocking: spawn_blocking).
    pub fn size(path: &str) -> i64 {
        let mut sum = 0i64;
        let mut stack = vec![PathBuf::from(path)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let Ok(m) = std::fs::symlink_metadata(e.path()) else { continue };
                if m.file_type().is_symlink() || is_reparse(&m) {
                    continue; // never follow a link
                }
                if m.is_dir() {
                    stack.push(e.path());
                } else {
                    sum += m.len() as i64;
                }
            }
        }
        sum
    }

    /// `git -C <path> push -u origin <branch>` → exit code; detached → -1.
    /// The path and branch must match a row of the last scan (arguments from the webview never reach git alone).
    pub async fn push(&self, path: &str, branch: Option<&str>) -> CoreResult<i32> {
        let Some(branch) = branch else { return Ok(-1) };
        let known = self.cached().rows.into_iter().any(|w| !w.branch_only && eq_ci(&w.path, path) && w.branch.as_deref() == Some(branch));
        if !known || branch.starts_with('-') {
            return Err(t!("Wt.Run.Missing"));
        }
        Ok(g(path, &["push", "-u", "origin", branch]).await?.code)
    }

    /// `git check-ignore -q .claude/worktrees/x` exits 0.
    pub async fn ignores_worktrees(&self, repo: &str) -> bool {
        g(repo, &["check-ignore", "-q", ".claude/worktrees/x"]).await.is_ok_and(|o| o.code == 0)
    }

    /// Clean the chosen worktrees. The plan is REBUILT here with `worktree_rules::plan` from this service's own
    /// cached rows (`last_scan`) for `paths` (unknown paths skipped): command arguments never come from the webview
    /// (a `-c core.fsmonitor=…` would be code execution). `kept_ok` = paths whose kept files were acknowledged.
    /// Then step by step, re-validating each path's row first; emit `Push::WorktreeRunStep` per step and
    /// `Push::WorktreeRunDone`; full rescan of each repo after. Cancellable by `cancel_run(run_id)`.
    pub async fn run(&self, run_id: u64, paths: Vec<String>, kept_ok: Vec<String>) -> CoreResult<()> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.runs.lock().insert(run_id, cancel.clone());
        let rows: Vec<WorktreeInfo> = self.cached().rows.into_iter().filter(|w| contains_ci(&paths, &w.path)).collect();
        let steps = rules::plan(&rows, &kept_ok);
        let res = self.run_steps(run_id, &steps, &cancel).await;
        self.runs.lock().remove(&run_id);
        self.sink.emit(Push::WorktreeRunDone { run_id });
        res
    }

    async fn run_steps(&self, run_id: u64, plan: &[CleanupStep], cancel: &AtomicBool) -> CoreResult<()> {
        let mut skip: Vec<String> = Vec::new(); // paths and branches of skipped rows
        let mut checked: Vec<String> = Vec::new();
        let mut row: Option<WorktreeInfo> = None;
        for (index, step) in plan.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                return Ok(());
            }
            let (exit, output) = self.run_step(step, &mut skip, &mut checked, &mut row).await?;
            self.sink.emit(Push::WorktreeRunStep(WorktreeRunStep {
                run_id,
                index,
                repo: step.repo.clone(),
                display: step.display.clone(),
                exit,
                output,
            }));
        }
        let mut repos: Vec<&str> = Vec::new();
        for s in plan {
            if !repos.contains(&s.repo.as_str()) {
                repos.push(&s.repo);
            }
        }
        for repo in repos {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            self.scan(repo, None).await?; // refresh the cleanable count and the cache
        }
        Ok(())
    }

    async fn run_step(
        &self,
        step: &CleanupStep,
        skip: &mut Vec<String>,
        checked: &mut Vec<String>,
        row: &mut Option<WorktreeInfo>,
    ) -> CoreResult<(i32, String)> {
        let a = &step.git_args;
        if rules::forbidden(a) {
            return Ok((-1, t!("Wt.Run.Forbidden")));
        }
        let path = match a.as_slice() {
            [w, op, p] if w == "worktree" && (op == "unlock" || op == "remove") => Some(p.clone()),
            _ => None,
        };
        let target = path.clone().or_else(|| match a.as_slice() {
            [b, d, br] if b == "branch" && d == "-d" => Some(br.clone()),
            _ => None,
        });
        if let Some(p) = &path {
            if !contains_ci(checked, p) {
                checked.push(p.clone());
                *row = self.scan(&step.repo, Some(&norm(p))).await?.into_iter().find(|w| eq_ci(&w.path, p));
                if !row.as_ref().is_some_and(|r| rules::cleanable(r, step.kept_ok)) {
                    skip.push(p.clone());
                    if let Some(b) = row.as_ref().and_then(|r| r.branch.clone()) {
                        skip.push(b);
                    }
                    return Ok((
                        -1,
                        match row {
                            None => t!("Wt.Run.Missing"),
                            Some(r) => t!("Wt.Run.Changed", rules::label(r.state)),
                        },
                    ));
                }
            }
        }
        if target.as_ref().is_some_and(|t| contains_ci(skip, t)) {
            return Ok((-1, t!("Wt.Run.Skipped")));
        }
        let args: Vec<&str> = a.iter().map(String::as_str).collect();
        let o = g(&step.repo, &args).await?;
        let mut output = format!("{}{}", o.stdout, o.stderr).trim().to_string();
        if o.code != 0 && a.len() >= 2 && a[0] == "branch" && a[1] == "-d" {
            output = format!("{}\n{output}", t!("Wt.Run.BranchKept"));
        }
        if o.code != 0 && step.failure_is_fatal {
            if let Some(p) = &path {
                skip.push(p.clone());
                if let Some(b) = row.as_ref().and_then(|r| r.branch.clone()) {
                    skip.push(b);
                }
            }
        }
        Ok((o.code, output))
    }

    pub fn cancel_run(&self, run_id: u64) {
        if let Some(f) = self.runs.lock().get(&run_id) {
            f.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(windows)]
fn is_reparse(m: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
}

#[cfg(not(windows))]
fn is_reparse(_: &std::fs::Metadata) -> bool {
    false
}

async fn g(dir: &str, args: &[&str]) -> CoreResult<GitOut> {
    git::run(dir, args, None, None).await
}

/// `Ok(r)`: stdout on exit 0, else Err(stderr).
fn ok(r: GitOut) -> CoreResult<String> {
    if r.code == 0 {
        Ok(r.stdout)
    } else {
        Err(r.stderr.trim().to_string())
    }
}

/// One `worktree list --porcelain -z` record: fields split on the first space; the first of a repeated key wins.
fn parse_record(r: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for l in r.split('\0') {
        let (k, v) = l.split_once(' ').unwrap_or((l, ""));
        m.entry(k.to_string()).or_insert_with(|| v.to_string());
    }
    m
}

async fn base_branch(repo: &str, main: &HashMap<String, String>) -> CoreResult<String> {
    let o = g(repo, &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"]).await?.stdout.trim().to_string();
    if !o.is_empty() {
        return Ok(o.strip_prefix("origin/").unwrap_or(&o).to_string());
    }
    for b in ["main", "master"] {
        if g(repo, &["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")]).await?.code == 0 {
            return Ok(b.into());
        }
    }
    Ok(main.get("branch").map_or("HEAD".into(), |b| b.replace("refs/heads/", "")))
}

#[derive(Clone, Debug)]
struct MergedPr {
    number: i32,
    branch: String,
    head: String,
}

/// Everything a row needs, computed once per scan.
struct ScanCtx {
    repo: String,
    bas: String,
    base_tree: String,
    base_ok: bool,
    merged: HashSet<String>,
    /// name → [name, upstream, track, committerdate].
    refs: HashMap<String, [String; 4]>,
    /// Sessions with a live process.
    live: Vec<LiveInfo>,
    cwds: Vec<(u32, String, String)>,
    live_heads: Option<HashSet<String>>,
    /// merge-base → patch-ids of the base since it (per-scan memo).
    base_pids: Mutex<HashMap<String, HashSet<String>>>,
    prs: Vec<MergedPr>,
}

static AHEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"ahead (\d+)").unwrap());

impl ScanCtx {
    async fn g(&self, args: &[&str]) -> CoreResult<GitOut> {
        g(&self.repo, args).await
    }

    /// Commits on the branch that no remote has, or the git error (unknown is not 0).
    async fn ahead(&self, branch: &str, rf: Option<&[String; 4]>) -> CoreResult<(i32, Option<String>)> {
        if let Some(c) = rf.and_then(|r| AHEAD.captures(&r[2])) {
            return Ok((c[1].parse().unwrap_or(0), None));
        }
        if rf.is_some_and(|r| !r[1].is_empty() && r[2] != "gone") {
            return Ok((0, None)); // upstream gone: count against every remote instead
        }
        let o = self.g(&["rev-list", "--count", branch, "--not", "--remotes"]).await?;
        Ok(match (o.code, o.stdout.trim().parse::<i32>()) {
            (0, Ok(n)) => (n, None),
            _ => (0, Some(Some(o.stderr.trim().to_string()).filter(|m| !m.is_empty()).unwrap_or(format!("git rev-list: exit {}", o.code)))),
        })
    }

    async fn remote_refs(&self, rev: &str) -> CoreResult<Vec<String>> {
        let o = self.g(&["for-each-ref", "--contains", rev, "--format=%(refname:short)", "refs/remotes"]).await?;
        Ok(if o.code != 0 {
            Vec::new()
        } else {
            o.stdout.split('\n').map(str::trim).filter(|r| !r.is_empty() && !r.ends_with("/HEAD")).map(String::from).collect()
        })
    }

    /// A remote branch holding `rev`, whether origin confirmed it exists, and the ref of a branch origin deleted.
    async fn remote_holding(&self, rev: &str) -> CoreResult<(Option<String>, bool, Option<String>)> {
        let refs = self.remote_refs(rev).await?;
        let Some(live) = &self.live_heads else { return Ok((refs.first().cloned(), false, None)) }; // origin unreachable
        Ok(match refs.iter().find(|r| live.contains(*r)) {
            Some(r) => (Some(r.clone()), true, None),
            None => (None, false, refs.first().cloned()),
        })
    }

    async fn patch_id(&self, mb: &str, rev: &str) -> CoreResult<Option<String>> {
        let d = self.g(&["diff", mb, rev]).await?;
        if d.code != 0 || d.stdout.is_empty() {
            return Ok(None);
        }
        let o = git::run(&self.repo, &["patch-id", "--stable"], Some(d.stdout), None).await?;
        let id = o.stdout.split(' ').next().unwrap_or("").trim().to_string();
        Ok((o.code == 0 && !id.is_empty()).then_some(id))
    }

    /// ponytail: the last 2000 commits of the base since the merge-base; an older squash reads as not merged.
    async fn base_patch_ids(&self, mb: &str) -> CoreResult<HashSet<String>> {
        let log = self.g(&["log", "-p", "--no-merges", "-n", "2000", &format!("{mb}..{}", self.bas)]).await?;
        if log.code != 0 || log.stdout.is_empty() {
            return Ok(HashSet::new());
        }
        let o = git::run(&self.repo, &["patch-id", "--stable"], Some(log.stdout), None).await?;
        Ok(if o.code != 0 {
            HashSet::new()
        } else {
            o.stdout.split('\n').filter(|l| !l.is_empty()).map(|l| l.split(' ').next().unwrap_or("").to_string()).collect()
        })
    }

    /// The branch's whole diff (or its upstream's) has the patch-id of a commit of the base.
    async fn squashed(&self, rev: &str, upstream: Option<&str>) -> CoreResult<bool> {
        let mut rev = rev.to_string();
        if let Some(u) = upstream {
            if self.g(&["merge-base", "--is-ancestor", &rev, u]).await?.code == 0 {
                rev = u.to_string();
            }
        }
        let mb = self.g(&["merge-base", &self.bas, &rev]).await?.stdout.trim().to_string();
        if mb.is_empty() {
            return Ok(false);
        }
        let Some(pid) = self.patch_id(&mb, &rev).await? else { return Ok(false) };
        let known = self.base_pids.lock().get(&mb).map(|s| s.contains(&pid));
        if let Some(k) = known {
            return Ok(k);
        }
        let set = self.base_patch_ids(&mb).await?;
        let hit = set.contains(&pid);
        self.base_pids.lock().insert(mb, set);
        Ok(hit)
    }

    /// merge-tree adds nothing to the base, or the patch-id rule.
    async fn branch_squashed(&self, branch: &str, rf: Option<&[String; 4]>) -> CoreResult<bool> {
        let mt = self.g(&["merge-tree", "--write-tree", &self.bas, branch]).await?;
        if mt.code == 0 && mt.stdout.split('\n').next().unwrap_or("").trim() == self.base_tree {
            return Ok(true);
        }
        let upstream = rf.map(|r| r[1].as_str()).filter(|u| !u.is_empty());
        Ok(self.base_ok && self.squashed(branch, upstream).await?)
    }

    /// GitHub's compare: tip is an ancestor of head when head is 0 commits behind it (cached for good).
    async fn behind0(&self, tip: &str, head: &str) -> bool {
        let key = format!("{tip}...{head}");
        if let Some(b) = BEHIND_CACHE.lock().get(&key) {
            return *b;
        }
        let Some(o) = git::gh(&self.repo, &["api", &format!("repos/{{owner}}/{{repo}}/compare/{tip}...{head}"), "--jq", ".behind_by"]).await
        else {
            return false; // unknown is not merged: no cache, ask again next scan
        };
        let b = o.trim() == "0";
        BEHIND_CACHE.lock().insert(key, b);
        b
    }

    /// A PR merged into the base whose head is this tip, or whose head this tip leads to.
    async fn pr_of(&self, branch: Option<&str>, tip: &str) -> CoreResult<Option<i32>> {
        if let Some(p) = self.prs.iter().find(|p| p.head == tip) {
            if p.number > 0 {
                return Ok(Some(p.number));
            }
        }
        for pr in self.prs.iter().filter(|p| Some(p.branch.as_str()) == branch) {
            if self.g(&["merge-base", "--is-ancestor", tip, &pr.head]).await?.code == 0 || self.behind0(tip, &pr.head).await {
                return Ok(Some(pr.number));
            }
        }
        Ok(None)
    }

    async fn row(&self, r: &HashMap<String, String>) -> CoreResult<WorktreeInfo> {
        let path = norm(&r["worktree"]);
        let branch = r.get("branch").map(|b| b.replace("refs/heads/", ""));
        let exists = Path::new(&path).is_dir();
        let (mut unreadable, mut dirty, mut dirty_names) = (None, 0, None);
        if exists {
            let st = g(&path, &["status", "--porcelain=v1", "-uall"]).await?;
            if st.code == 0 {
                let changed: Vec<&str> = st.stdout.split('\n').filter(|l| !l.is_empty()).collect();
                dirty = changed.len() as i32;
                let names: Vec<String> = changed
                    .iter()
                    .map(|l| match l.char_indices().nth(3) {
                        Some((i, _)) => l[i..].trim().trim_matches('"').to_string(),
                        None => l.to_string(),
                    })
                    .collect();
                dirty_names = rules::names_of(&names);
            } else {
                let m = st.stderr.trim();
                unreadable = Some(if m.is_empty() { format!("git status: exit {}", st.code) } else { m.to_string() });
            }
        }
        let rf = branch.as_ref().and_then(|b| self.refs.get(b));
        let track = rf.map_or("", |r| r[2].as_str());
        let (ahead, ahead_err) = match &branch {
            None => (0, None),
            Some(b) => self.ahead(b, rf).await?,
        };
        if unreadable.is_none() {
            unreadable = ahead_err;
        }
        let head_sha = r.get("HEAD").cloned();
        let is_merged = match (&branch, &head_sha) {
            (Some(b), _) => self.merged.contains(b),
            (None, Some(h)) => self.g(&["merge-base", "--is-ancestor", h, &self.bas]).await?.code == 0,
            (None, None) => false,
        };
        let mut squash = false;
        if !is_merged {
            if let Some(b) = &branch {
                squash = self.branch_squashed(b, rf).await?;
            } else if let Some(dh) = head_sha.as_deref().filter(|h| self.base_ok && !h.is_empty()) {
                // the PR branch may hold more than the agent saw
                let any = self.remote_refs(dh).await?.into_iter().next();
                squash = self.squashed(dh, any.as_deref()).await?;
            }
        }
        let reason = r.get("locked").cloned();
        let pid = rules::pid_of(reason.as_deref());
        let session = self.live.iter().find(|s| inside(&s.cwd, &path));
        let head = branch.clone().or_else(|| head_sha.clone());
        let (on_remote, confirmed, gone) = match &head {
            Some(h) if !is_merged && !squash && ahead == 0 && unreadable.is_none() => self.remote_holding(h).await?,
            _ => (None, false, None),
        };
        let user = self.cwds.iter().find(|c| inside(&c.2, &path));
        let kept = if exists && unreadable.is_none() { kept_ignored(&path).await? } else { None };
        let pr = match head_sha.as_deref() {
            Some(tip) if !is_merged && !squash && !confirmed && ahead == 0 && unreadable.is_none() && !tip.is_empty() && !self.prs.is_empty() => {
                self.pr_of(branch.as_deref(), tip).await? // a sha: GitHub may no longer know the branch name
            }
            _ => None,
        };
        let touched = git_dir_mtime(&path, "index");
        let moved = git_dir_mtime(&path, "logs/HEAD"); // created, committed, checked out: a status elsewhere does not restart it
        let now = Utc::now();
        let recent = moved.filter(|m| now - *m < chrono::Duration::hours(1)).map(|m| (now - m).num_minutes() as i32);
        let facts = WtFacts {
            dirty,
            ahead,
            merged: is_merged,
            squash_merged: squash,
            upstream_gone: track == "gone",
            locked: reason.is_some(),
            lock_pid: pid,
            pid_alive: pid.is_some_and(alive),
            exists,
            prunable: r.contains_key("prunable"),
            active_session_name: session.map(|s| s.name.clone()),
            detached: branch.is_none(),
            unreadable: unreadable.clone(),
            on_remote: if confirmed { on_remote.clone() } else { None },
            in_use_by: user.map(|u| format!("{} (pid {})", u.1, u.0)),
            recent_min: recent,
            kept: kept.clone(),
            on_remote_stale: if confirmed { None } else { on_remote.clone() },
            remote_gone: gone,
            merged_pr: pr,
            dirty_names,
            ..WtFacts::default()
        };
        let (state, mut why) = rules::classify(&facts);
        if let Some(s) = session.filter(|s| s.waiting) {
            why = t!("Wt.Why.Waiting", enc(&s.name));
        }
        let last = [touched, rf.and_then(|r| parse_date(&r[3]))].into_iter().flatten().max();
        if let Some(l) = last {
            let idle = (now - l).num_days();
            if matches!(state, WtState::Safe | WtState::Check) && idle >= 2 {
                why += &t!("Wt.Why.Idle", idle);
            }
        }
        Ok(WorktreeInfo {
            repo: norm(&self.repo),
            name: Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            path,
            branch,
            head: head_sha,
            state,
            why: why.replace("{base}", &enc(&self.bas)),
            size_bytes: None,
            last_activity: last,
            locked: reason.is_some(),
            lock_reason: reason,
            dirty,
            unpushed: ahead,
            merged: is_merged,
            squash_merged: squash || pr.is_some(),
            upstream_gone: track == "gone",
            branch_only: false,
            unreadable,
            pushed: if confirmed { on_remote } else { None },
            recent_min: recent,
            kept,
        })
    }

    async fn orphan_row(&self, branch: String, rows: &[WorktreeInfo]) -> CoreResult<WorktreeInfo> {
        let rf = self.refs.get(&branch);
        let gone_up = rf.is_some_and(|r| r[2] == "gone");
        let (ahead, ahead_err) = self.ahead(&branch, rf).await?;
        let is_merged = self.merged.contains(&branch);
        let squash = !is_merged && self.branch_squashed(&branch, rf).await?;
        // claude -w names the branch worktree-<folder>: when that folder is still listed (it went detached), say whose.
        let twin = rows.iter().find(|w| format!("worktree-{}", w.name) == branch);
        let clear = !is_merged && !squash && ahead == 0 && ahead_err.is_none();
        let (on_remote, confirmed, gone) = if clear { self.remote_holding(&branch).await? } else { (None, false, None) };
        let pr = if clear && !confirmed && !self.prs.is_empty() {
            let tip = self.g(&["rev-parse", &branch]).await?.stdout.trim().to_string();
            self.pr_of(Some(&branch), &tip).await?
        } else {
            None
        };
        let (state, why) = rules::classify(&WtFacts {
            ahead,
            merged: is_merged,
            squash_merged: squash,
            upstream_gone: gone_up,
            no_worktree: true,
            unreadable: ahead_err,
            on_remote: if confirmed { on_remote.clone() } else { None },
            twin_of: twin.map(|t| t.name.clone()),
            twin_active: twin.is_some_and(|t| t.state == WtState::Active),
            on_remote_stale: if confirmed { None } else { on_remote.clone() },
            remote_gone: gone,
            merged_pr: pr,
            ..WtFacts::default()
        });
        let root = norm(&self.repo);
        Ok(WorktreeInfo {
            path: format!("{root}/refs/heads/{branch}"),
            repo: root,
            name: branch.clone(),
            branch: Some(branch),
            head: None,
            state,
            why: why.replace("{base}", &enc(&self.bas)),
            size_bytes: None,
            last_activity: rf.and_then(|r| parse_date(&r[3])),
            locked: false,
            lock_reason: None,
            dirty: 0,
            unpushed: ahead,
            merged: is_merged,
            squash_merged: squash,
            upstream_gone: gone_up,
            branch_only: true,
            unreadable: None,
            pushed: if confirmed { on_remote } else { None },
            recent_min: None,
            kept: None,
        })
    }
}

fn parse_date(s: &str) -> Option<Time> {
    DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.with_timezone(&Utc))
}

/// mtime of `<gitdir>/<rel>`, gitdir read from the worktree's `.git` file; None when unknown.
fn git_dir_mtime(path: &str, rel: &str) -> Option<Time> {
    let dotgit = Path::new(path).join(".git");
    if !dotgit.is_file() {
        return None;
    }
    let gitdir = std::fs::read_to_string(&dotgit).ok()?.replace("gitdir:", "").trim().to_string();
    let full = Path::new(path).join(gitdir).join(rel); // an absolute gitdir replaces the base
    let m = std::fs::metadata(full).ok()?.modified().ok()?;
    Some(DateTime::<Utc>::from(m))
}

/// Ignored paths that are not known build output (a .env, an agent log, a dump), summarized.
async fn kept_ignored(path: &str) -> CoreResult<Option<String>> {
    let o = g(path, &["ls-files", "--others", "--ignored", "--exclude-standard", "--directory"]).await?;
    Ok(if o.code != 0 { None } else { rules::kept_summary(&rules::kept_of(&o.stdout)) })
}

type HeadsEntry = (Instant, Option<HashSet<String>>);
static LIVE_CACHE: LazyLock<Mutex<HashMap<String, HeadsEntry>>> = LazyLock::new(Mutex::default);
static PR_CACHE: LazyLock<Mutex<HashMap<String, (Instant, Vec<MergedPr>)>>> = LazyLock::new(Mutex::default);
static BEHIND_CACHE: LazyLock<Mutex<HashMap<String, bool>>> = LazyLock::new(Mutex::default);
type ProcList = Vec<(u32, String, String)>;
static PROC_CACHE: LazyLock<Mutex<Option<(Instant, ProcList)>>> = LazyLock::new(Mutex::default);

/// The branches origin has right now (`origin/<name>`), or None when it cannot be asked. Cached 2 min (None too).
async fn live_heads(repo: &str) -> Option<HashSet<String>> {
    let key = repo.to_lowercase();
    if let Some((at, h)) = LIVE_CACHE.lock().get(&key) {
        if at.elapsed() < Duration::from_secs(120) {
            return h.clone();
        }
    }
    // too slow means unknown, not empty
    let heads = match git::run(repo, &["ls-remote", "--heads", "origin"], None, Some(Duration::from_secs(10))).await {
        Ok(o) if o.code == 0 => Some(
            o.stdout
                .split('\n')
                .filter_map(|l| {
                    let p: Vec<&str> = l.split('\t').collect();
                    (p.len() == 2).then(|| p[1].trim()).and_then(|r| r.strip_prefix("refs/heads/")).map(|b| format!("origin/{b}"))
                })
                .collect(),
        ),
        _ => None,
    };
    LIVE_CACHE.lock().insert(key, (Instant::now(), heads.clone()));
    heads
}

/// PRs merged into `bas` (gh, optional): none when gh is missing, not logged in, or not on GitHub. Cached 10 min.
async fn merged_prs(repo: &str, bas: &str) -> Vec<MergedPr> {
    let key = format!("{repo}|{bas}").to_lowercase();
    if let Some((at, prs)) = PR_CACHE.lock().get(&key) {
        if at.elapsed() < Duration::from_secs(600) {
            return prs.clone();
        }
    }
    let mut list = Vec::new();
    let args = ["pr", "list", "--state", "merged", "--base", bas, "--limit", "300", "--json", "number,headRefName,headRefOid"];
    if let Some(json) = git::gh(repo, &args).await {
        if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(&json) {
            for e in &items {
                let Some(number) = e.get("number").and_then(|n| n.as_i64()) else { break };
                list.push(MergedPr {
                    number: number as i32,
                    branch: ccui_shared::json::string(e, "headRefName"),
                    head: ccui_shared::json::string(e, "headRefOid"),
                });
            }
        }
    }
    PR_CACHE.lock().insert(key, (Instant::now(), list.clone()));
    list
}

/// Working directory of every other process (lsof), cached 5 s. No lsof (Windows): nothing is known.
async fn process_cwds() -> ProcList {
    if let Some((at, l)) = PROC_CACHE.lock().as_ref() {
        if at.elapsed() < Duration::from_secs(5) {
            return l.clone();
        }
    }
    let list = tokio::task::spawn_blocking(|| {
        std::process::Command::new("lsof")
            .args(["-n", "-P", "-w", "-d", "cwd", "-Fpcn"])
            .stderr(std::process::Stdio::null())
            .output()
            .map(|o| rules::parse_lsof(&String::from_utf8_lossy(&o.stdout), std::process::id()))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    *PROC_CACHE.lock() = Some((Instant::now(), list.clone()));
    list
}

/// `RootOf(cwd)`: the main repo root of a folder (a linked worktree resolves to its main repo); None when the folder
/// is gone, not in a repo, or bare.
async fn root_of(cwd: &str) -> CoreResult<Option<String>> {
    if !Path::new(cwd).is_dir() {
        return Ok(None);
    }
    let o = g(cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).await?;
    Ok(if o.code != 0 { None } else { git::common_dir_root(&o.stdout).map(|r| norm(&r)) })
}

fn load_remembered(file: &Path) -> Vec<String> {
    let list: Vec<String> = std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for p in list.iter().map(|p| norm(p)) {
        if !contains_ci(&out, &p) {
            out.push(p);
        }
    }
    out
}

/// A cache: a failed save only costs rediscovery.
fn save_remembered(file: &Path, set: &[String]) {
    let mut v = set.to_vec();
    sort_ci(&mut v);
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&v) {
        let _ = std::fs::write(file, json);
    }
}

static CWD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""cwd":("(?:[^"\\]|\\.)*")"#).unwrap());

/// The cwd recorded in the first 64 KiB of a transcript.
fn cwd_of(jsonl: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = vec![0u8; 64 * 1024];
    let n = std::fs::File::open(jsonl).ok()?.read(&mut buf).ok()?;
    let head = String::from_utf8_lossy(&buf[..n]);
    serde_json::from_str(CWD.captures(&head)?.get(1)?.as_str()).ok()
}

/// cwds of the newest transcript folders of the last 30 days (not under the temp dir), worktree suffix stripped.
fn from_transcripts() -> Vec<String> {
    let dir = crate::paths::projects_root();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let tmp = std::env::temp_dir().to_string_lossy().trim_end_matches(['/', '\\']).to_string();
    let temp = ccui_shared::text::slug(&tmp).to_lowercase();
    let since = std::time::SystemTime::now() - Duration::from_secs(30 * 86_400);
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut folders: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().to_lowercase().starts_with(&temp))
        .filter_map(|e| mtime(&e.path()).filter(|m| *m > since).map(|m| (m, e.path())))
        .collect();
    folders.sort_by(|a, b| b.0.cmp(&a.0));
    folders.truncate(200); // ponytail: cap, raise if real repos get missed
    let mut seen: Vec<String> = Vec::new();
    for (_, d) in folders {
        let newest = std::fs::read_dir(&d)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .filter_map(|p| mtime(&p).map(|m| (m, p)))
            .max_by_key(|(m, _)| *m);
        let Some(cwd) = newest.and_then(|(_, f)| cwd_of(&f)) else { continue };
        let cwd = strip_worktree(&cwd);
        if Path::new(&cwd).is_dir() && !contains_ci(&seen, &cwd) {
            seen.push(cwd);
        }
    }
    seen
}

/// Lexical normalization (see module doc).
pub fn norm(path: &str) -> String {
    let p = Path::new(path);
    let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().replace('\\', "/")
}

/// `norm(cwd) == path` or starts with `path + "/"` (case-insensitive).
pub fn inside(cwd: &str, path: &str) -> bool {
    let c = norm(cwd).to_lowercase();
    let p = path.to_lowercase();
    c == p || c.starts_with(&format!("{p}/"))
}

static WORKTREE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\\/]\.claude[\\/]worktrees[\\/][^\\/]+$").unwrap());

/// Remove `[\\/]\.claude[\\/]worktrees[\\/][^\\/]+$` (a `claude -w` cwd maps to its repo).
pub fn strip_worktree(cwd: &str) -> String {
    WORKTREE_SUFFIX.replace(cwd, "").into_owned()
}

/// Process exists and has not exited (Unix `kill(pid, 0) == 0 || EPERM`).
pub fn alive(pid: i32) -> bool {
    if pid <= 0 {
        return false; // 0 and negatives address process groups
    }
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks that the process exists.
        let r = unsafe { libc::kill(pid, 0) };
        r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        // SAFETY: the handle is checked and closed.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
            if h.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code) != 0;
            CloseHandle(h);
            ok && code == STILL_ACTIVE as u32
        }
    }
    #[cfg(not(any(unix, windows)))]
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::g as G;

    #[derive(Default)]
    struct Rec(Mutex<Vec<Push>>);
    impl UiSink for Rec {
        fn emit(&self, push: Push) {
            self.0.lock().push(push);
        }
        fn notify(&self, _: &str, _: &str, _: &str) {}
        fn window_focused(&self) -> bool {
            true
        }
    }

    fn svc(file: &Path) -> (WorktreeService, Arc<Rec>) {
        let rec = Arc::new(Rec::default());
        let s = WorktreeService::build(None, rec.clone(), file.to_path_buf());
        *s.repos.lock() = Some(Vec::new()); // no transcript discovery in tests
        (s, rec)
    }

    #[test]
    fn paths() {
        assert_eq!(norm("/a/b/../c/./d/"), "/a/c/d");
        assert_eq!(norm("/"), "/");
        assert_eq!(norm("/.."), "/");
        assert!(inside("/R/x/Y/z", "/r/x/y") && inside("/r/x/y/", "/r/x/y") && !inside("/r/x/yz", "/r/x/y"));
        assert_eq!(strip_worktree("/r/repo/.claude/worktrees/agent-a"), "/r/repo");
        assert_eq!(strip_worktree("C:\\r\\.claude\\worktrees\\a"), "C:\\r");
        assert_eq!(strip_worktree("/r/repo"), "/r/repo");
        assert!(alive(std::process::id() as i32) && !alive(0) && !alive(-1));
        let p = parse_record("worktree /a b\0HEAD x\0locked\0branch refs/heads/m\0branch other");
        assert_eq!((p["worktree"].as_str(), p["locked"].as_str(), p["branch"].as_str()), ("/a b", "", "refs/heads/m"));
    }

    #[test]
    fn cleanable_count_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let (s, rec) = svc(&tmp.path().join("repos.json"));
        assert_eq!(s.cleanable_count(), None);
        s.set_cleanable("C:/r", 2);
        assert_eq!((rec.0.lock().len(), s.cleanable_count()), (1, Some(2)));
        s.set_cleanable("c:/R", 2);
        assert_eq!(rec.0.lock().len(), 1);
        s.set_cleanable("C:/r", 0);
        assert_eq!((rec.0.lock().len(), s.cleanable_count()), (2, Some(0)));
        assert_eq!(rec.0.lock()[1], Push::WorktreeCount { count: Some(0) });
    }

    #[test]
    fn size_skips_links() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("a/b/f"), [0u8; 10]).unwrap();
        std::fs::write(d.join("g"), [0u8; 5]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(d.join("a"), d.join("link")).unwrap();
        assert_eq!(WorktreeService::size(&d.to_string_lossy()), 15);
        assert_eq!(WorktreeService::size(&d.join("nope").to_string_lossy()), 0);
    }

    #[tokio::test]
    async fn scan_discover_run() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        G(&repo, &["init", "-q"]);
        G(&repo, &["commit", "-q", "--allow-empty", "-m", "i"]);
        G(&repo, &["worktree", "add", "-q", ".claude/worktrees/demo", "-b", "demo"]);
        G(&repo, &["branch", "worktree-foo"]);
        G(&repo, &["branch", "feat-x"]);
        G(&repo, &["switch", "-q", "-c", "worktree-bar"]);
        G(&repo, &["commit", "-q", "--allow-empty", "-m", "b"]);
        G(&repo, &["switch", "-q", "-"]);
        let file = tmp.path().join("repos.json");
        let rs = repo.to_string_lossy().to_string();

        let (s, _) = svc(&file);
        let all = s.scan(&rs, None).await.unwrap();
        let bo: Vec<&WorktreeInfo> = all.iter().filter(|w| w.branch_only).collect();
        assert_eq!(bo.len(), 2, "{all:#?}");
        assert!(bo.iter().any(|w| w.branch.as_deref() == Some("worktree-foo") && w.state == WtState::Orphan));
        assert!(bo.iter().any(|w| w.branch.as_deref() == Some("worktree-bar") && w.state == WtState::Check));
        assert!(bo.iter().all(|w| Some(w.name.as_str()) == w.branch.as_deref()));
        assert!(all.iter().all(|w| !w.why.contains("{base}")));
        let demo = all.iter().find(|w| !w.branch_only).unwrap().clone();
        assert_eq!((demo.name.as_str(), demo.branch.as_deref(), demo.merged), ("demo", Some("demo"), true));
        // Created moments ago: held back by recency.
        assert_eq!((demo.state, demo.recent_min.is_some()), (WtState::Check, true));
        assert_eq!(s.cleanable_count(), Some(1));
        assert_eq!(s.cached().rows.len(), 3);
        let one = s.scan(&rs, Some(&demo.path)).await.unwrap();
        assert!(one.len() == 1 && one[0].path == demo.path);

        // The run rebuilds its plan from the cache: worktree-foo (Orphan) gets its branch -d; demo (Check) is not touched.
        s.run(7, vec![demo.path.clone(), bo.iter().find(|w| w.state == WtState::Orphan).unwrap().path.clone()], vec![])
            .await
            .unwrap();
        let branches = std::process::Command::new("git").arg("-C").arg(&repo).args(["branch", "--list", "worktree-foo"]).output().unwrap();
        assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
        assert!(Path::new(&demo.path).is_dir());
        // A push refuses a path/branch the scan never produced.
        assert!(s.push(&demo.path, Some("--force")).await.is_err());

        // Discover: the repo of a -w session whose worktree was deleted, then remembered after restart.
        G(&repo, &["worktree", "remove", ".claude/worktrees/demo"]);
        let unique = tmp.path().file_name().unwrap().to_string_lossy().to_string(); // macOS temp is a symlink
        let (s1, _) = svc(&file);
        s1.test_live.lock().push(LiveInfo { cwd: demo.path.clone(), ..LiveInfo::default() });
        assert!(s1.discover_repos(false).await.unwrap().iter().any(|r| r.contains(&unique)));
        let (s2, _) = svc(&file);
        assert!(s2.discover_repos(false).await.unwrap().iter().any(|r| r.contains(&unique)));
        // Vanished repo forgotten.
        std::fs::remove_dir_all(&repo).unwrap();
        let (s3, _) = svc(&file);
        assert!(!s3.discover_repos(false).await.unwrap().iter().any(|r| r.contains(&unique)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "[]");
        // Corrupt repos.json ignored.
        std::fs::write(&file, "not json").unwrap();
        assert!(svc(&file).0.discover_repos(false).await.unwrap().is_empty());
        // An existing folder without a git root stays remembered.
        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(&file, serde_json::to_string(&[norm(&plain.to_string_lossy())]).unwrap()).unwrap();
        svc(&file).0.discover_repos(false).await.unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("plain"));
        // A live session's cwd is compared normalized (trailing slash): its dead folder stays remembered.
        let gone = tmp.path().join("gone");
        std::fs::write(&file, serde_json::to_string(&[norm(&gone.to_string_lossy())]).unwrap()).unwrap();
        let (s4, _) = svc(&file);
        s4.test_live.lock().push(LiveInfo { cwd: format!("{}/", gone.to_string_lossy()), ..LiveInfo::default() });
        s4.discover_repos(false).await.unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("gone"));
    }
}
