//! Port of `Components/Session/Thread.razor` — spec 07 §33, 02 §7. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::common::markdown::Markdown;
use crate::interop;
use crate::ipc;
use crate::session::api_error_card::ApiErrorCard;
use crate::session::crash_banner::CrashBanner;
use crate::session::file_card::FileCard;
use crate::session::hook_activity::HookActivity;
use crate::session::ledger::Ledger;
use crate::session::rewind_dialog::RewindDialog;
use crate::session::status_line::StatusLine;
use crate::session::streaming_tail::StreamingTail;
use crate::session::workflow::{ChildrenMap, Workflow};
use crate::session::workflow_card::WorkflowCard;
use crate::session::Cwd;
use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::thread_blocks::{self, Block};
use ccui_shared::{fmt, i18n, session_rules, t, tool_kinds, workflow};
use chrono::NaiveDate;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use std::collections::{HashMap, HashSet};

/// One rendered block: its key (stable while the block lives: a run keeps its first tool's id while it grows), the
/// block, and the day divider shown above it (part of the key: a divider appearing remounts that one block).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub block: Block,
    pub divider: Option<NaiveDate>,
}

/// The thread's blocks with their keys and the index of each key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blocks {
    pub list: Vec<Entry>,
    pub at: HashMap<String, usize>,
    /// Key of the latest non-agent run (the Ledger shown open).
    pub last_run: Option<String>,
}

fn kind(item: &Item) -> (&'static str, &str) {
    match item {
        Item::User(u) => ("u", u.uuid.as_deref().unwrap_or("")),
        Item::Text(t) => ("t", t.uuid.as_deref().unwrap_or("")),
        Item::Thought(_) => ("th", ""),
        Item::Compact(_) => ("c", ""),
        Item::Fallback(_) => ("f", ""),
        Item::ApiError(_) => ("e", ""),
        Item::Reset => ("x", ""),
        Item::Resumed => ("r", ""),
        Item::Hook(_) => ("h", ""),
        Item::Notice(_) => ("n", ""),
        Item::Scheduled(_) => ("s", ""),
        Item::Tool(t) => ("tool", &t.id),
    }
}

/// `ThreadBlocks.Of` (hidden thoughts never split a run) + keys + day dividers (`stamps`): a divider when the local day
/// changes, but none above a thread that starts today.
pub fn blocks(items: &[&Item], hide_thoughts: bool, stamps: bool, day_of: impl Fn(Time) -> NaiveDate, today: NaiveDate) -> Blocks {
    let mut out = Blocks::default();
    let mut day: Option<NaiveDate> = None;
    for block in thread_blocks::of(items, hide_thoughts) {
        let (mut key, dated) = match &block {
            Block::Run { tools, .. } => {
                let first = tools.first().copied().unwrap_or_default();
                (format!("run:{}", items.get(first).and_then(|i| i.as_tool()).map_or("", |t| t.id.as_str())), None)
            }
            Block::Hooks(ix) => (format!("hooks:{}", ix.first().copied().unwrap_or_default()), None),
            Block::Item(i) => {
                let item = items[*i];
                let (k, id) = kind(item);
                let dated = match item {
                    Item::User(u) => Some(u.at),
                    Item::Text(t) => t.at,
                    _ => None,
                };
                (format!("{i}:{k}:{id}"), dated)
            }
        };
        let mut divider = None;
        if let Some(d) = dated.filter(|_| stamps).map(&day_of) {
            if Some(d) != day {
                if day.is_some() || d != today {
                    divider = Some(d);
                    key.push_str(&format!("|{d}"));
                }
                day = Some(d);
            }
        }
        if matches!(block, Block::Run { agents: false, .. }) {
            out.last_run = Some(key.clone());
        }
        out.at.insert(key.clone(), out.list.len());
        out.list.push(Entry { key, block, divider });
    }
    out
}

/// The start of a reply, so each "Fork from here" button has a distinct accessible name: markdown marks dropped,
/// whitespace collapsed, 60 chars + "…".
pub fn gist(md: &str) -> String {
    let s = md.chars().filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '>')).collect::<String>();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 60 {
        format!("{}…", s.chars().take(60).collect::<String>())
    } else {
        s
    }
}

fn local_day(t: Time) -> NaiveDate {
    t.with_timezone(&chrono::Local).date_naive()
}

