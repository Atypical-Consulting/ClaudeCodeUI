#![warn(unused, dead_code)]
//! Port of `Components/Pages/Worktrees.razor` — spec 06 §6.6, 04 §1. Owner: ui-pages-a.

use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::pages::new_session::{file_name, go, set_title, use_go};
use crate::state::{use_app, use_ui};
use ccui_shared::api::WorktreeRunStep;
use ccui_shared::model::Time;
use ccui_shared::worktree_rules::{self as rules, display_name, WorktreeInfo, WtState};
use ccui_shared::{fmt, i18n, t};
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Titles are resource keys.
const GROUPS: [(&str, &str, &[WtState]); 3] = [
    ("Wt.Group.Go", "Wt.Group.GoHelp", &[WtState::Safe, WtState::Orphan]),
    ("Wt.State.Check", "Wt.Group.CheckHelp", &[WtState::Check]),
    ("Wt.Filter.Active", "Wt.Group.ActiveHelp", &[WtState::Active]),
];

thread_local! {
    /// Kept across visits: arrival shows the last sizes, then re-measures.
    static SIZES: RefCell<HashMap<String, i64>> = RefCell::new(HashMap::new());
    /// Shown with the cached rows, so the warnings never push the table down when the scan lands.
    static LAST_UNIGNORED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

type Sizes = HashMap<String, i64>;

/// The same ignored file, one decision.
pub fn ignored_only(w: &WorktreeInfo) -> bool {
    !w.branch_only && w.state == WtState::Check && w.kept.is_some() && w.dirty == 0 && w.at_risk() == 0 && rules::cleanable(w, true)
}

/// Folded per repo and kind: orphaned branches, detached Safe folders, squash-merged Safe folders, squashed branches
/// whose folder went, the same ignored file.
pub fn folds(w: &WorktreeInfo) -> bool {
    w.branch_only && w.state == WtState::Orphan
        || !w.branch_only && w.state == WtState::Safe && (w.branch.is_none() || w.squash_merged && !w.merged)
        || w.branch_only && w.state == WtState::Check && w.squash_merged && !w.merged
        || ignored_only(w)
}

pub fn fold_kind(w: &WorktreeInfo) -> char {
    if w.branch_only {
        if w.state == WtState::Check { 'q' } else { 'b' }
    } else if ignored_only(w) {
        'i'
    } else if w.branch.is_none() {
        'd'
    } else {
        's'
    }
}

pub fn fold_key(w: &WorktreeInfo) -> String {
    let k = fold_kind(w);
    let mut s = format!("fold:{k}:{}", w.repo);
    if k == 'i' {
        s.push(':');
        s.push_str(w.kept.as_deref().unwrap_or(""));
    }
    s
}

/// Branches, then detached folders; Check rows: commits nothing else holds, then uncommitted files…
pub fn risk(w: &WorktreeInfo) -> i32 {
    if w.state != WtState::Check {
        return if w.branch_only { 2 } else if w.branch.is_none() { 1 } else { 0 };
    }
    if w.at_risk() > 0 {
        0
    } else if w.dirty > 0 {
        1
    } else if w.unreadable.is_some() {
        2
    } else if w.kept.is_some() {
        3
    } else if w.recent_min.is_some() {
        4
    } else if w.branch_only {
        6
    } else if w.locked {
        7
    } else {
        5
    }
}

/// Riskiest first, then a fixed order: the cached rows and the fresh scan list the same rows in the same place.
pub fn order(mut g: Vec<WorktreeInfo>, by_size: bool, sizes: &Sizes) -> Vec<WorktreeInfo> {
    if by_size {
        let sz = |w: &WorktreeInfo| sizes.get(&w.path).copied().unwrap_or(-1);
        g.sort_by(|a, b| sz(b).cmp(&sz(a)).then_with(|| a.path.cmp(&b.path)));
    } else {
        g.sort_by(|a, b| {
            risk(a)
                .cmp(&risk(b))
                .then(b.at_risk().cmp(&a.at_risk()))
                .then(b.dirty.cmp(&a.dirty))
                .then(b.last_activity.cmp(&a.last_activity))
                .then_with(|| file_name(&a.repo).to_lowercase().cmp(&file_name(&b.repo).to_lowercase()))
                .then_with(|| display_name(a).cmp(display_name(b)))
        });
    }
    g
}

pub enum Part {
    Row(WorktreeInfo),
    Fold(Vec<WorktreeInfo>),
}

/// One ordered sequence: a fold (≥ 2 members of a kind in a repo) takes the place of its first member, so it keeps
/// the risk order and the size sort.
pub fn split(g: Vec<WorktreeInfo>) -> Vec<Part> {
    let mut groups: Vec<(String, Vec<WorktreeInfo>)> = Vec::new();
    for w in g.iter().filter(|w| folds(w)) {
        let k = fold_key(w);
        match groups.iter_mut().find(|(x, _)| *x == k) {
            Some((_, v)) => v.push(w.clone()),
            None => groups.push((k, vec![w.clone()])),
        }
    }
    groups.retain(|(_, v)| v.len() >= 2);
    let folded: HashSet<&str> = groups.iter().flat_map(|(_, v)| v.iter().map(|w| w.path.as_str())).collect();
    let mut out = Vec::new();
    for w in &g {
        if let Some(i) = groups.iter().position(|(_, v)| v[0].path == w.path) {
            out.push(Part::Fold(groups[i].1.clone()));
        } else if !folded.contains(w.path.as_str()) {
            out.push(Part::Row(w.clone()));
        }
    }
    out
}

/// "5 worktrees + 21 branches": branch-only rows are not worktrees.
pub fn items(ws: &[WorktreeInfo]) -> String {
    let b = ws.iter().filter(|w| w.branch_only).count() as i64;
    let n = ws.len() as i64 - b;
    if b == 0 {
        i18n::plural(n, "Wt.Worktrees")
    } else if n == 0 {
        i18n::plural(b, "Wt.Branches")
    } else {
        format!("{} + {}", i18n::plural(n, "Wt.Worktrees"), i18n::plural(b, "Wt.Branches"))
    }
}

/// Branches alone have no size, as in the table; "…" until every folder is measured.
pub fn size_of(ws: &[WorktreeInfo], sizes: &Sizes) -> String {
    let known: Vec<&WorktreeInfo> = ws.iter().filter(|w| w.state != WtState::Orphan && !w.branch_only).collect();
    if known.is_empty() {
        return "—".into();
    }
    if known.iter().all(|w| sizes.contains_key(&w.path)) {
        fmt::size(known.iter().map(|w| sizes[&w.path]).sum())
    } else {
        "…".into()
    }
}

pub fn what(w: &WorktreeInfo, kept_ok: bool) -> String {
    if let (true, Some(k)) = (kept_ok, &w.kept) {
        return t!("Wt.What.WithKept", k);
    }
    if w.branch_only {
        return t!("Wt.What.BranchOnly");
    }
    if w.state == WtState::Orphan {
        return t!("Wt.What.RefOnly");
    }
    let key = if w.branch.is_none() {
        "Wt.What.Detached"
    } else if !w.merged {
        "Wt.What.FolderOnly"
    } else {
        "Wt.What.FolderAndBranch"
    };
    format!("{}{}", if w.locked { t!("Wt.What.Unlock") } else { String::new() }, t!(key))
}

/// An agent's scratch repo needs no .gitignore advice.
/// ponytail: the C# also matched `Path.GetTempPath()`; on Windows that is %TEMP%, unknown to the webview.
pub fn scratch(repo: &str) -> bool {
    crate::pages::overview::scratch(repo)
}

/// Tags stripped, entities decoded (a `title`).
pub fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    let mut s = out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    while let Some(i) = s.find("&#x") {
        let Some(end) = s[i..].find(';') else { break };
        let ch = u32::from_str_radix(&s[i + 3..i + end], 16).ok().and_then(char::from_u32);
        let Some(ch) = ch else { break };
        s.replace_range(i..i + end + 1, &ch.to_string());
    }
    s.replace("&amp;", "&")
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogLine {
    pub repo: String,
    pub cmd: String,
    pub result: Option<String>,
}

/// `.cmdlog` is white-space:pre-wrap: one encoded string, "# repo" comments and results dimmed.
pub fn cmd_log(lines: &[LogLine], home: &str) -> String {
    let mut repos: Vec<&str> = Vec::new();
    for l in lines {
        if !repos.contains(&l.repo.as_str()) {
            repos.push(&l.repo);
        }
    }
    let mut sb = String::new();
    for r in repos {
        if !sb.is_empty() {
            sb.push('\n');
        }
        sb.push_str(&format!("<span class=\"c\"># {}</span>", fmt::enc(&fmt::home(r, home))));
        for l in lines.iter().filter(|l| l.repo == r) {
            sb.push('\n');
            sb.push_str(&fmt::enc(&l.cmd));
            if let Some(res) = &l.result {
                sb.push_str(&format!("\n<span class=\"c\">→ {}</span>", fmt::enc(res)));
            }
        }
    }
    sb
}

/// A run step's result line: "ok", the output of prune / branch -d ("Deleted branch x (was 1a2b3c4)": the sha to
/// restore it), the runner's refusal, or the exit code.
pub fn step_result(display: &str, exit: i32, output: &str) -> String {
    let first: Vec<&str> = output.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect();
    match exit {
        0 => {
            let verb = display.split(' ').nth(2).unwrap_or("");
            if !first.is_empty() && (verb == "prune" || verb == "-d") { first.join(" · ") } else { "ok".into() }
        }
        -1 => output.to_string(),
        _ => t!("Wt.ExitOutput", exit, first.join(" · ")),
    }
}

/// A row's action set (RowActions, first matching branch).
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Unplan,
    /// prune (a folder already gone) vs delete.
    Add(bool),
    Go(String),
    Push,
    UnplanKept,
    /// Copy path, + "Delete anyway" when cleanable with its kept files.
    Kept(bool),
    CopyActive,
    /// New session, with Copy path first when dirty.
    Plain(bool),
    None,
}