/// What every block of one Thread shares (Copy).
#[derive(Clone, Copy)]
struct Ctx {
    session: SessionHandle,
    stamps: bool,
    thoughts: bool,
    marked: RwSignal<HashSet<String>>,
    /// (sent uuids, has_process && Idle && no background agent running): `CanRewind` without the items per block.
    rewind_base: Memo<(Vec<String>, bool)>,
    can_fork: Memo<bool>,
    rewind: RwSignal<Option<UserItem>>,
    nav: StoredValue<Box<dyn Fn(&str)>, LocalStorage>,
}

impl Ctx {
    fn can_rewind(&self, uuid: Option<&str>) -> bool {
        self.rewind_base.with(|(sent, ok)| *ok && uuid.is_some_and(|u| sent.iter().any(|s| s == u)))
    }

    fn mark(&self, uuid: String, md: String) {
        let (id, marked) = (self.session.id(), self.marked);
        leptos::task::spawn_local(async move {
            match ipc::bookmark_toggle(&id, &uuid, &gist(&md)).await {
                Ok(on) => {
                    let _ = marked.try_update(|m| {
                        if on {
                            m.insert(uuid);
                        } else {
                            m.remove(&uuid);
                        }
                    });
                }
                Err(e) => leptos::logging::error!("[{id}] bookmark: {e}"),
            }
        });
    }

    fn fork(&self, at: String) {
        let (id, nav) = (self.session.id(), self.nav);
        leptos::task::spawn_local(async move {
            match ipc::session_fork(&id, Some(&at)).await {
                Ok(new) => {
                    let _ = nav.try_with_value(|n| n(&format!("/session/{new}")));
                }
                Err(e) => leptos::logging::error!("[{id}] fork: {e}"),
            }
        });
    }

    fn stamp(&self, at: Option<Time>) -> Option<impl IntoView> {
        at.filter(|_| self.stamps).map(|ta| {
            let iso = ta.with_timezone(&chrono::Local).to_rfc3339();
            view! { <time class="stamp" datetime=iso>{fmt::time(ta)}</time> }
        })
    }