pub fn act_of(w: &WorktreeInfo, planned: bool, kept_ok: bool, session: Option<String>) -> Act {
    if rules::cleanable(w, false) {
        return if planned { Act::Unplan } else { Act::Add(w.state == WtState::Orphan && !w.branch_only) };
    }
    if w.state == WtState::Active {
        if let Some(s) = session {
            return Act::Go(s);
        }
    }
    if w.at_risk() > 0 && w.dirty == 0 && w.branch.is_some() && !w.branch_only {
        return Act::Push;
    }
    if kept_ok && planned {
        return Act::UnplanKept;
    }
    if w.kept.is_some() && w.dirty == 0 && w.at_risk() == 0 {
        return Act::Kept(rules::cleanable(w, true));
    }
    if w.state == WtState::Active && !w.branch_only {
        return Act::CopyActive;
    }
    if !w.branch_only {
        return Act::Plain(w.dirty > 0);
    }
    Act::None
}

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Group(&'static str, &'static str),
    Row(String, bool),
    Fold(String),
}

fn entry_key(e: &Entry) -> String {
    match e {
        Entry::Group(t, _) => format!("g:{t}"),
        Entry::Row(p, child) => format!("{p}|{child}"),
        Entry::Fold(k) => k.clone(),
    }
}

#[derive(Clone, Default, PartialEq)]
struct Layout {
    entries: Vec<Entry>,
    by_path: HashMap<String, WorktreeInfo>,
    folds: HashMap<String, Vec<WorktreeInfo>>,
    /// Keys of the rows in display order (fold rows by their key), for the roving Tab stop.
    shown: Vec<String>,
}

fn layout(rows: &[WorktreeInfo], filter: Option<WtState>, open: &HashSet<String>, by_size: bool, sizes: &Sizes) -> Layout {
    let mut l = Layout { by_path: rows.iter().map(|w| (w.path.clone(), w.clone())).collect(), ..Layout::default() };
    for (title, help, states) in GROUPS {
        let g: Vec<WorktreeInfo> =
            rows.iter().filter(|w| states.contains(&w.state) && filter.is_none_or(|f| w.state == f)).cloned().collect();
        if g.is_empty() {
            continue;
        }
        l.entries.push(Entry::Group(title, help));
        for part in split(order(g, by_size, sizes)) {
            match part {
                Part::Row(w) => {
                    l.shown.push(w.path.clone());
                    l.entries.push(Entry::Row(w.path, false));
                }
                Part::Fold(f) => {
                    let key = fold_key(&f[0]);
                    l.shown.push(key.clone());
                    l.entries.push(Entry::Fold(key.clone()));
                    if open.contains(&key) || filter.is_some() {
                        for w in &f {
                            l.shown.push(w.path.clone());
                            l.entries.push(Entry::Row(w.path.clone(), true));
                        }
                    }
                    l.folds.insert(key, f);
                }
            }
        }
    }
    l
}

/// Repos → worktree rows (state, why, size), plan preview, run log (Push::WorktreeRunStep), push. `?path=`.
#[component]
pub fn Worktrees() -> impl IntoView {
    let app = use_app();
    let ui = use_ui();
    let nav = use_go();
    let query = use_query_map();
    set_title(&t!("Nav.Worktrees"));
    let home = StoredValue::new(app.boot.with_value(|b| b.home.clone()));

    let rows = RwSignal::new(None::<Vec<WorktreeInfo>>);
    let repos_n = RwSignal::new(0usize);
    let unignored = RwSignal::new(Vec::<String>::new());
    let errors = RwSignal::new(Vec::<String>::new());
    let sizes = RwSignal::new(SIZES.with(|s| s.borrow().clone()));
    let plan = RwSignal::new(HashSet::<String>::new());
    let kept_ok = RwSignal::new(HashSet::<String>::new()); // rows planned with "Delete anyway": their ignored files go too
    let log = RwSignal::new(Vec::<LogLine>::new());
    let filter = RwSignal::new(None::<WtState>);
    let selected = RwSignal::new(None::<String>);
    let push_result = RwSignal::new(None::<String>);
    let push = RwSignal::new(None::<WorktreeInfo>);
    let scanning = RwSignal::new(false);
    let running = RwSignal::new(false);
    let cursor = RwSignal::new(None::<String>); // the row that holds the list's one Tab stop
    let by_size = RwSignal::new(false);
    let open = RwSignal::new(HashSet::<String>::new());
    let from_cache = RwSignal::new(false);
    let cached_at = RwSignal::new(None::<Time>);
    let progress = RwSignal::new((0usize, 0usize));
    let pulsed = RwSignal::new(false); // the landing pulse played once; the fresh scan's re-render must not replay it
    let revealed = StoredValue::new(false);
    let seeded = StoredValue::new(false); // the landed row's fold was opened once, and stays closable
    let focus_to = RwSignal::new(None::<String>);
    let plan_origin = StoredValue::new(None::<String>);
    let cur_run = StoredValue::new(None::<u64>);
    let pane_el = NodeRef::<leptos::html::Div>::new();
    let plan_el = NodeRef::<leptos::html::Div>::new();
    let head_btn = NodeRef::<leptos::html::Button>::new();

    // ?path=: the Overview links one worktree; its row is selected and scrolled into view once the scan lists it.
    let focus = Memo::new(move |_| query.with(|q| q.get("path")).filter(|p| !p.is_empty()));

    let seed_focus = move || {
        // The landed row's fold opens once; after that it is the user's to close.
        let Some(f) = focus.get_untracked() else { return };
        if seeded.get_value() {
            return;
        }
        let fw = rows.with_untracked(|r| r.as_ref().and_then(|r| r.iter().find(|w| w.path == f).cloned()));
        if let Some(fw) = fw.filter(folds) {
            open.update(|o| {
                o.insert(fold_key(&fw));
            });
            seeded.set_value(true);
        }
    };

    let measure = move |path: String| {
        leptos::task::spawn_local(async move {
            if let Ok(z) = ipc::worktrees_size(&path).await {
                SIZES.with(|s| s.borrow_mut().insert(path.clone(), z));
                sizes.update(|m| {
                    m.insert(path, z);
                });
            }
        });
    };

    let scan = move |refresh: bool| async move {
        if scanning.get_untracked() {
            return;
        }
        scanning.set(true);
        // The last scan at once (an Overview link lands on its row), then the fresh one.
        if rows.get_untracked().is_none() {
            if let Ok(c) = ipc::worktrees_cached().await {
                if rows.is_disposed() {
                    return;
                }
                if c.cached_at.is_some() && rows.get_untracked().is_none() {
                    cached_at.set(c.cached_at);
                    rows.set(Some(c.rows));
                    from_cache.set(true);
                    unignored.set(LAST_UNIGNORED.with(|u| u.borrow().clone()));
                    seed_focus();
                }
            }
        }
        match ipc::worktrees_discover(refresh).await {
            Err(e) => {
                // git missing: no repo can be resolved
                rows.set(Some(Vec::new()));
                unignored.set(Vec::new());
                errors.set(vec![e]);
            }
            Ok(repos) => {
                if rows.is_disposed() {
                    return;
                }
                let home = home.get_value(); // read while mounted: the loop below awaits
                repos_n.set(repos.len());
                progress.set((0, repos.len()));
                let (mut found, mut bad, mut errs) = (Vec::new(), Vec::new(), Vec::new());
                for r in &repos {
                    match ipc::worktrees_scan(r, None).await {
                        Ok(list) => {
                            if !list.is_empty() && !scratch(r) && !ipc::worktrees_ignores(r).await.unwrap_or(true) {
                                bad.push(r.clone());
                            }
                            found.extend(list);
                        }
                        Err(e) => errs.push(t!("Wt.RepoError", fmt::home(r, &home), e)),
                    }
                    if rows.is_disposed() {
                        return;
                    }
                    progress.update(|p| p.0 += 1); // counted once read, so the last one never shows done early
                }
                LAST_UNIGNORED.with(|u| *u.borrow_mut() = bad.clone());
                let ko = kept_ok.get_untracked();
                plan.update(|p| p.retain(|path| found.iter().any(|w| w.path == *path && rules::cleanable(w, ko.contains(path)))));
                let pl = plan.get_untracked();
                kept_ok.update(|k| k.retain(|p| pl.contains(p)));
                // over the old sizes, so the totals never blank
                for w in found.iter().filter(|w| w.state != WtState::Orphan && !w.branch_only) {
                    measure(w.path.clone());
                }
                rows.set(Some(found));
                unignored.set(bad);
                errors.set(errs);
                from_cache.set(false);
                seed_focus();
            }
        }
        scanning.set(false);
        progress.set((0, 0));
    };
    leptos::task::spawn_local(scan(false));

    // The clicked button is replaced (Delete… becomes Keep): focus goes back to its row rather than to the page.
    let focus_row = move |key: String| {
        plan_origin.set_value(Some(key.clone())); // where Cancel hands focus back
        focus_to.set(Some(format!("row:{key}")));
    };
    Effect::new(move |_| {
        let Some(to) = focus_to.get() else { return };
        focus_to.set(None);
        request_animation_frame(move || {
            if let Some(key) = to.strip_prefix("row:") {
                if let Some(p) = pane_el.try_get_untracked().flatten() {
                    interop::reveal(&p, &rules::anchor(key), "nearest", true);
                }
            } else if to == "plan" {
                if let Some(p) = plan_el.try_get_untracked().flatten() {
                    let _ = p.focus();
                }
            } else if let Some(b) = head_btn.try_get_untracked().flatten() {
                let _ = b.focus();
            }
        });
    });
    // Landing: the linked row scrolls to the centre once listed, its pulse plays once.
    Effect::new(move |_| {
        let Some(f) = focus.get() else { return };
        if revealed.get_value() || !rows.with(|r| r.as_ref().is_some_and(|r| r.iter().any(|w| w.path == f))) {
            return;
        }
        revealed.set_value(true);
        request_animation_frame(move || {
            if let Some(p) = pane_el.try_get_untracked().flatten() {
                interop::reveal(&p, &rules::anchor(&f), "center", true);
            }
        });
        set_timeout(move || pulsed.set(true), Duration::from_millis(2600));
    });

    // The run's steps arrive as pushes tagged with its id; WorktreeRunDone ends it.
    let finish = move |id: u64| {
        cur_run.set_value(None);
        running.set(false);
        plan.set(HashSet::new());
        kept_ok.set(HashSet::new());
        app.wt_steps.update(|v| v.retain(|s| s.run_id != id));
        leptos::task::spawn_local(scan(false));
    };
    Effect::new(move |_| {
        let steps = app.wt_steps.get();
        let done = app.wt_done.get();
        let Some(id) = cur_run.get_value() else { return };
        let mine: Vec<&WorktreeRunStep> = steps.iter().filter(|s| s.run_id == id).collect();
        if !mine.is_empty() {
            log.set(mine.iter().map(|s| LogLine { repo: s.repo.clone(), cmd: s.display.clone(), result: Some(step_result(&s.display, s.exit, &s.output)) }).collect());
        }
        if done == Some(id) {
            finish(id);
        }
    });
    on_cleanup(move || {
        if let Some(id) = cur_run.try_get_value().flatten() {
            leptos::task::spawn_local(async move {
                let _ = ipc::worktrees_cancel_run(id).await;
            });
        }
    });

    let run = move |chosen: Vec<WorktreeInfo>| {
        if running.get_untracked() || chosen.is_empty() {
            return;
        }
        let paths: Vec<String> = chosen.iter().map(|w| w.path.clone()).collect();
        let kept: Vec<String> = kept_ok.get_untracked().into_iter().collect();
        let repo = chosen[0].repo.clone();
        let id = js_sys::Date::now() as u64 ^ (js_sys::Math::random() * 1e9) as u64;
        running.set(true);
        log.set(Vec::new());
        cur_run.set_value(Some(id));
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::worktrees_run(id, &paths, &kept).await {
                log.update(|l| l.push(LogLine { repo, cmd: "git".into(), result: Some(e) }));
                if cur_run.try_get_value().flatten() == Some(id) {
                    finish(id);
                }
            }
        });
    };

    let do_push = move |w: WorktreeInfo| {
        if running.get_untracked() {
            return;
        }
        running.set(true);
        push_result.set(Some("…".into()));
        leptos::task::spawn_local(async move {
            let got = ipc::worktrees_push(&w.path, w.branch.as_deref()).await;
            if running.is_disposed() {
                return;
            }
            running.set(false);
            match got {
                Ok(0) => {
                    push_result.set(Some("ok".into()));
                    scan(false).await;
                }
                Ok(n) => push_result.set(Some(format!("exit {n}"))),
                Err(e) => push_result.set(Some(e)),
            }
        });
    };

    let add = move |path: String| {
        log.set(Vec::new());
        plan.update(|p| {
            p.insert(path.clone());
        });
        ui.inspector_open.set(true); // the plan it joined is in view
        focus_row(path);
    };

    let lay = Memo::new(move |_| {
        rows.with(|r| {
            let r = r.as_deref().unwrap_or_default();
            open.with(|o| sizes.with(|s| layout(r, filter.get(), o, by_size.get(), s)))
        })
    });
    // One Tab stop: the row the keys last reached, else the landed row, else the first.
    let cur = Memo::new(move |_| {
        lay.with(|l| {
            let c = cursor.get().filter(|c| l.shown.contains(c));
            c.or_else(|| focus.get().filter(|f| l.shown.contains(f))).or_else(|| l.shown.first().cloned())
        })
    });

    let session_of = move |w: &WorktreeInfo| {
        let p = w.path.to_lowercase();
        app.summary.with(|s| s.sessions.iter().find(|x| x.has_process && x.cwd.replace('\\', "/").to_lowercase().starts_with(&p)).map(|x| x.id.clone()))
    };

    let row_view = move |path: String, child: bool| {
        let path_s = StoredValue::new(path.clone());
        let Some(first) = lay.with_untracked(|l| l.by_path.get(&path).cloned()) else { return ().into_any() };
        let w = Memo::new(move |prev: Option<&WorktreeInfo>| {
            lay.with(|l| l.by_path.get(&path_s.get_value()).cloned()).or_else(|| prev.cloned()).unwrap_or_else(|| first.clone())
        });
        let tab = move || if cur.with(|c| c.as_deref() == Some(path_s.get_value().as_str())) { 0 } else { -1 };
        let planned = move || plan.with(|p| p.contains(&path_s.get_value()));
        let act = Memo::new(move |_| {
            let w = w.get();
            let p = path_s.get_value();
            act_of(&w, plan.with(|x| x.contains(&p)), kept_ok.with(|x| x.contains(&p)), session_of(&w))
        });
        let name = move || w.with(|w| display_name(w).to_string());
        let size_cell = move || {
            let w = w.get();
            if w.state == WtState::Orphan || w.branch_only {
                "—".to_string()
            } else {
                sizes.with(|s| s.get(&w.path).map_or_else(|| "…".to_string(), |z| fmt::size(*z)))
            }
        };
        let copy_path = move |title: String| {
            view! {
                <button type="button" class="btn ghost" tabindex=tab data-copy=path_s.get_value() title=title aria-label=move || t!("Wt.CopyPathOf", name())>{t!("Wt.CopyPath")}</button>
            }
        };
        let unplan = move |kept_too: bool| {
            view! {
                <button class="btn ghost" disabled=move || running.get() tabindex=tab aria-label=move || format!("{} {}", t!("Wt.Unplan"), name())
                    on:click=move |_| {
                        let p = path_s.get_value();
                        plan.update(|x| { x.remove(&p); });
                        if kept_too {
                            kept_ok.update(|x| { x.remove(&p); });
                        }
                        focus_row(p);
                    }>{t!("Wt.Unplan")}</button>
            }
        };
        let actions = move || match act.get() {
            Act::Unplan => unplan(false).into_any(),
            Act::UnplanKept => unplan(true).into_any(),
            Act::Add(prune) => {
                // prune is git's word for a folder already gone
                let verb = t!(if prune { "Wt.Prune" } else { "Wt.Delete" });
                let v2 = verb.clone();
                view! {
                    <button class="btn" disabled=move || running.get() tabindex=tab title=t!("Wt.AddsToPlan") aria-label=move || format!("{v2} {}", name())
                        on:click=move |_| add(path_s.get_value())>{verb}</button>
                }
                .into_any()
            }
            Act::Go(id) => view! {
                <button class="btn" tabindex=tab aria-label=move || format!("{} {}", t!("Wt.Go"), name()) on:click=move |_| go(nav, &format!("session/{id}"))>{t!("Wt.Go")}</button>
            }
            .into_any(),
            Act::Push => view! {
                <button class="btn" disabled=move || running.get() tabindex=tab aria-label=move || format!("{} {}", t!("Wt.Push"), name())
                    on:click=move |_| {
                        selected.set(Some(path_s.get_value()));
                        push.set(Some(w.get_untracked()));
                        push_result.set(None);
                        ui.inspector_open.set(true);
                    }>{t!("Wt.Push")}</button>
            }
            .into_any(),
            // look first (the path to the clipboard, confirmed in place), then decide; never --force: git removes
            // ignored files itself
            Act::Kept(anyway) => view! {
                {copy_path(t!("Wt.CopyPathHelp"))}
                {anyway.then(|| view! {
                    <button class="btn" disabled=move || running.get() tabindex=tab title=move || t!("Wt.DeleteAnywayHelp", w.with(|w| w.kept.clone().unwrap_or_default()))
                        aria-label=move || t!("Wt.DeleteAnywayOf", name()) on:click=move |_| {
                            let p = path_s.get_value();
                            log.set(Vec::new());
                            kept_ok.update(|x| { x.insert(p.clone()); });
                            plan.update(|x| { x.insert(p.clone()); });
                            ui.inspector_open.set(true);
                            focus_row(p);
                        }>{t!("Wt.DeleteAnyway")}</button>
                })}
            }
            .into_any(),
            Act::CopyActive => copy_path(t!("Wt.CopyPathActive")).into_any(),
            Act::Plain(dirty) => view! {
                {dirty.then(|| copy_path(t!("Wt.CopyPathHelp")))}
                <button class="btn" tabindex=tab title=t!("Wt.NewSessionIn", path_s.get_value()) aria-label=move || t!("Wt.NewSessionIn", name())
                    on:click=move |_| go(nav, &format!("?cwd={}", String::from(js_sys::encode_uri_component(&path_s.get_value()))))>{t!("Wt.NewSession")}</button>
            }
            .into_any(),
            Act::None => ().into_any(),
        };
        view! {
            <tr id=rules::anchor(&path) data-r="" tabindex=tab on:focusin=move |_| cursor.set(Some(path_s.get_value()))
                class=move || {
                    let p = path_s.get_value();
                    let a = if selected.get().as_deref() == Some(p.as_str()) || planned() { "sel" } else if w.with(|w| w.state == WtState::Orphan) { "dim" } else { "" };
                    let b = if focus.get().as_deref() == Some(p.as_str()) && !pulsed.get() { "landed" } else { "" };
                    format!("{a} {b} {}", if child { "fchild" } else { "" })
                }>
                // the folder first on the second line: it is what tells two rows on the same branch apart
                {move || {
                    let w = w.get();
                    let lead = (!w.branch_only && w.branch.is_some()).then(|| w.name.clone());
                    let size = (!w.branch_only && w.state != WtState::Orphan).then(|| sizes.with(|s| s.get(&w.path).copied())).flatten();
                    let why2 = match size {
                        Some(z) => format!("{}<span class=\"sz2\"> · {}</span>", w.why, fmt::enc(&fmt::size(z))),
                        None => w.why.clone(),
                    };
                    view! {
                        <td title=if w.branch_only { w.branch.clone().unwrap_or_default() } else { w.path.clone() }>
                            <div class="n">{display_name(&w).to_string()}{w.branch_only.then(|| view! { <span class="tag">{t!("Wt.BranchTag")}</span> })}</div>
                            <div class="branch"><Icon name="branch"/>
                                {(w.branch.is_none() && !w.branch_only).then(|| view! { <span class="tag">{t!(if w.state == WtState::Orphan { "Wt.BranchMissing" } else { "Wt.DetachedTag" })}</span> })}
                                {lead.map(|l| view! { <span class="lf"><bdi>{l}</bdi></span><span class="dot">"·"</span> })}
                                <span class="rp">{file_name(&w.repo).to_string()}</span></div>
                            // the reason under the name in a narrow pane
                            <div class="why2" aria-hidden="true" inner_html=why2></div>
                        </td>
                        <td class="why" title=plain(&w.why)><span class="cl" inner_html=w.why.clone()></span></td>
                    }
                }}
                <td class="num">{size_cell}</td>
                <td><div class="rowacts">{actions}</div></td>
            </tr>
        }
        .into_any()
    };

    let fold_view = move |key: String| {
        let key_s = StoredValue::new(key.clone());
        let Some(first) = lay.with_untracked(|l| l.folds.get(&key).cloned()) else { return ().into_any() };
        let f = Memo::new(move |prev: Option<&Vec<WorktreeInfo>>| {
            lay.with(|l| l.folds.get(&key_s.get_value()).cloned()).or_else(|| prev.cloned()).unwrap_or_else(|| first.clone())
        });
        let all = move || f.with(|f| f.iter().map(|w| w.path.clone()).collect::<Vec<_>>());
        let planned = Memo::new(move |_| {
            let a = all();
            plan.with(|p| a.iter().all(|x| p.contains(x)))
        });
        let tab = move || if cur.with(|c| c.as_deref() == Some(key_s.get_value().as_str())) { 0 } else { -1 };
        let is_open = move || open.with(|o| o.contains(&key_s.get_value())) || filter.get().is_some();
        let kind = fold_kind(&f.get_untracked()[0]);
        let branches = kind == 'b' || kind == 'q';
        let repo_name = move || f.with(|f| file_name(&f[0].repo).to_string());
        let kept = move || f.with(|f| f[0].kept.clone().unwrap_or_default());
        let label = move || {
            let n = f.with(Vec::len) as i64;
            if kind == 'i' {
                t!("Wt.FoldIgnored", n, kept())
            } else {
                i18n::plural(n, match kind { 'b' => "Wt.FoldBranches", 's' => "Wt.FoldSquash", 'q' => "Wt.FoldKeptBranches", _ => "Wt.FoldDetached" })
            }
        };
        let fold_help = match kind {
            'b' => "Wt.FoldHelp",
            's' => "Wt.FoldSquashHelp",
            'q' => "Wt.FoldKeptBranchesHelp",
            'i' => "Wt.FoldIgnoredHelp",
            _ => "Wt.FoldDetachedHelp",
        };
        let actions = move || {
            if kind == 'q' {
                // this page never forces a delete: the command is the user's to run
                let cmd = f.with(|f| format!("git -C {} branch -D {}", f[0].repo, f.iter().filter_map(|w| w.branch.clone()).collect::<Vec<_>>().join(" ")));
                return view! { <button type="button" class="btn ghost" tabindex=tab data-copy=cmd title=t!("Wt.CopyCommandsHelp")>{t!("Wt.CopyCommands")}</button> }.into_any();
            }
            if planned.get() {
                return view! {
                    <button class="btn ghost" disabled=move || running.get() tabindex=tab aria-label=move || format!("{} {}", t!("Wt.Unplan"), repo_name())
                        on:click=move |_| {
                            let a = all();
                            plan.update(|p| p.retain(|x| !a.contains(x)));
                            kept_ok.update(|p| p.retain(|x| !a.contains(x)));
                            focus_row(key_s.get_value());
                        }>{t!("Wt.Unplan")}</button>
                }
                .into_any();
            }
            if kind == 'i' {
                // the warning verb, wrapping in its cell: it deletes files git can't restore
                return view! {
                    <button class="btn wrapl" disabled=move || running.get() tabindex=tab title=move || t!("Wt.DeleteAllAnywayHelp", kept())
                        aria-label=move || t!("Wt.DeleteAllAnywayIn", repo_name(), kept()) on:click=move |_| {
                            let a = all();
                            log.set(Vec::new());
                            kept_ok.update(|p| p.extend(a.iter().cloned()));
                            plan.update(|p| p.extend(a));
                            ui.inspector_open.set(true);
                            focus_row(key_s.get_value());
                        }>{t!("Wt.DeleteAllAnyway")}</button>
                }
                .into_any();
            }
            let aria = match kind { 'b' => "Wt.PruneAllIn", 's' => "Wt.DeleteSquashIn", _ => "Wt.DeleteDetachedIn" };
            view! {
                <button class="btn" disabled=move || running.get() tabindex=tab title=t!("Wt.AddsToPlanAll") aria-label=move || t!(aria, repo_name())
                    on:click=move |_| {
                        log.set(Vec::new());
                        plan.update(|p| p.extend(all()));
                        ui.inspector_open.set(true);
                        focus_row(key_s.get_value());
                    }>{t!("Wt.DeleteAll")}</button>
            }
            .into_any()
        };
        view! {
            <tr id=rules::anchor(&key) class=move || if planned.get() { "wfold sel" } else { "wfold" } data-r="" tabindex=tab
                on:focusin=move |_| cursor.set(Some(key_s.get_value()))>
                <td colspan="2">
                    // the row's keys open it: no second Tab stop
                    <button class="fold-t" aria-expanded=move || if is_open() { "true" } else { "false" } aria-disabled=move || filter.get().is_some().then_some("true") tabindex="-1"
                        on:click=move |_| {
                            if filter.get_untracked().is_none() {
                                let k = key_s.get_value();
                                open.update(|o| if !o.remove(&k) { o.insert(k); });
                            }
                        }><Icon name="right" sm=true/>{label}" · "{repo_name}</button>
                    <span class="gh">{t!(fold_help)}{(!branches).then(|| view! { <span class="sz2">" · "{move || f.with(|f| sizes.with(|s| size_of(f, s)))}</span> })}</span>
                </td>
                <td class="num">{move || if branches { "—".to_string() } else { f.with(|f| sizes.with(|s| size_of(f, s))) }}</td>
                <td><div class="rowacts">{actions}</div></td>
            </tr>
        }
        .into_any()
    };

    let entry_view = move |e: Entry| match e {
        Entry::Group(title, help) => view! { <tr class="grp-row"><th colspan="4" scope="rowgroup">{t!(title)}<span class="gh">{t!(help)}</span></th></tr> }.into_any(),
        Entry::Row(path, child) => row_view(path, child),
        Entry::Fold(key) => fold_view(key),
    };

    let safe_rows = Memo::new(move |_| rows.with(|r| r.iter().flatten().filter(|w| matches!(w.state, WtState::Safe | WtState::Orphan)).cloned().collect::<Vec<_>>()));
    let count = move |s: WtState| rows.with(|r| r.iter().flatten().filter(|w| w.state == s).count());

    let head = move || {
        let safe = safe_rows.get();
        let check = count(WtState::Check) as i64;
        let mixed = safe.iter().any(|w| w.branch_only);
        let h2 = if safe.is_empty() {
            t!("Wt.NothingToClean")
        } else if mixed {
            t!("Wt.SafeMixed", items(&safe))
        } else {
            i18n::plural(safe.len() as i64, "Wt.Safe")
        };
        let size = sizes.with(|s| size_of(&safe, s));
        // the count is the way to the decisions; no "… to reclaim" before the sizes are measured
        let p = if safe.is_empty() {
            i18n::plural(check, "Wt.CheckOnly").into_any()
        } else {
            view! {
                {if size == "…" { String::new() } else { format!("{} ", t!("Wt.Recoverable", size)) }}
                {if check > 0 {
                    view! { <button tabindex="0" class="linkish" title=t!("Wt.ShowCheck") on:click=move |_| filter.set(Some(WtState::Check))>{i18n::plural(check, "Wt.Check")}</button> }.into_any()
                } else {
                    i18n::plural(check, "Wt.Check").into_any()
                }}
                {t!("Wt.NothingActive")}
            }
            .into_any()
        };
        let add_label = if mixed { t!("Wt.AddSafe", items(&safe)) } else { i18n::plural(safe.len() as i64, "Wt.AddSafe") };
        let paths: Vec<String> = safe.iter().map(|w| w.path.clone()).collect();
        // kept in place, hidden, while a plan is open: the table never jumps
        let held = move || plan.with(|p| !p.is_empty()) || log.with(|l| !l.is_empty());
        view! {
            <div class="wt-head"><div><h2>{h2}</h2><p>{p}</p></div>
                {(!safe.is_empty()).then(|| view! {
                    // the verdict's action, beside the verdict
                    <button tabindex=move || if held() { -1 } else { 0 } aria-hidden=move || held().then_some("true") class=move || if held() { "btn held" } else { "btn" }
                        node_ref=head_btn on:click=move |_| {
                            plan.update(|p| p.extend(paths.iter().cloned()));
                            ui.inspector_open.set(true);
                            focus_to.set(Some("plan".into()));
                        }>{add_label}</button>
                })}
            </div>
        }
    };

    let chip = move |state: Option<WtState>, dot: Option<&'static str>, label: String| {
        let on = move || filter.get() == state;
        view! {
            <button tabindex="0" class=move || if on() { "chip on" } else { "chip" } aria-pressed=move || if on() { "true" } else { "false" } on:click=move |_| filter.set(state)>
                {dot.map(|d| view! { <span class=format!("state {d}")><i></i></span> })}{label}" "
                <b>{move || match state { None => rows.with(|r| r.as_ref().map_or(0, Vec::len)), Some(s) => count(s) }}</b></button>
        }
    };

    let table_part = move || {
        view! {
            {head}
            {move || unignored.get().into_iter().map(|r| {
                let b = format!("<b title=\"{}\">{}</b>", fmt::enc(&r), fmt::enc(file_name(&r)));
                view! { <div class="guard"><Icon name="shield" sm=true/><span inner_html=t!("Wt.Unignored", b)></span></div> }
            }).collect_view()}
            {move || errors.get().into_iter().map(|e| view! { <div class="errbox" role="alert">{e}</div> }).collect_view()}
            <div class="filters" role="group" aria-label=t!("Wt.FilterLabel")>
                {chip(None, None, t!("Filter.All"))}
                {chip(Some(WtState::Safe), Some("st-safe"), t!("Wt.Filter.Safe"))}
                {chip(Some(WtState::Orphan), Some("st-orphan"), t!("Wt.Filter.Orphan"))}
                {chip(Some(WtState::Check), Some("st-check"), t!("Wt.State.Check"))}
                {chip(Some(WtState::Active), Some("st-active"), t!("Wt.Filter.Active"))}
            </div>
            <table class="wtt" aria-label=t!("Nav.Worktrees")>
                <thead><tr><th scope="col">{t!("Wt.Col.Worktree")}</th><th scope="col">{t!("Wt.Col.Why")}</th>
                    <th scope="col" class="num" aria-sort=move || if by_size.get() { "descending" } else { "none" }>
                        <button tabindex="0" class=move || if by_size.get() { "thsort on" } else { "thsort" } title=t!("Wt.SortSize") on:click=move |_| by_size.update(|b| *b = !*b)>
                            {t!("Wt.Col.Size")}{move || if by_size.get() { " ↓" } else { "" }}</button></th>
                    <th scope="col"><span class="vh">{t!("Col.Actions")}</span></th></tr></thead>
                <tbody>
                    <For each=move || lay.with(|l| l.entries.clone()) key=entry_key children=entry_view/>
                </tbody>
            </table>
        }
    };

    let has_rows = Memo::new(move |_| rows.with(Option::is_some));

    let header_sub = move || {
        (has_rows.get() && !from_cache.get() && !scanning.get()).then(|| {
            rows.with(|r| {
                let r = r.as_deref().unwrap_or_default();
                let mut repos: Vec<&str> = r.iter().map(|w| w.repo.as_str()).collect();
                repos.sort_unstable();
                repos.dedup();
                // the repos that have rows; the scanned count is the title
                let txt = format!("{} · {} · {}", i18n::plural(repos.len() as i64, "Wt.Repos"), items(r), sizes.with(|s| size_of(r, s)));
                view! { <span class="sub" title=t!("Wt.ReposScanned", repos_n.get())>{txt}</span> }
            })
        })
    };
    let scan_status = move || {
        (scanning.get() && has_rows.get()).then(|| {
            let (d, n) = progress.get();
            let mut title = if n > 0 { format!("{} · ", t!("Wt.ScanProgress", d, n)) } else { String::new() };
            if let (true, Some(at)) = (from_cache.get(), cached_at.get()) {
                let mins = (chrono::Utc::now() - at).num_minutes().max(1);
                title.push_str(&t!("Wt.CachedAge", fmt::dur(mins * 60_000)));
            }
            let txt = if n > 0 { t!("Wt.ScanProgress", d, n) } else { t!("Wt.Scanning") };
            view! { <span class="sub" role="status" title=title>{txt}</span> }
        })
    };

    let chosen = Memo::new(move |_| rows.with(|r| plan.with(|p| r.iter().flatten().filter(|w| p.contains(&w.path)).cloned().collect::<Vec<_>>())));

    let push_section = move || {
        push.get().map(|pw| {
            let unpushed = if pw.unpushed == 1 {
                t!("Wt.Unpushed1", fmt::enc(pw.branch.as_deref().unwrap_or("")))
            } else {
                t!("Wt.UnpushedN", pw.unpushed, fmt::enc(pw.branch.as_deref().unwrap_or("")))
            };
            let line = |res: Option<String>| LogLine { repo: pw.path.clone(), cmd: format!("git push -u origin {}", pw.branch.clone().unwrap_or_default()), result: res };
            let line = StoredValue::new(line(None));
            let pw2 = pw.clone();
            let path = pw.path.clone();
            view! {
                <div class="section">
                    <h3>{t!("Wt.PushTitle", display_name(&pw).to_string())}</h3>
                    <p inner_html=unpushed></p>
                    <div class="cmdlog" role="status" inner_html=move || {
                        let mut l = line.get_value();
                        l.result = push_result.get();
                        cmd_log(&[l], &home.get_value())
                    }></div>
                    <div class="acts">
                        {move || (push_result.get().as_deref() != Some("ok")).then(|| {
                            let pw = pw2.clone();
                            view! { <button class="btn primary" disabled=move || running.get() on:click=move |_| do_push(pw.clone())>{t!("Wt.Push")}</button> }
                        })}
                        <button class="btn ghost" on:click=move |_| {
                            focus_row(path.clone());
                            push.set(None);
                            selected.set(None);
                        }>{move || t!(if push_result.get().as_deref() == Some("ok") { "Action.Close" } else { "Action.Cancel" })}</button>
                    </div>
                </div>
            }
        })
    };

    let plan_body = move || {
        let ch = chosen.get();
        let lg = log.get();
        if ch.is_empty() && lg.is_empty() {
            return view! { <p>{t!("Wt.NothingPlanned")}</p> }.into_any();
        }
        let ko = kept_ok.get();
        let sz = sizes.get();
        let n = ch.len();
        // one block per repo, folded once the plan is long: a plan of 120 rows must still fit a glance
        let mut groups: Vec<(String, Vec<WorktreeInfo>)> = Vec::new();
        for w in order(ch.clone(), by_size.get(), &sz) {
            match groups.iter_mut().find(|(r, _)| *r == w.repo) {
                Some((_, g)) => g.push(w),
                None => groups.push((w.repo.clone(), vec![w])),
            }
        }
        let blocks = groups
            .into_iter()
            .map(|(repo, g)| {
                let head = format!("{} · {}", items(&g), size_of(&g, &sz));
                let its = g
                    .iter()
                    .map(|w| {
                        let sub = format!("{}{}", if w.branch_only || w.branch.is_none() { String::new() } else { format!("{} · ", w.name) }, what(w, ko.contains(&w.path)));
                        let size = if w.state == WtState::Orphan || w.branch_only {
                            "—".to_string()
                        } else {
                            sz.get(&w.path).map_or_else(|| "…".to_string(), |z| fmt::size(*z))
                        };
                        view! { <div class="it"><Icon name="check" sm=true/><div><div class="n">{display_name(w).to_string()}</div><div class="w">{sub}</div></div><span class="sz">{size}</span></div> }
                    })
                    .collect_view();
                view! {
                    <details class="plan" open=n <= 8>
                        <summary><span class="n"><Icon name="right" sm=true/>{file_name(&repo).to_string()}</span><span class="w">{head}</span></summary>
                        {its}
                    </details>
                }
            })
            .collect_view();
        let lines = if lg.is_empty() {
            let kept: Vec<String> = ko.iter().cloned().collect();
            rules::plan(&ch, &kept).into_iter().map(|s| LogLine { repo: s.repo, cmd: s.display, result: None }).collect()
        } else {
            lg.clone()
        };
        let html = cmd_log(&lines, &home.get_value());
        let log_view = if !lg.is_empty() || lines.len() <= 12 {
            view! { <div class="cmdlog" inner_html=html></div> }.into_any()
        } else {
            view! { <details class="cmds"><summary><span><Icon name="right" sm=true/>{t!("Wt.PlanCmds", lines.len())}</span></summary><div class="cmdlog" inner_html=html></div></details> }.into_any()
        };
        let never_force = lines.iter().any(|l| l.cmd.contains(" branch -d "));
        let acts = if lg.is_empty() {
            let ch2 = ch.clone();
            view! {
                <button tabindex="0" class="btn primary" disabled=move || running.get() || chosen.with(Vec::is_empty) on:click=move |_| run(ch2.clone())>{t!("Wt.CleanN", items(&ch))}</button>
                <button tabindex="0" class="btn ghost" on:click=move |_| {
                    plan.set(HashSet::new());
                    kept_ok.set(HashSet::new());
                    let to = plan_origin.try_update_value(Option::take).flatten().map_or_else(|| "head".to_string(), |o| format!("row:{o}"));
                    focus_to.set(Some(to));
                }>{t!("Action.Cancel")}</button>
            }
            .into_any()
        } else {
            view! {
                <button class="btn" disabled=move || running.get() on:click=move |_| {
                    log.set(Vec::new());
                    plan.set(HashSet::new());
                    kept_ok.set(HashSet::new());
                }>{t!("Action.Close")}</button>
            }
            .into_any()
        };
        view! {
            {(!ch.is_empty()).then_some(blocks)}
            {log_view}
            {never_force.then(|| view! { <div class="guard"><Icon name="shield" sm=true/><span inner_html=t!("Wt.NeverForce")></span></div> })}
            <div class="acts">{acts}</div>
        }
        .into_any()
    };

    let inspector = move || {
        view! {
            <aside class="pane insp" aria-label=t!("Inspector")>
                <div class="ph"><h2 class="ttl">{t!("Wt.Cleanup")}</h2>
                    {move || has_rows.get().then(|| {
                        let ch = chosen.get();
                        let txt = if ch.is_empty() { t!("Wt.NothingPlannedShort") } else { format!("{} · {}", items(&ch), sizes.with(|s| size_of(&ch, s))) };
                        view! { <span class="sub">{txt}</span> }
                    })}</div>
                {push_section}
                // focus lands here after "Add the safe ones": the plan, never the Clean up button
                <div class="section" tabindex="-1" node_ref=plan_el>
                    <h3>{t!("Wt.WhatHappens")}</h3>
                    {plan_body}
                </div>
            </aside>
        }
    };

    view! {
        <div class="pane">
            <div class="ph wrap">
                <h1 class="ttl">{t!("Nav.Worktrees")}</h1>
                {header_sub}
                <span class="end">{scan_status}<button class=move || if scanning.get() { "btn busy" } else { "btn" } tabindex="0"
                    aria-disabled=move || if scanning.get() { "true" } else { "false" } on:click=move |_| leptos::task::spawn_local(scan(true))>
                    <Icon name="refresh" sm=true/>{t!("Wt.Refresh")}</button></span>
            </div>
            <div class="wt" node_ref=pane_el>
                {move || if has_rows.get() {
                    table_part().into_any()
                } else {
                    view! { <div class="wt-head"><div><h2>{t!("Wt.Scanning")}</h2><p>{t!("Wt.ScanningText")}</p></div></div> }.into_any()
                }}
            </div>
        </div>
        {move || ui.inspector_open.get().then(inspector)}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(path: &str, state: WtState) -> WorktreeInfo {
        WorktreeInfo {
            repo: "/r/app".into(),
            path: path.into(),
            name: file_name(path).into(),
            branch: Some(file_name(path).into()),
            head: None,
            state,
            why: "<b>merged</b> &amp; gone".into(),
            size_bytes: None,
            last_activity: None,
            locked: false,
            lock_reason: None,
            dirty: 0,
            unpushed: 0,
            merged: false,
            squash_merged: false,
            upstream_gone: false,
            branch_only: false,
            unreadable: None,
            pushed: None,
            recent_min: None,
            kept: None,
        }
    }

    #[test]
    fn folding() {
        let mut d1 = w("/r/app/.claude/worktrees/a", WtState::Safe);
        d1.branch = None;
        let mut d2 = w("/r/app/.claude/worktrees/b", WtState::Safe);
        d2.branch = None;
        let plain = w("/r/app/.claude/worktrees/c", WtState::Safe);
        let mut lone = w("/r/app/refs/heads/x", WtState::Orphan);
        lone.branch_only = true;
        assert_eq!(fold_kind(&d1), 'd');
        assert_eq!(fold_key(&lone), "fold:b:/r/app");
        let parts = split(vec![plain.clone(), d1, lone, d2]);
        let shape: Vec<String> = parts
            .iter()
            .map(|p| match p {
                Part::Row(r) => r.name.clone(),
                Part::Fold(f) => format!("fold{}", f.len()),
            })
            .collect();
        // the fold takes the place of its first member; a single member of a kind stays a row
        assert_eq!(shape, ["c", "fold2", "x"]);
    }

    #[test]
    fn ordering() {
        let mut risky = w("/r/z", WtState::Check);
        risky.unpushed = 3;
        let mut dirty = w("/r/y", WtState::Check);
        dirty.dirty = 2;
        let plain = w("/r/x", WtState::Check);
        let got: Vec<String> = order(vec![plain.clone(), dirty.clone(), risky.clone()], false, &Sizes::new()).into_iter().map(|w| w.path).collect();
        assert_eq!(got, ["/r/z", "/r/y", "/r/x"]);
        let mut s = Sizes::new();
        s.insert("/r/x".into(), 10);
        let got: Vec<String> = order(vec![risky, dirty, plain], true, &s).into_iter().map(|w| w.path).collect();
        assert_eq!(got, ["/r/x", "/r/y", "/r/z"]);
    }

    #[test]
    fn totals() {
        i18n::set_thread_lang(Some(i18n::Lang::En));
        let a = w("/r/a", WtState::Safe);
        let mut b = w("/r/b", WtState::Orphan);
        b.branch_only = true;
        let mut s = Sizes::new();
        assert_eq!(size_of(&[a.clone(), b.clone()], &s), "…");
        assert_eq!(size_of(&[b.clone()], &s), "—");
        s.insert("/r/a".into(), 2048);
        assert_eq!(size_of(&[a.clone(), b.clone()], &s), fmt::size(2048));
        assert_eq!(items(&[a.clone(), b.clone()]), format!("{} + {}", i18n::plural(1, "Wt.Worktrees"), i18n::plural(1, "Wt.Branches")));
        assert_eq!(items(&[a]), i18n::plural(1, "Wt.Worktrees"));
        i18n::set_thread_lang(None);
    }

    #[test]
    fn logs_and_results() {
        let lines = vec![
            LogLine { repo: "/h/r".into(), cmd: "git worktree remove a".into(), result: Some("ok".into()) },
            LogLine { repo: "/x".into(), cmd: "git branch -d <b>".into(), result: None },
            LogLine { repo: "/h/r".into(), cmd: "git worktree prune -v".into(), result: None },
        ];
        assert_eq!(
            cmd_log(&lines, "/h"),
            "<span class=\"c\"># ~/r</span>\ngit worktree remove a\n<span class=\"c\">→ ok</span>\ngit worktree prune -v\n<span class=\"c\"># /x</span>\ngit branch -d &lt;b&gt;"
        );
        assert_eq!(step_result("git branch -d x", 0, "Deleted branch x (was 1a2b3c4).\n"), "Deleted branch x (was 1a2b3c4).");
        assert_eq!(step_result("git worktree remove /a", 0, "done\n"), "ok");
        assert_eq!(step_result("git worktree remove /a", -1, "refused"), "refused");
        assert_eq!(plain("<b>merged</b> &amp; gone &#x2014;"), "merged & gone —");
    }

    #[test]
    fn row_actions() {
        let safe = w("/r/a", WtState::Safe);
        assert_eq!(act_of(&safe, false, false, None), Act::Add(false));
        assert_eq!(act_of(&safe, true, false, None), Act::Unplan);
        let active = w("/r/b", WtState::Active);
        assert_eq!(act_of(&active, false, false, Some("s".into())), Act::Go("s".into()));
        assert_eq!(act_of(&active, false, false, None), Act::CopyActive);
        let mut ahead = w("/r/c", WtState::Check);
        ahead.unpushed = 2;
        assert_eq!(act_of(&ahead, false, false, None), Act::Push);
        let mut dirty = w("/r/d", WtState::Check);
        dirty.dirty = 1;
        assert_eq!(act_of(&dirty, false, false, None), Act::Plain(true));
    }
}