    /// A block rendered from its item (everything but tools, runs and hooks).
    fn item(self, item: Item) -> AnyView {
        match item {
            Item::User(u) => self.user(u),
            Item::Text(t) => {
                let stamp = self.stamp(t.at);
                match t.uuid.clone() {
                    // Always rendered (disabled while a turn runs) so the thread does not shift at each turn's start/end.
                    Some(at) => {
                        let g = gist(&t.markdown);
                        let (md, md_copy, md_mark) = (t.markdown.clone(), t.markdown.clone(), t.markdown);
                        let (uuid_mark, uuid_fork, uuid_on) = (at.clone(), at.clone(), at.clone());
                        let marked = self.marked;
                        let can_fork = self.can_fork;
                        view! {
                            <div class="said" id=format!("m-{at}")>
                                {stamp}
                                <Markdown text=md/>
                                <span class="said-acts">
                                    <button type="button" class="btn ghost fork-here"
                                        aria-pressed=move || if marked.with(|m| m.contains(&uuid_on)) { "true" } else { "false" }
                                        aria-label=t!("Bookmark.Of", g)
                                        on:click=move |_| self.mark(uuid_mark.clone(), md_mark.clone())>{t!("Bookmark.Add")}</button>
                                    <button type="button" class="btn ghost fork-here" data-copy=md_copy aria-label=t!("Thread.CopyOf", g)>
                                        <Icon name="copy" sm=true/>{t!("Md.Copy")}</button>
                                    <button class="btn ghost fork-here" title=t!("Session.ForkHint") aria-label=t!("Session.ForkHereOf", g)
                                        disabled=move || !can_fork.get() on:click=move |_| self.fork(uuid_fork.clone())>
                                        <Icon name="branch" sm=true/>{t!("Session.ForkHere")}</button>
                                </span>
                            </div>
                        }
                        .into_any()
                    }
                    None => view! { {stamp}<Markdown text=t.markdown/> }.into_any(),
                }
            }
            Item::Thought(th) if self.thoughts => view! {
                <details class="fold thought"><summary><Icon name="right" sm=true class="chev"/>{t!("Thread.Thinking")}</summary><Markdown text=th.text/></details>
            }
            .into_any(),
            Item::Compact(c) => {
                // The CLI's "Compacted chat": what was summarized away, and the summary the conversation goes on from.
                let small = [
                    i18n::name("Thread.Compact.", Some(&c.trigger)),
                    if c.pre_tokens > 0 { t!("Thread.CompactTokens", fmt::tokens(c.pre_tokens), fmt::tokens(c.post_tokens)) } else { String::new() },
                    if c.duration_ms > 0 { fmt::dur(c.duration_ms as i64) } else { String::new() },
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
                let body = match c.summary {
                    Some(sum) => view! { <Markdown text=sum/> }.into_any(),
                    None => view! { <p class="prose">{t!("Thread.CompactNoSummary")}</p> }.into_any(),
                };
                view! {
                    <details class="fold compact"><summary><Icon name="right" sm=true class="chev"/><Icon name="compress" sm=true/>{t!("Thread.Compacted")}
                        " "<small>{small}</small></summary>{body}</details>
                }
                .into_any()
            }
            Item::Fallback(f) => {
                // The refused message back in the composer, conversation (and files) rewound to before it: the CLI's
                // "Edit prompt and retry".
                let session = self.session;
                let retry = f.retry.clone();
                let target = Memo::new(move |_| {
                    let r = retry.as_deref()?;
                    session.with_items(|it| it.iter().find_map(|i| i.as_user().filter(|u| u.uuid.as_deref() == Some(r)).cloned()))
                });
                let button = move || {
                    target.get().filter(|u| self.can_rewind(u.uuid.as_deref())).map(|u| {
                        view! {
                            <button type="button" class="btn ghost" aria-haspopup="dialog" on:click=move |_| self.rewind.set(Some(u.clone()))>
                                <Icon name="undo" sm=true/>{t!("Thread.EditRetry")}</button>
                        }
                    })
                };
                view! { <p class="notice warn">{f.text}{button}</p> }.into_any()
            }
            Item::ApiError(e) => view! { <ApiErrorCard error=e.error session=self.session/> }.into_any(),
            Item::Resumed => view! { <p class="notice">{t!("Thread.Resumed")}</p> }.into_any(),
            // The terminal's "Running scheduled task (time)" line, with the prompt the task ran.
            Item::Scheduled(sc) => view! {
                <p class="notice"><Icon name="refresh" sm=true/>" "{t!("Thread.Scheduled", fmt::time(sc.at))}" "<q>{sc.prompt}</q></p>
            }
            .into_any(),
            Item::Notice(n) => {
                let bad = matches!(n.notice.color.as_deref(), Some("error" | "warning"));
                view! { <p class=if bad { "notice bad" } else { "notice" }>{n.notice.text}</p> }.into_any()
            }
            _ => ().into_any(),
        }
    }

    fn user(self, u: UserItem) -> AnyView {
        let time = fmt::time(u.at);
        let uuid = u.uuid.clone();
        let can = Memo::new(move |_| self.can_rewind(uuid.as_deref()));
        let target = u.clone();
        let rewind_btn = move || {
            can.get().then(|| {
                let target = target.clone();
                view! {
                    <button type="button" class="rw-btn" aria-haspopup="dialog" aria-label=t!("Rewind.ActionLabel", fmt::time(target.at))
                        on:click=move |_| self.rewind.set(Some(target.clone()))><Icon name="undo" sm=true/>{t!("Rewind.Action")}</button>
                }
            })
        };
        let images = u.images.filter(|i| !i.is_empty()).map(|imgs| {
            let rows = imgs
                .into_iter()
                .enumerate()
                .map(|(i, img)| {
                    let n = i + 1;
                    let inner = if img.document() {
                        let title = img.name.clone();
                        let name = img.name.unwrap_or_else(|| t!("Composer.Document", n));
                        view! { <span class="miss" title=title><Icon name="paperclip" sm=true/>{name}</span> }.into_any()
                    } else if let Some(url) = img.url {
                        view! { <img src=url alt=t!("Composer.ImageAlt", n) loading="lazy"/> }.into_any()
                    } else {
                        view! { <span class="miss"><Icon name="paperclip" sm=true/>{t!("Thread.ImageMissing", n)}</span> }.into_any()
                    };
                    view! { <li>{inner}</li> }
                })
                .collect_view();
            view! { <ul class="thumbs" aria-label=t!("Composer.Images")>{rows}</ul> }
        });
        view! {
            <div class="you">
                <div class="w">{t!("Thread.You", time)}{rewind_btn}</div>
                {u.ultracode.then(|| view! { <span class="ultra-chip"><Icon name="agents" sm=true/>"ultracode"</span> })}
                <span class="tx">{u.text}</span>
                {images}
            </div>
        }
        .into_any()
    }
}

/// The log: `session.with_items(|it| thread_blocks::of(it, hide_thoughts))` memo (index blocks; rows read
/// `session.tool_at(i)` / `row(i)`), timestamps / day dividers (`fmt::long_date`, flag),
/// bookmarks, fork from here, rewind; StreamingTail; CrashBanner; dock (jump + StatusLine); RewindDialog.
/// Keyed per block: a patch re-groups the indexes (cheap) and only new blocks mount; rows update in place.
#[component]
pub fn Thread(session: SessionHandle) -> impl IntoView {
    let app = use_app();
    let ui = use_ui();
    // Appearance toggles, read once per mount.
    let (stamps, thoughts) = app.flags.with_untracked(|f| (f.timestamps, f.thinking));
    provide_context(ChildrenMap::new(session));
    provide_context(Cwd(Memo::new(move |_| session.meta.with(|m| m.cwd.clone()))));
    let navigate = use_navigate();
    let nav: StoredValue<Box<dyn Fn(&str)>, LocalStorage> =
        StoredValue::new_local(Box::new(move |p: &str| navigate(p, NavigateOptions::default())));

    let view = Memo::new(move |_| {
        let today = chrono::Local::now().date_naive();
        session.with_items(|it| blocks(it, !thoughts, stamps, local_day, today))
    });
    let bg_running = Memo::new(move |_| session.with_items(|it| session_rules::background_running(it)));
    let rewind_base = Memo::new(move |_| {
        let bg = bg_running.get();
        session.meta.with(|m| (m.sent.clone(), m.has_process && m.status == SessionStatus::Idle && !bg))
    });
    let can_fork = Memo::new(move |_| session.meta.with(|m| m.can_fork));
    let marked = RwSignal::new(HashSet::<String>::new());
    let rewind = RwSignal::new(None::<UserItem>);
    let ctx = Ctx { session, stamps, thoughts, marked, rewind_base, can_fork, rewind, nav };

    // Bookmarked replies of this session (the context panel lists them); re-read when they change elsewhere.
    let load_marks = move || {
        let id = session.id();
        leptos::task::spawn_local(async move {
            match ipc::bookmarks_of(&id).await {
                Ok(b) => {
                    let _ = marked.try_set(b.into_iter().map(|b| b.uuid).collect());
                }
                Err(e) => leptos::logging::error!("[{id}] bookmarks: {e}"),
            }
        });
    };
    load_marks();
    let seen_marks = StoredValue::new(app.bookmarks.get_untracked().map_or(0, |(n, _)| n));
    Effect::new(move |_| {
        let Some((n, sid)) = app.bookmarks.get() else { return };
        if n <= seen_marks.get_value() {
            return;
        }
        seen_marks.set_value(n);
        if sid == session.id() {
            load_marks();
        }
    });
    // A typed /rewind (Composer) opens the dialog on that message.
    let seen_rewind = StoredValue::new(ui.rewind.get_untracked().map_or(0, |(n, _)| n));
    Effect::new(move |_| {
        let Some((n, u)) = ui.rewind.get() else { return };
        if n <= seen_rewind.get_value() {
            return;
        }
        seen_rewind.set_value(n);
        rewind.set(Some(u));
    });

    // Keep the thread pinned to the bottom while it grows, unless the user scrolled up (StreamingTail pins it while
    // text streams); here after each change of the session, not after a selection. A new prompt brings it back down.
    let el = NodeRef::<leptos::html::Div>::new();
    let prompts = StoredValue::new(0usize);
    Effect::new(move |_| {
        session.version.track();
        let Some(div) = el.get() else { return };
        let n = session.items.with_untracked(|v| v.iter().filter(|r| r.with_untracked(|i| matches!(i, Item::User(_)))).count());
        let force = n > prompts.get_value();
        prompts.set_value(n);
        request_animation_frame(move || interop::scroll_end(&div, force));
    });

    let entry = move |e: Entry| {
        let divider = e.divider.map(|d| view! { <div class="day" role="separator">{fmt::long_date(d)}</div> });
        let key = StoredValue::new(e.key.clone());
        let block = match e.block {
            Block::Run { agents, .. } => {
                let tools = Memo::new(move |_| {
                    key.with_value(|k| {
                        view.with(|v| match v.at.get(k).map(|&i| &v.list[i].block) {
                            Some(Block::Run { tools, .. }) => tools.clone(),
                            _ => Vec::new(),
                        })
                    })
                });
                if agents {
                    view! { <Workflow session=session agents=tools/> }.into_any()
                } else {
                    let expanded = Signal::derive(move || key.with_value(|k| view.with(|v| v.last_run.as_ref() == Some(k))));
                    view! { <Ledger session=session tools=tools expanded=expanded/> }.into_any()
                }
            }
            Block::Hooks(_) => {
                let hooks = Memo::new(move |_| {
                    let ix = key.with_value(|k| {
                        view.with(|v| match v.at.get(k).map(|&i| &v.list[i].block) {
                            Some(Block::Hooks(ix)) => ix.clone(),
                            _ => Vec::new(),
                        })
                    });
                    ix.into_iter()
                        .filter_map(|i| session.row(i))
                        .filter_map(|r| r.with(|it| match it {
                            Item::Hook(h) => Some(h.clone()),
                            _ => None,
                        }))
                        .collect::<Vec<_>>()
                });
                view! { <HookActivity hooks=hooks/> }.into_any()
            }
            Block::Item(i) => {
                let row = session.items.with_untracked(|v| v.get(i).cloned());
                match row {
                    Some(row) => {
                        let tool_name = row.with_untracked(|it| it.as_tool().map(|t| t.name.clone()));
                        match tool_name {
                            Some(n) if n == tool_kinds::SEND_FILE => {
                                view! { <FileCard session=session tool=session.tool_at(i)/> }.into_any()
                            }
                            Some(n) if n == workflow::TOOL => {
                                view! { <WorkflowCard session=session tool=session.tool_at(i)/> }.into_any()
                            }
                            _ => (move || ctx.item(row.get())).into_any(),
                        }
                    }
                    None => ().into_any(),
                }
            }
        };
        view! { {divider}{block} }
    };

    view! {
        // role=log wraps the blocks only: finished blocks are announced as they are added; the token-by-token stream is
        // hidden until it lands as a TextItem. StatusLine and CrashBanner are their own live regions, outside the log.
        <div class="thread" node_ref=el>
            <div role="log" aria-live="polite" aria-relevant="additions">
                <For each=move || view.with(|v| v.list.clone()) key=|e| e.key.clone() children=entry/>
                <StreamingTail session=session/>
            </div>
            <CrashBanner session=session/>
        </div>
        // Outside the scroller: busy/idle stays visible while reading earlier output. interop shows .jump on new output
        // when scrolled up (the button's hidden attribute is the script's from then on).
        <div class="dock">
            <button type="button" class="btn jump" hidden=true><Icon name="down" sm=true/>{t!("Thread.JumpLatest")}</button>
            <StatusLine session=session/>
        </div>
        <RewindDialog session=session target=rewind/>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user(day: i64) -> Item {
        Item::User(UserItem { text: "q".into(), at: Time::default() + chrono::Duration::days(day), ultracode: false, images: None, uuid: Some(format!("u{day}")) })
    }

    fn tool(id: &str) -> Item {
        Item::Tool(ToolItem {
            id: id.into(),
            name: "Bash".into(),
            input: json!({}),
            parent_tool_use_id: None,
            state: ToolState::Done,
            result_text: None,
            structured: None,
            started_at: Time::default(),
            ended_at: None,
            task_id: None,
            task_status: None,
            progress: None,
            tokens: 0,
            sub_tool_uses: 0,
            background: false,
            model: None,
        })
    }

    #[test]
    fn keys_runs_and_days() {
        let items = [user(0), tool("a"), tool("b"), user(1), tool("c")];
        let refs: Vec<&Item> = items.iter().collect();
        let day = |t: Time| t.date_naive();
        let today = Time::default().date_naive();
        let b = blocks(&refs, true, true, day, today);
        let keys: Vec<&str> = b.list.iter().map(|e| e.key.as_str()).collect();
        let d1 = today + chrono::Duration::days(1);
        assert_eq!(keys, ["0:u:u0".to_string(), "run:a".into(), format!("3:u:u1|{d1}"), "run:c".into()]);
        assert_eq!(b.list[0].divider, None, "no divider above a thread that starts today");
        assert_eq!(b.list[2].divider, Some(d1));
        assert_eq!(b.last_run.as_deref(), Some("run:c"));
        assert_eq!(b.at["run:a"], 1);
        // Not today: the first block gets its divider; without stamps none.
        let b = blocks(&refs, true, true, day, today - chrono::Duration::days(5));
        assert_eq!(b.list[0].divider, Some(today));
        assert!(blocks(&refs, true, false, day, today).list.iter().all(|e| e.divider.is_none()));
    }

    #[test]
    fn gists() {
        assert_eq!(gist("## Hello **world**\n\n> quoted `code`"), "Hello world quoted code");
        let long = "a".repeat(70);
        assert_eq!(gist(&long), format!("{}…", "a".repeat(60)));
    }
}
