#![warn(unused, dead_code)]
//! Port of `Components/Pages/Extensions.razor` (page shell: session selector, tabs, query `tab` / `server` /
//! `session`, inspector) — spec 06 §6.7. Owner: ui-extensions. One file per tab beside it.
//! The selected live session is loaded with `AppStore::load_live` (never opens a transcript; `release` on change /
//! cleanup); the "Past" session is looked up with `ipc::transcript_by_id`; MCP sign-in uses
//! `ipc::session_mcp_authenticate`.
//!
//! The Razor page was one component: its state (busy, armed confirm, filters, the hooks / rules / plugins read for
//! the session shown…) is shared by every tab and the inspectors, so it lives in one `Ext` context here.

pub mod agents;
pub mod hooks;
pub mod mcp;
pub mod permissions;
pub mod plugins;
pub mod skills;

use crate::common::icon::Icon;
use crate::ipc;
use crate::state::{use_app, use_ui, AppStore, SessionHandle, UiState};
use ccui_shared::api::PastSession;
use ccui_shared::hooks::{HookListing, HookRow};
use ccui_shared::mcp::{tools as mcp_tools, ToolInfo};
use ccui_shared::model::InitInfo;
use ccui_shared::permission_rules::PermissionRulesState;
use ccui_shared::plugins::{ListedPlugin, Market, Outcome, PluginOption, PluginRow};
use ccui_shared::session::SessionHeavy;
use ccui_shared::{cli_edit, fmt, i18n, json, permission_rules, t};
use leptos::leptos_dom::helpers::{set_timeout_with_handle, TimeoutHandle};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_query_map};
use leptos_router::NavigateOptions;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use agents::AgentsTab;
use hooks::HooksTab;
use mcp::McpTab;
use permissions::PermissionsTab;
use plugins::PluginsTab;
use skills::SkillsTab;

/// Tabs, in display order (query `tab`; default mcp).
pub const TABS: [&str; 6] = ["mcp", "skills", "agents", "hooks", "permissions", "plugins"];

// ============================================================================================ pure helpers

/// One MCP server row (`Srv`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Srv {
    pub name: String,
    pub status: String,
    pub short: String,
    pub origin: String,
    pub tag: String,
    pub error: Option<String>,
    pub transport: Option<String>,
    pub target: Option<String>,
    pub tools: Option<Vec<ToolInfo>>,
    pub scope: Option<String>,
    pub version: Option<String>,
}

/// `Servers(s)`: mcp_status when available (error, transport, tools), else the init brief.
pub fn servers(heavy: &SessionHeavy) -> Vec<Srv> {
    if let Some(Value::Array(arr)) = heavy.mcp_status.as_ref().and_then(|st| json::prop(st, "mcpServers")) {
        return arr
            .iter()
            .map(|m| {
                let name = json::string(m, "name");
                let scope = json::str(m, "scope").map(str::to_string);
                let cfg = json::get(m, "config");
                let (short, origin, tag) = split_server(&name, scope.as_deref(), json::str(m, "source"));
                Srv {
                    status: json::string(m, "status"),
                    short,
                    origin,
                    tag,
                    error: json::str(m, "error").map(str::to_string),
                    transport: json::str(cfg, "type").map(str::to_string),
                    target: target(cfg),
                    tools: matches!(json::prop(m, "tools"), Some(Value::Array(_))).then(|| mcp_tools(m)),
                    scope,
                    version: json::prop(m, "serverInfo").and_then(|si| json::str(si, "version")).map(str::to_string),
                    name,
                }
            })
            .collect();
    }
    heavy
        .init
        .as_ref()
        .map(|i| {
            i.mcp
                .iter()
                .map(|m| {
                    let (short, origin, tag) = split_server(&m.name, None, None);
                    Srv { name: m.name.clone(), status: m.status.clone(), short, origin, tag, ..Srv::default() }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// url for http/sse, command line for stdio.
fn target(cfg: &Value) -> Option<String> {
    if let Some(url) = json::str(cfg, "url").filter(|u| !u.is_empty()) {
        return Some(url.to_string());
    }
    let cmd = json::str(cfg, "command")?;
    Some(match json::prop(cfg, "args") {
        Some(Value::Array(a)) => {
            let args: Vec<String> = a.iter().map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string)).collect();
            format!("{cmd} {}", args.join(" "))
        }
        _ => cmd.to_string(),
    })
}

/// `SignsInHere(m)`: transport != "claudeai-proxy" && tag != "claude.ai".
pub fn signs_in_here(transport: Option<&str>, tag: &str) -> bool {
    transport != Some("claudeai-proxy") && tag != "claude.ai"
}

/// `Split(name)`: `plugin:P:rest` → (rest, "plugin:P", "plugin"); `claude.ai X` → (X, "claude.ai", "claude.ai");
/// else (name, scope label, source tag).
pub fn split_server(name: &str, scope: Option<&str>, source: Option<&str>) -> (String, String, String) {
    if name.starts_with("plugin:") {
        let parts: Vec<&str> = name.splitn(3, ':').collect();
        if let [_, p, rest] = parts[..] {
            return (rest.to_string(), format!("plugin:{p}"), "plugin".to_string());
        }
    }
    if let Some(rest) = name.strip_prefix("claude.ai ") {
        return (rest.to_string(), "claude.ai".to_string(), "claude.ai".to_string());
    }
    let origin = match scope {
        Some("user") => t!("Ext.Scope.User"),
        Some("project") => t!("Ext.Scope.Project"),
        Some("local") => t!("Ext.Scope.Local"),
        _ => scope.or(source).unwrap_or_default().to_string(),
    };
    let tag = match source {
        None | Some("user") => t!("Ext.Scope.User"),
        Some("claudeai") => "claude.ai".to_string(),
        Some(s) => s.to_string(),
    };
    (name.to_string(), origin, tag)
}

/// `Agents(s)`: initialize `agents[] {name, description, model}`, else init.agents names.
pub fn agents(heavy: &SessionHeavy) -> Vec<(String, Option<String>, Option<String>)> {
    if let Some(Value::Array(a)) = heavy.initialize_info.as_ref().and_then(|i| json::prop(i, "agents")) {
        return a
            .iter()
            .map(|x| {
                (json::string(x, "name"), json::str(x, "description").map(str::to_string), json::str(x, "model").map(str::to_string))
            })
            .collect();
    }
    heavy.init.as_ref().map(|i| i.agents.iter().map(|n| (n.clone(), None, None)).collect()).unwrap_or_default()
}

/// `Describe(s, skill)`: the initialize `commands[]` entry's description.
pub fn describe(heavy: &SessionHeavy, skill: &str) -> Option<String> {
    match heavy.initialize_info.as_ref().and_then(|i| json::prop(i, "commands")) {
        Some(Value::Array(c)) => {
            c.iter().find(|x| json::str(x, "name") == Some(skill)).and_then(|x| json::str(x, "description")).map(str::to_string)
        }
        _ => None,
    }
}

/// `SplitSource(d)`: a plugin's "(commit-commands) …" → (rest, lead); "… (user)" → (text, user); else (d, None).
pub fn split_source(d: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(d) = d else { return (None, None) };
    // ^\(([^()]+)\)\s*(.*)$
    if let Some(rest) = d.strip_prefix('(') {
        if let Some(end) = rest.find(')') {
            let lead = &rest[..end];
            if !lead.is_empty() && !lead.contains('(') {
                return (Some(rest[end + 1..].trim_start().to_string()), Some(lead.to_string()));
            }
        }
    }
    // ^(.*?)\s*\(([\w.:@-]+)\)$
    if let Some(body) = d.strip_suffix(')') {
        if let Some(open) = body.rfind('(') {
            let inner = &body[open + 1..];
            let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '@' | '-');
            if !inner.is_empty() && inner.chars().all(word) {
                return (Some(body[..open].trim_end().to_string()), Some(inner.to_string()));
            }
        }
    }
    (Some(d.to_string()), None)
}

/// userSettings / projectSettings / localSettings / pluginHook…; anything else (policy…) as the CLI names it.
pub fn source_tag(source: &str) -> String {
    match source {
        "userSettings" => t!("Ext.Scope.User"),
        "projectSettings" => t!("Ext.Scope.Project"),
        "localSettings" => t!("Ext.Scope.Local"),
        "pluginHook" => t!("Ext.Scope.Plugin"),
        "session" => t!("Ext.Scope.Session"),
        "policySettings" => t!("Ext.Scope.Policy"),
        "cliArg" => t!("Ext.Scope.Cli"),
        _ => source.to_string(),
    }
}

/// A scope as the CLI writes it (user, project, local, dynamic, claudeai, managed…) in the page's words.
pub fn scope_label(scope: &str) -> String {
    match scope {
        "dynamic" => t!("Ext.Scope.Plugin"),
        "claudeai" => "claude.ai".to_string(),
        "user" | "project" | "local" | "policy" => source_tag(&format!("{scope}Settings")),
        _ => scope.to_string(),
    }
}

pub fn slash(path: &str) -> String {
    path.replace('\\', "/")
}

/// A long command or matcher wraps after its separators, not inside a word (HTML).
pub fn breaks(s: &str, c: char) -> String {
    s.split(c).map(fmt::enc).collect::<Vec<_>>().join(&format!("{}<wbr>", fmt::enc(&c.to_string())))
}

/// A hook told apart from its siblings: SessionStart (startup), PreToolUse (Bash)…
pub fn hook_name(h: &HookRow) -> String {
    if h.matcher.is_empty() { h.event.clone() } else { format!("{} ({})", h.event, h.matcher) }
}

pub fn hook_key(h: &HookRow) -> String {
    format!("{}|{}|{}|{}", h.event, h.source, h.matcher, h.command)
}

/// The listing's signature: a write is seen once it changed.
pub fn hook_rows_sig(l: Option<&HookListing>) -> String {
    l.map(|l| {
        l.rows
            .iter()
            .map(|r| format!("{}|{}|{}|{}|{:?}|{}", r.event, r.source, r.matcher, r.command, r.timeout, r.blocks_on_failure()))
            .collect::<Vec<_>>()
            .join("\n")
    })
    .unwrap_or_default()
}

/// The Skills / Agents / Hooks filter: case-insensitive in the name or the description.
pub fn matches(query: &str, name: &str, desc: Option<&str>) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || name.to_lowercase().contains(&q) || desc.is_some_and(|d| d.to_lowercase().contains(&q))
}

/// The row that holds a table's one Tab stop: the one the keys last reached, while it is shown, else the first.
pub fn stop_of(cursor: Option<&str>, keys: &[String]) -> Option<String> {
    match cursor {
        Some(c) if keys.iter().any(|k| k == c) => Some(c.to_string()),
        _ => keys.first().cloned(),
    }
}

/// `extensions?tab={tab}[&session={id}]`.
pub fn link(tab: &str, session: Option<&str>) -> String {
    match session.filter(|s| !s.is_empty()) {
        Some(s) => format!("extensions?tab={tab}&session={}", escape(s)),
        None => format!("extensions?tab={tab}"),
    }
}

/// `Uri.EscapeDataString`: unreserved ASCII kept, every other byte percent-encoded.
pub fn escape(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Where a failed server is defined, so its error has a place to be fixed.
pub fn failed_where(m: &Srv, installed: Option<&[PluginRow]>) -> String {
    if let Some(p) = m.origin.strip_prefix("plugin:") {
        let opts = installed.is_some_and(|rows| rows.iter().any(|r| r.name() == p && r.has_options));
        return t!(if opts { "Ext.FailedWhere.PluginOptions" } else { "Ext.FailedWhere.Plugin" }, p);
    }
    if m.tag == "claude.ai" {
        return t!("Ext.FailedWhere.Claude");
    }
    t!(match m.scope.as_deref() {
        Some("user") => "Ext.FailedWhere.User",
        Some("project") => "Ext.FailedWhere.Project",
        Some("local") => "Ext.FailedWhere.Local",
        _ => "Ext.FailedWhere.Other",
    })
}

/// How a plugin stands in this session (`PluginState`). The server ones link to that server on the MCP tab.
#[derive(Clone, Debug, PartialEq)]
pub enum PState {
    Off,
    Failed,
    ServerFailing(usize, Srv),
    NeedsAuth(Srv),
    ServersOff(usize, Srv),
    Loaded,
    Reload,
    On,
}

/// Off, failed to load, a server of it failing or waiting for sign-in, loaded, or enabled after the session
/// started (reload). Before the first message the session has not said what it loaded. `live` = a session is shown.
pub fn plugin_state(
    p: &PluginRow,
    init: Option<&InitInfo>,
    servers: Option<&[Srv]>,
    skills: Option<&[String]>,
    hooks: Option<&HookListing>,
    live: bool,
) -> PState {
    let name = p.name();
    let origin = format!("plugin:{name}");
    let mine: Vec<&Srv> = servers.unwrap_or_default().iter().filter(|x| x.origin == origin).collect();
    if !p.enabled {
        return PState::Off;
    }
    let init_err = init.and_then(|i| i.plugin_errors.as_ref()).is_some_and(|e| e.iter().any(|e| e.plugin == name));
    if !p.errors.is_empty() || init_err {
        return PState::Failed;
    }
    let failed: Vec<&&Srv> = mine.iter().filter(|x| x.status == "failed").collect();
    if let Some(f) = failed.first() {
        return PState::ServerFailing(failed.len(), (**f).clone());
    }
    if let Some(na) = mine.iter().find(|x| x.status == "needs-auth") {
        return PState::NeedsAuth((*na).clone());
    }
    if !mine.is_empty() && mine.iter().all(|x| x.status == "disabled") {
        return PState::ServersOff(mine.len(), mine[0].clone());
    }
    if let Some(i) = init {
        return if i.plugins.iter().any(|x| x == name) { PState::Loaded } else { PState::Reload };
    }
    let prefix = format!("{name}:");
    let from_hook = live && hooks.is_some_and(|h| h.rows.iter().any(|r| r.plugin_name.as_deref().is_some_and(|x| x == p.id || x == name)));
    if mine.iter().any(|x| x.status == "connected") || skills.is_some_and(|k| k.iter().any(|k| k.starts_with(&prefix))) || from_hook {
        return PState::Loaded;
    }
    PState::On
}

/// What the MCP row's action cell holds (kept as a kind so a toggle survives connected ↔ disabled).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowAct {
    Retry,
    SignIn,
    Toggle,
    None,
}

pub fn row_act(m: &Srv) -> RowAct {
    match m.status.as_str() {
        "failed" => RowAct::Retry,
        "needs-auth" if signs_in_here(m.transport.as_deref(), &m.tag) => RowAct::SignIn,
        "connected" | "disabled" => RowAct::Toggle,
        _ => RowAct::None,
    }
}

// ============================================================================================ shared fragments

/// `State(status)`: `<span class="ms …"><i></i>{label}</span>`.
pub fn state_view(status: &str) -> AnyView {
    let (cls, text) = match status {
        "connected" => ("ok", t!("Ext.State.Connected")),
        "needs-auth" => ("auth", t!("Ext.State.NeedsAuth")),
        "failed" => ("fail", t!("Ext.State.Failed")),
        "disabled" => ("off", t!("Ext.State.Disabled")),
        "pending" => ("wait", t!("Ext.State.Pending")),
        _ => ("off", status.to_string()),
    };
    dot(cls, text)
}

pub fn dot(cls: &str, text: String) -> AnyView {
    view! { <span class=format!("ms {cls}")><i></i>{text}</span> }.into_any()
}

/// `Filter(key, label, n)`.
pub fn filter_chip(ext: Ext, key: &'static str, label: String, n: Signal<usize>) -> impl IntoView {
    let on = move || ext.filter.with(|f| f == key);
    view! {
        <button class=move || if on() { "chip on" } else { "chip" } aria-pressed=move || if on() { "true" } else { "false" }
            on:click=move |_| ext.set_filter(key)>{label}" "<b>{move || n.get()}</b></button>
    }
}

/// `FindBox(shown, all)`.
pub fn find_box(ext: Ext, shown: Signal<usize>, all: Signal<usize>) -> impl IntoView {
    view! {
        <div class="findrow"><div class="input find"><Icon name="search" sm=true/><input type="search" aria-label=t!("Ext.Find")
            placeholder=t!("Ext.Find") prop:value=move || ext.query.get() on:input=move |e| ext.query.set(event_target_value(&e))
            spellcheck="false" autocomplete="off"/></div>
            <Show when=move || !ext.query.with(|q| q.trim().is_empty())>
                <span class="d" role="status">{move || t!("Ext.FindCount", shown.get(), all.get())}</span>
            </Show>
        </div>
    }
}

/// `CloseInspector`: the last child of `.ph` (interop's focus return selector).
fn close_inspector(ui: UiState) -> impl IntoView {
    view! {
        <button type="button" class="btn ghost" title=t!("Ext.CloseInspector") aria-label=t!("Action.Close")
            on:click=move |_| ui.inspector_open.set(false)><Icon name="x" sm=true/></button>
    }
}

/// `"true"` / `"false"` for aria-* attributes.
pub fn tf(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// Roving tab stop.
pub fn ti(b: bool) -> &'static str {
    if b { "0" } else { "-1" }
}

// ============================================================================================ page state

/// The page's state, shared by every tab and the inspectors (context).
#[derive(Clone, Copy)]
pub struct Ext {
    pub app: AppStore,
    pub ui: UiState,
    /// The session shown (`Live`), loaded with `load_live`.
    pub live: RwSignal<Option<SessionHandle>>,
    pub session_q: Memo<Option<String>>,
    pub home: StoredValue<String>,
    nav: StoredValue<Rc<dyn Fn(&str, bool)>, LocalStorage>,
    pub tab: RwSignal<String>,
    pub filter: RwSignal<String>,
    pub selected: RwSignal<Option<String>>,
    pub note: RwSignal<Option<String>>,
    pub busy: RwSignal<bool>,
    /// The Skills / Agents / Hooks filter.
    pub query: RwSignal<String>,
    /// Skill (k:) and agent (a:) rows showing their whole description.
    pub expanded: RwSignal<HashSet<String>>,
    /// The focused row of the tab's table.
    pub cursor: RwSignal<Option<String>>,
    /// The remove / uninstall awaiting its second click.
    pub confirm: RwSignal<Option<String>>,
    disarm: StoredValue<Option<TimeoutHandle>>,
    refreshed_for: StoredValue<Option<String>>,
    /// The shown session was read with its process (mcp_status, hooks, rules).
    read_live: StoredValue<bool>,
    autoselect: StoredValue<bool>,
    /// reload_skills, before the first message gives init.skills.
    pub skills_read: RwSignal<Option<Vec<String>>>,
    pub hooks: RwSignal<Option<HookListing>>,
    pub hooks_note: RwSignal<Option<String>>,
    /// The row the form replaces; None = the form adds.
    pub hook_editing: RwSignal<Option<HookRow>>,
    pub hook_event: RwSignal<String>,
    pub hook_matcher: RwSignal<String>,
    pub hook_command: RwSignal<String>,
    pub hook_file: RwSignal<String>,
    pub hook_timeout: RwSignal<String>,
    pub hook_block: RwSignal<bool>,
    pub perms: RwSignal<Option<PermissionRulesState>>,
    pub perms_note: RwSignal<Option<String>>,
    pub rule_draft: RwSignal<String>,
    pub rule_behavior: RwSignal<String>,
    pub rule_file: RwSignal<String>,
    /// Sign-in in progress: its page stays one click away until the server connects.
    pub auth: RwSignal<Option<(String, String)>>,
    auth_gen: StoredValue<u64>,
    focus_auth: StoredValue<bool>,
    pub installed: RwSignal<Option<Vec<PluginRow>>>,
    pub available: RwSignal<Vec<ListedPlugin>>,
    pub markets: RwSignal<Option<Vec<Market>>>,
    pub plugins_note: RwSignal<Option<String>>,
    pub plugins_done: RwSignal<Option<String>>,
    pub install_draft: RwSignal<String>,
    pub install_scope: RwSignal<String>,
    pub market_draft: RwSignal<String>,
    /// An install whose marketplace command waits for a person.
    pub to_accept: RwSignal<Option<(Outcome, Vec<String>)>>,
    pub configuring: RwSignal<Option<String>>,
    pub options: RwSignal<Option<Vec<PluginOption>>>,
    pub option_draft: RwSignal<Map<String, Value>>,
}

pub fn use_ext() -> Ext {
    expect_context::<Ext>()
}

fn sleep(ms: u32) -> gloo_timers::future::TimeoutFuture {
    gloo_timers::future::TimeoutFuture::new(ms)
}

impl Ext {
    fn alive(&self) -> bool {
        !self.tab.is_disposed()
    }

    fn navigate(&self, path: &str, replace: bool) {
        if let Some(f) = self.nav.try_get_value() {
            f(path, replace);
        }
    }

    /// Has the session a claude process (the summary says; reactive).
    pub fn has_process(&self, id: &str) -> bool {
        self.app.summary.with(|s| s.sessions.iter().any(|x| x.id == id && x.has_process))
    }

    fn has_process_now(&self, id: &str) -> bool {
        self.app.summary.with_untracked(|s| s.sessions.iter().any(|x| x.id == id && x.has_process))
    }

    /// The session's folder (summary first: meta comes with the snapshot).
    pub fn cwd(&self, h: SessionHandle) -> String {
        let id = h.id();
        self.app
            .summary
            .with(|s| s.sessions.iter().find(|x| x.id == id).map(|x| x.cwd.clone()))
            .unwrap_or_else(|| h.meta.with(|m| m.cwd.clone()))
    }

    /// `s.Init?.Skills ?? skillsRead`.
    pub fn skills(&self, h: SessionHandle) -> Option<Vec<String>> {
        h.heavy.with(|hv| hv.init.as_ref().map(|i| i.skills.clone())).or_else(|| self.skills_read.get())
    }

    // ---------- armed confirms

    /// An armed Remove/Uninstall disarms after 5 s too: WebKit never focuses a clicked button, so its blur never comes.
    pub fn arm(self, key: String) {
        self.confirm.set(Some(key.clone()));
        if let Some(h) = self.disarm.try_update_value(Option::take).flatten() {
            h.clear();
        }
        let ext = self;
        if let Ok(h) = set_timeout_with_handle(
            move || {
                if ext.alive() && ext.confirm.get_untracked().as_deref() == Some(key.as_str()) {
                    ext.confirm.set(None);
                }
            },
            Duration::from_secs(5),
        ) {
            self.disarm.set_value(Some(h));
        }
    }

    /// First click arms, the second (same key) runs.
    pub fn armed(self, key: String, act: impl FnOnce()) {
        if self.confirm.get_untracked().as_deref() != Some(key.as_str()) {
            self.arm(key);
            return;
        }
        self.confirm.set(None);
        act();
    }

    pub fn disarm(self) {
        self.confirm.set(None);
    }

    pub fn expand(self, key: String) {
        self.expanded.update(|e| {
            if !e.remove(&key) {
                e.insert(key);
            }
        });
    }

    // ---------- navigation, tabs, filters

    pub fn select(self, name: String) {
        self.selected.set(Some(name));
        self.ui.inspector_open.set(true);
    }

    pub fn show(self, key: &str) {
        self.tab.set(key.to_string());
        self.filter.set(String::new()); // the filters are per tab: an MCP state means nothing to the rules
        self.confirm.set(None);
        self.query.set(String::new());
        self.cursor.set(None);
        self.expanded.update(HashSet::clear);
        self.navigate(&link(key, self.session_q.get_untracked().as_deref()), true); // reload and Back keep the tab
        let Some(h) = self.live.get_untracked() else { return };
        match key {
            "hooks" => spawn_local(self.load_hooks(h.id())),
            "permissions" => spawn_local(self.load_perms(h.id())),
            "plugins" => spawn_local(self.load_plugins(Some(h.id()), self.cwd_now(h))),
            _ => {}
        }
    }

    fn cwd_now(&self, h: SessionHandle) -> String {
        untrack(|| self.cwd(h))
    }

    /// An MCP filter that hides the selected server moves the selection (and its inspector) to the first one shown.
    pub fn set_filter(self, key: &str) {
        self.filter.set(key.to_string());
        if self.tab.get_untracked() != "mcp" {
            return;
        }
        let Some(h) = self.live.get_untracked() else { return };
        let shown: Vec<Srv> = h.heavy.with_untracked(servers).into_iter().filter(|x| key.is_empty() || x.status == key).collect();
        let sel = self.selected.get_untracked();
        if !shown.iter().any(|x| Some(&x.name) == sel.as_ref()) {
            self.selected.set(shown.first().map(|x| x.name.clone()));
        }
    }

    // ---------- reading the session

    /// After a refresh: the first failed server, else the first, when none is selected.
    fn try_autoselect(self, h: SessionHandle) {
        if !self.autoselect.get_value() {
            return;
        }
        if self.selected.get_untracked().is_some() {
            self.autoselect.set_value(false);
            return;
        }
        let all = h.heavy.with_untracked(servers);
        if let Some(s) = all.iter().find(|x| x.status == "failed").or(all.first()) {
            self.selected.set(Some(s.name.clone()));
            self.autoselect.set_value(false);
        }
    }

    /// `Refresh`: another session clears what the previous one showed; with a process, mcp_status, skills (before
    /// init), hooks, rules; always the plugins. Never starts a process unasked (Start does that).
    pub fn refresh(self, h: SessionHandle) {
        let id = h.id();
        if self.refreshed_for.get_value().as_deref() != Some(id.as_str()) {
            if self.refreshed_for.get_value().is_some() {
                self.selected.set(None); // another session: its own failed server is the one to show
            }
            self.clear_read();
        }
        self.refreshed_for.set_value(Some(id.clone()));
        let proc = self.has_process_now(&id);
        if proc {
            self.read_live.set_value(true);
        }
        let cwd = self.cwd_now(h);
        spawn_local(async move {
            if proc {
                let rid = id.clone();
                self.run(async move { ipc::session_refresh_mcp(&rid).await }).await;
                if !self.alive() {
                    return;
                }
                self.autoselect.set_value(true);
                self.try_autoselect(h);
                if h.heavy.with_untracked(|x| x.init.is_none()) {
                    if let Ok(v) = ipc::session_request(&id, "reload_skills", None).await {
                        if self.alive() && self.refreshed_for.get_value().as_deref() == Some(id.as_str()) {
                            self.skills_read.set(Some(json::names(&v, "skills")));
                        }
                    }
                }
                self.load_hooks(id.clone()).await;
                self.load_perms(id.clone()).await;
            }
            self.load_plugins(Some(id), cwd).await;
        });
    }

    /// The previous session's (another folder) must not show meanwhile, nor its hook stay the edit target.
    fn clear_read(self) {
        self.hooks.set(None);
        self.hooks_note.set(None);
        self.perms.set(None);
        self.perms_note.set(None);
        self.installed.set(None);
        self.markets.set(None);
        self.skills_read.set(None);
        self.read_live.set_value(false);
        self.reset_hook_form();
    }

    fn current(&self, id: &str) -> bool {
        self.alive() && self.refreshed_for.get_value().as_deref() == Some(id)
    }

    /// get_hooks_listing reads the settings files on every call: asked again whenever the tab is opened. A process
    /// that exited must not be respawned for it: the tab only reads.
    pub async fn load_hooks(self, id: String) {
        if !self.has_process_now(&id) {
            return;
        }
        let got = ipc::session_hooks(&id).await;
        if !self.current(&id) {
            return; // Live moved to another session meanwhile: its Refresh loads its own hooks
        }
        match got {
            Ok(Some(l)) => {
                self.hooks.set(Some(l));
                self.hooks_note.set(None);
            }
            Ok(None) => {}
            Err(e) => {
                self.hooks.set(None);
                self.hooks_note.set(Some(t!("Ext.Hooks.Failed", e.trim_end())));
            }
        }
    }

    /// list_permission_rules: rules from every settings file plus session grants and --allowedTools.
    pub async fn load_perms(self, id: String) {
        if !self.has_process_now(&id) {
            return;
        }
        let got = ipc::session_permission_rules(&id).await;
        if !self.current(&id) {
            return;
        }
        match got {
            Ok(Some(p)) => {
                self.perms.set(Some(p));
                self.perms_note.set(None);
            }
            Ok(None) => {}
            Err(e) => {
                self.perms.set(None);
                self.perms_note.set(Some(t!("Ext.Perm.Failed", e.trim_end())));
            }
        }
    }

    /// `claude plugin list` + marketplaces in the folder. `sid` = the session they are read for (None = home).
    pub async fn load_plugins(self, sid: Option<String>, cwd: String) {
        let got = async {
            let l = ipc::plugins_list(&cwd).await?;
            let m = ipc::plugins_markets(&cwd).await?;
            Ok::<_, String>((l, m))
        }
        .await;
        if !self.alive() {
            return;
        }
        match got {
            Ok((l, m)) => {
                if sid != self.refreshed_for.get_value() {
                    return;
                }
                if let Some(cid) = self.configuring.get_untracked() {
                    if !l.installed.iter().any(|r| r.id == cid) {
                        self.configuring.set(None); // uninstalled meanwhile
                        self.options.set(None);
                    }
                }
                self.installed.set(Some(l.installed));
                self.available.set(l.available);
                self.markets.set(Some(m));
            }
            Err(e) => self.plugins_note.set(Some(t!("Ext.Plugins.Failed", e.trim_end()))),
        }
    }

    // ---------- MCP actions

    /// `Run`: busy, note cleared; an error is the note.
    pub async fn run(self, f: impl Future<Output = Result<(), String>>) {
        self.busy.set(true);
        self.note.set(None);
        let r = f.await;
        if !self.alive() {
            return;
        }
        if let Err(e) = r {
            self.note.set(Some(t!("Ext.ActionFailed", e.trim_end())));
        }
        self.busy.set(false);
    }

    /// A session named by a link may have no process: the page asks before starting one (mcp_status spawns it).
    pub fn start(self, h: SessionHandle) {
        spawn_local(async move {
            let id = h.id();
            self.run(async move { ipc::session_refresh_mcp(&id).await }).await;
            if self.alive() {
                self.refreshed_for.set_value(None);
                self.refresh(h);
            }
        });
    }

    pub fn toggle(self, h: SessionHandle, name: String, enabled: bool) {
        spawn_local(self.run(async move {
            let id = h.id();
            ipc::session_request(&id, "mcp_toggle", Some(json!({"serverName": name, "enabled": enabled}))).await?;
            ipc::session_refresh_mcp(&id).await
        }));
    }

    /// A server that still fails answers with its error: shown as the note.
    pub fn reconnect(self, h: SessionHandle, name: String) {
        spawn_local(self.run(async move {
            let id = h.id();
            let r = ipc::session_request(&id, "mcp_reconnect", Some(json!({"serverName": name}))).await;
            let s = ipc::session_refresh_mcp(&id).await;
            r?;
            s
        }));
    }

    /// The CLI has no bulk reconnect ("/mcp reconnect all" is terminal-only): one mcp_reconnect per failed server,
    /// then one mcp_status. A server that still fails is named in the note, the others carry on.
    pub fn reconnect_all(self, h: SessionHandle, servers: Vec<Srv>) {
        spawn_local(self.run(async move {
            let id = h.id();
            let mut still = Vec::new();
            for m in servers.iter().filter(|x| x.status == "failed") {
                if ipc::session_request(&id, "mcp_reconnect", Some(json!({"serverName": m.name}))).await.is_err() {
                    still.push(m.short.clone());
                }
            }
            ipc::session_refresh_mcp(&id).await?;
            if !still.is_empty() && self.alive() {
                self.note.set(Some(t!("Ext.StillFailing", still.join(", "))));
            }
            Ok(())
        }));
    }

    /// mcp_clear_auth drops the token the CLI stored: the next call signs in again.
    pub fn sign_out(self, h: SessionHandle, name: String) {
        spawn_local(self.run(async move {
            let id = h.id();
            let r = ipc::session_request(&id, "mcp_clear_auth", Some(json!({"serverName": name}))).await;
            let s = ipc::session_refresh_mcp(&id).await;
            r?;
            s
        }));
    }

    /// mcp_authenticate answers at once with the sign-in page; the CLI then waits for the browser's redirect and
    /// reconnects the server itself. The page is opened by a link (target=_blank → the system browser), not
    /// window.open: after the round-trip there is no user gesture left.
    pub fn sign_in(self, h: SessionHandle, name: String) {
        spawn_local(self.run(async move {
            let gen = self.auth_gen.get_value() + 1;
            self.auth_gen.set_value(gen);
            self.auth.set(None);
            self.select(name.clone());
            let id = h.id();
            let start = ipc::session_mcp_authenticate(&id, &name).await?;
            if !self.alive() {
                return Ok(());
            }
            match start.url {
                None => ipc::session_refresh_mcp(&id).await, // the stored token was still valid
                Some(_) if !start.callback_expected => {
                    self.note.set(Some(t!("Ext.AuthTerminal", name))); // nothing would reconnect it: see SignsInHere
                    Ok(())
                }
                Some(url) => {
                    self.auth.set(Some((name.clone(), url)));
                    self.focus_auth.set_value(true);
                    spawn_local(self.await_connected(h, name, gen));
                    Ok(())
                }
            }
        }));
    }

    /// mcp_status every 2 s until the server leaves needs-auth/pending, 5 minutes at most. Stops once the session
    /// has no process: mcp_status would spawn a new claude, without the callback listener this sign-in waits on.
    async fn await_connected(self, h: SessionHandle, name: String, gen: u64) {
        let id = h.id();
        let live = move || self.alive() && self.auth_gen.get_value() == gen;
        for _ in 0..150 {
            sleep(2000).await;
            if !live() {
                return;
            }
            if !self.has_process_now(&id) {
                self.auth.set(None);
                return;
            }
            let r = ipc::session_refresh_mcp(&id).await;
            if !live() {
                return; // a newer sign-in took over while mcp_status was in flight
            }
            if let Err(e) = r {
                self.auth.set(None);
                self.note.set(Some(t!("Ext.ActionFailed", e.trim_end())));
                return;
            }
            // ponytail: reads the heavy the push already applied (≤ 50 ms behind this refresh): worst case one
            // more 2 s round.
            let st = h.heavy.with_untracked(servers).into_iter().find(|x| x.name == name).map(|x| x.status);
            if !matches!(st.as_deref(), Some("needs-auth" | "pending")) {
                self.auth.set(None);
                return;
            }
        }
        self.auth.set(None);
        self.note.set(Some(t!("Ext.AuthTimeout", name)));
    }

    // ---------- hooks

    pub fn edit_hook(self, h: HookRow) {
        let edit = h.edit.clone().unwrap_or_else(|| ccui_shared::hooks::HookEdit { matcher: h.matcher.clone(), config: Value::Null });
        self.hook_event.set(h.event.clone());
        self.hook_matcher.set(edit.matcher.clone());
        self.hook_command.set(json::str(&edit.config, "command").map_or_else(|| h.command.clone(), str::to_string));
        self.hook_timeout.set(h.timeout.map(|t| t.to_string()).unwrap_or_default());
        self.hook_block.set(h.blocks_on_failure());
        self.hook_editing.set(Some(h));
    }

    pub fn reset_hook_form(self) {
        self.hook_editing.set(None);
        self.hook_matcher.set(String::new());
        self.hook_command.set(String::new());
        self.hook_timeout.set(String::new());
        self.hook_block.set(false);
    }

    /// `claude edit-hook` writes the settings file (the CLI's own writer and checks); a replace or remove names the
    /// row's stored entry, so a hook changed on disk meanwhile is refused rather than overwritten. The listing
    /// follows once the session's file watcher reloads the settings: listed again until it changed.
    pub async fn save_hook(self, h: SessionHandle, target: Option<HookRow>, remove: bool) {
        let command = self.hook_command.get_untracked().trim().to_string();
        if self.busy.get_untracked() || !remove && command.is_empty() {
            return;
        }
        let target_edit = target.as_ref().and_then(|t| t.edit.clone().map(|e| (t.event.clone(), e)));
        if (remove || target.is_some()) && target_edit.is_none() {
            return;
        }
        self.busy.set(true);
        self.hooks_note.set(None);
        let event = self.hook_event.get_untracked();
        let no_matcher = self.hooks.with_untracked(|l| {
            l.as_ref().and_then(|l| l.catalog.iter().find(|x| x.name == event)).is_some_and(|x| !x.matcher)
        });
        let matcher = if no_matcher { String::new() } else { self.hook_matcher.get_untracked().trim().to_string() };
        let tref = target_edit.as_ref().map(|(ev, e)| (ev.as_str(), e));
        let edit = match (&target, remove) {
            (Some(t), true) => cli_edit::hook("remove", &t.source, &t.event, &tref.expect("checked").1.matcher, None, tref),
            _ => {
                let timeout = self.hook_timeout.get_untracked().trim().parse::<i32>().unwrap_or(0);
                let hook = cli_edit::command_hook(tref.map(|(_, e)| &e.config), &command, timeout, self.hook_block.get_untracked());
                let source = target.as_ref().map_or_else(|| self.hook_file.get_untracked(), |t| t.source.clone());
                cli_edit::hook(if target.is_none() { "add" } else { "replace" }, &source, &event, &matcher, Some(hook), tref)
            }
        };
        let before = self.hooks.with_untracked(|l| hook_rows_sig(l.as_ref()));
        let cwd = self.perms.with_untracked(|p| p.as_ref().and_then(|p| p.cwd.clone())).unwrap_or_else(|| self.cwd_now(h));
        let r = ipc::cli_edit_hook(&cwd, &edit).await;
        if !self.alive() {
            return;
        }
        let failed = match r {
            Ok(Some(err)) => Some(err),
            Err(e) => Some(e),
            Ok(None) => None,
        };
        if let Some(err) = failed {
            self.hooks_note.set(Some(t!("Ext.Hooks.EditFailed", err.trim_end())));
            self.busy.set(false);
            return;
        }
        if !remove || target == self.hook_editing.get_untracked() {
            self.reset_hook_form();
        }
        for _ in 0..20 {
            // 10 s at most
            sleep(500).await;
            self.load_hooks(h.id()).await;
            if !self.alive() {
                return;
            }
            if self.hooks.with_untracked(|l| l.is_none() || hook_rows_sig(l.as_ref()) != before) {
                break;
            }
        }
        self.busy.set(false);
    }

    // ---------- permission rules

    /// Writes the settings file the CLI reads for that source, then lists again until its file watcher shows it.
    pub async fn edit_rule(self, h: SessionHandle, source: String, behavior: String, rule: String, add: bool) {
        if self.busy.get_untracked() || rule.is_empty() {
            return;
        }
        if !permission_rules::valid(&rule) {
            self.perms_note.set(Some(t!("Ext.Perm.Invalid", rule)));
            return;
        }
        self.busy.set(true);
        self.perms_note.set(None);
        let cwd = self.perms.with_untracked(|p| p.as_ref().and_then(|p| p.cwd.clone())).unwrap_or_else(|| self.cwd_now(h));
        let r = ipc::permission_rule_edit(&cwd, &source, &behavior, &rule, add).await;
        if !self.alive() {
            return;
        }
        match r {
            Err(e) => self.perms_note.set(Some(t!("Ext.Perm.WriteFailed", e.trim_end()))),
            Ok(()) => {
                if add {
                    self.rule_draft.set(String::new());
                }
                for _ in 0..20 {
                    // 3 s on 2.1.296; 10 s at most
                    sleep(500).await;
                    self.load_perms(h.id()).await;
                    if !self.alive() {
                        return;
                    }
                    let done = self.perms.with_untracked(|p| match p {
                        None => true,
                        Some(p) => p.rules.iter().any(|r| r.source == source && r.behavior == behavior && r.rule == rule) == add,
                    });
                    if done {
                        break;
                    }
                }
            }
        }
        self.busy.set(false);
    }

    // ---------- plugins: `claude plugin … --json` in the folder, then reload_plugins

    async fn reload(self, id: &str) -> Result<String, String> {
        let r = ipc::session_reload_plugins(id).await?;
        let text = t!("Ext.Plugins.Reloaded", r.plugins, r.skills, r.agents);
        Ok(if r.errors > 0 { format!("{text} {}", t!("Ext.Plugins.ReloadErrors", r.errors)) } else { text })
    }

    pub fn reload_now(self, h: SessionHandle) {
        spawn_local(self.run(async move {
            let done = self.reload(&h.id()).await?;
            if self.alive() {
                self.plugins_done.set(Some(done));
            }
            Ok(())
        }));
    }

    /// The CLI writes the settings and caches; the running session only sees it after reload_plugins (a process
    /// that exited is not respawned for it: the next one reads the disk anyway).
    pub async fn plugin_act(self, s: Option<SessionHandle>, cwd: String, args: Vec<String>) {
        if self.busy.get_untracked() {
            return;
        }
        self.busy.set(true);
        self.plugins_note.set(None);
        self.plugins_done.set(None);
        self.confirm.set(None);
        self.to_accept.set(None);
        let r: Result<(), String> = async {
            let r = ipc::plugins_act(&cwd, &args).await?;
            if !self.alive() {
                return Ok(());
            }
            if r.ok {
                let mut done = r.message.clone();
                if let Some(h) = s.filter(|h| self.has_process_now(&h.id())) {
                    done = format!("{done} {}", self.reload(&h.id()).await?);
                }
                self.plugins_done.set(Some(done));
                if args.first().map(String::as_str) == Some("install") {
                    self.install_draft.set(String::new());
                }
            } else if r.command.is_some() && r.command_sha.is_some() {
                self.to_accept.set(Some((r, args.clone())));
            } else {
                self.plugins_note.set(Some(r.message));
            }
            Ok(())
        }
        .await;
        if !self.alive() {
            return;
        }
        if let Err(e) = r {
            self.plugins_note.set(Some(t!("Ext.Plugins.Failed", e.trim_end())));
        }
        self.busy.set(false);
        self.load_plugins(s.map(|h| h.id()), cwd).await;
    }

    /// The person read the command: the same install again, bound to that command's sha (a changed one is refused).
    pub fn accept(self, s: Option<SessionHandle>, cwd: String) {
        let Some((o, mut args)) = self.to_accept.get_untracked() else { return };
        args.push("--accept-command".into());
        args.push(o.command_sha.unwrap_or_default());
        spawn_local(self.plugin_act(s, cwd, args));
    }

    /// Uninstall and remove ask for a second click on the same button: the row and its data go.
    pub fn confirmed(self, s: Option<SessionHandle>, cwd: String, key: String, args: Vec<String>) {
        if self.confirm.get_untracked().as_deref() != Some(key.as_str()) {
            self.arm(key);
            return;
        }
        spawn_local(self.plugin_act(s, cwd, args));
    }

    pub fn configure(self, cwd: String, id: String) {
        self.configuring.set(Some(id.clone()));
        self.options.set(None);
        self.plugins_note.set(None);
        self.option_draft.set(Map::new());
        self.ui.inspector_open.set(true);
        spawn_local(async move {
            let r = ipc::plugins_options(&cwd, &id).await;
            if !self.alive() || self.configuring.get_untracked().as_deref() != Some(id.as_str()) {
                return;
            }
            match r {
                Ok(o) => self.options.set(Some(o)),
                Err(e) => {
                    self.configuring.set(None);
                    self.plugins_note.set(Some(t!("Ext.Plugins.Failed", e.trim_end())));
                }
            }
        });
    }

    /// Only what was edited is sent: a sensitive option left empty keeps its saved value.
    pub async fn save_options(self, s: Option<SessionHandle>, cwd: String, id: String) {
        self.busy.set(true);
        self.plugins_note.set(None);
        self.plugins_done.set(None);
        let draft = self.option_draft.get_untracked();
        let r: Result<(), String> = async {
            if let Some(refused) = ipc::plugins_save(&cwd, &id, &draft).await? {
                if self.alive() {
                    self.plugins_note.set(Some(refused));
                }
                return Ok(());
            }
            if !self.alive() {
                return Ok(());
            }
            self.option_draft.set(Map::new());
            let o = ipc::plugins_options(&cwd, &id).await?;
            if !self.alive() {
                return Ok(());
            }
            self.options.set(Some(o));
            let mut done = t!("Ext.Plugins.Saved");
            if let Some(h) = s.filter(|h| self.has_process_now(&h.id())) {
                done = format!("{done} {}", self.reload(&h.id()).await?);
            }
            if self.alive() {
                self.plugins_done.set(Some(done));
            }
            Ok(())
        }
        .await;
        if !self.alive() {
            return;
        }
        if let Err(e) = r {
            self.plugins_note.set(Some(t!("Ext.Plugins.Failed", e.trim_end())));
        }
        self.busy.set(false);
    }
}

fn tab_button(ext: Ext, key: &'static str, label: String, n: Signal<Option<usize>>) -> impl IntoView {
    let on = move || ext.tab.with(|t| t == key);
    view! {
        <button id=format!("ext-tab-{key}") role="tab" aria-selected=move || tf(on()) aria-controls="ext-panel" tabindex=move || ti(on())
            class=move || on().then_some("on") on:click=move |_| ext.show(key)>
            {label}" "<b>{move || n.get().map_or_else(|| "—".to_string(), |n| n.to_string())}</b></button>
    }
}

// ============================================================================================ the page

/// The page: picks the live session (`?session=` else the current one, URL replaced with it), shows one tab;
/// armed confirms auto-disarm after 5 s.
#[component]
pub fn Extensions() -> impl IntoView {
    let app = use_app();
    let ui = use_ui();
    let query = use_query_map();
    let navigate = use_navigate();
    crate::pages::new_session::set_title(&t!("Nav.Extensions"));

    let session_q = Memo::new(move |_| query.with(|q| q.get("session").filter(|s| !s.is_empty())));
    let first_tab = query.with_untracked(|q| q.get("tab")).filter(|t| TABS.contains(&t.as_str())).unwrap_or_else(|| "mcp".into());
    let nav: Rc<dyn Fn(&str, bool)> =
        Rc::new(move |path: &str, replace: bool| navigate(&format!("/{path}"), NavigateOptions { replace, ..Default::default() }));
    let ext = Ext {
        app,
        ui,
        live: RwSignal::new(None),
        session_q,
        home: StoredValue::new(app.boot.with_value(|b| b.home.clone())),
        nav: StoredValue::new_local(nav),
        tab: RwSignal::new(first_tab),
        filter: RwSignal::new(String::new()),
        selected: RwSignal::new(None),
        note: RwSignal::new(None),
        busy: RwSignal::new(false),
        query: RwSignal::new(String::new()),
        expanded: RwSignal::new(HashSet::new()),
        cursor: RwSignal::new(None),
        confirm: RwSignal::new(None),
        disarm: StoredValue::new(None),
        refreshed_for: StoredValue::new(None),
        read_live: StoredValue::new(false),
        autoselect: StoredValue::new(false),
        skills_read: RwSignal::new(None),
        hooks: RwSignal::new(None),
        hooks_note: RwSignal::new(None),
        hook_editing: RwSignal::new(None),
        hook_event: RwSignal::new("PreToolUse".into()),
        hook_matcher: RwSignal::new(String::new()),
        hook_command: RwSignal::new(String::new()),
        hook_file: RwSignal::new("localSettings".into()),
        hook_timeout: RwSignal::new(String::new()),
        hook_block: RwSignal::new(false),
        perms: RwSignal::new(None),
        perms_note: RwSignal::new(None),
        rule_draft: RwSignal::new(String::new()),
        rule_behavior: RwSignal::new("allow".into()),
        rule_file: RwSignal::new("localSettings".into()),
        auth: RwSignal::new(None),
        auth_gen: StoredValue::new(0),
        focus_auth: StoredValue::new(false),
        installed: RwSignal::new(None),
        available: RwSignal::new(Vec::new()),
        markets: RwSignal::new(None),
        plugins_note: RwSignal::new(None),
        plugins_done: RwSignal::new(None),
        install_draft: RwSignal::new(String::new()),
        install_scope: RwSignal::new("user".into()),
        market_draft: RwSignal::new(String::new()),
        to_accept: RwSignal::new(None),
        configuring: RwSignal::new(None),
        options: RwSignal::new(None),
        option_draft: RwSignal::new(Map::new()),
    };
    provide_context(ext);
    if let Some(server) = query.with_untracked(|q| q.get("server")).filter(|s| !s.is_empty()) {
        ext.select(server); // a failed MCP call's link in the thread
    }

    // Live: the session a link names (?session=), else the most recent one with a claude process (starting one
    // here would be a side effect).
    let live_id = Memo::new(move |_| {
        let q = session_q.get();
        app.summary.with(|s| match q {
            Some(id) => s.sessions.iter().find(|x| x.id == id).map(|x| x.id.clone()),
            None => s.sessions.iter().rev().find(|x| x.has_process).map(|x| x.id.clone()),
        })
    });
    let held = StoredValue::new(None::<String>);
    Effect::new(move |_| {
        let id = live_id.get();
        if held.get_value() == id {
            return;
        }
        if let Some(old) = held.get_value() {
            app.release(&old);
        }
        if let Some(id) = &id {
            app.load_live(id);
        }
        held.set_value(id.clone());
        ext.live.set(id.map(|id| app.session(&id)));
    });
    on_cleanup(move || {
        if let Some(id) = held.try_get_value().flatten() {
            app.release(&id);
        }
        ext.auth_gen.update_value(|g| *g += 1);
        if let Some(h) = ext.disarm.try_update_value(Option::take).flatten() {
            h.clear();
        }
    });

    // The past session a ?session= names when it is not open here: its title and folder still name the page.
    let past = RwSignal::new(None::<PastSession>);
    Effect::new(move |_| {
        let q = session_q.get().filter(|_| live_id.with(Option::is_none));
        past.set(None);
        if let Some(q) = q {
            spawn_local(async move {
                if let Ok(p) = ipc::transcript_by_id(&q).await {
                    if !past.is_disposed() && session_q.get_untracked().as_deref() == Some(q.as_str()) {
                        past.set(p);
                    }
                }
            });
        }
    });

    // OnParametersSet + OnChanged: no live session → the home folder's plugins once; no ?session= → the URL names
    // the session shown; another session, or one whose process started since → Refresh.
    let live_proc = Memo::new(move |_| live_id.with(|id| id.as_ref().is_some_and(|id| ext.has_process(id))));
    Effect::new(move |_| {
        let (id, q, proc) = (live_id.get(), session_q.get(), live_proc.get());
        match id {
            None => {
                if ext.refreshed_for.get_value().is_some() {
                    ext.refreshed_for.set_value(None);
                    ext.clear_read();
                }
                if ext.installed.get_untracked().is_none() {
                    spawn_local(ext.load_plugins(None, ext.home.get_value()));
                }
            }
            Some(id) if q.is_none() => ext.navigate(&link(&ext.tab.get_untracked(), Some(&id)), true),
            Some(id) => {
                if ext.refreshed_for.get_value().as_deref() != Some(id.as_str()) || proc && !ext.read_live.get_value() {
                    ext.refresh(app.session(&id));
                }
            }
        }
    });
    // mcp_status may land after Refresh asked for it: pick the server to show once it does.
    Effect::new(move |_| {
        if let Some(h) = ext.live.get() {
            h.heavy.track();
            ext.try_autoselect(h);
        }
    });

    // The inspectors, keyed on what they show: their fields follow reactively (the sign-in link keeps its focus).
    let mcp_key = Memo::new(move |_| {
        if !ui.inspector_open.get() || ext.tab.get() != "mcp" {
            return None;
        }
        let h = ext.live.get()?;
        let name = ext.selected.get()?;
        h.heavy.with(|hv| servers(hv).iter().any(|x| x.name == name)).then_some(name)
    });
    let options_key = Memo::new(move |_| {
        if !ui.inspector_open.get() || !(ext.tab.get() == "plugins" || ext.live.with(Option::is_none)) {
            return None;
        }
        ext.options.with(Option::is_some).then(|| ext.configuring.get()).flatten()
    });

    let header = move || {
        if let Some(h) = ext.live.get() {
            let id = h.id();
            let others: Vec<(String, String, String)> = app.summary.with(|s| {
                s.sessions.iter().filter(|x| x.has_process || x.id == id).map(|x| (x.id.clone(), x.name.clone(), x.cwd.clone())).collect()
            });
            let Some((_, name, cwd)) = others.iter().find(|x| x.0 == id).cloned() else { return ().into_any() };
            let home = ext.home.get_value();
            let inner = if others.len() > 1 {
                let opts = others
                    .into_iter()
                    .map(|(oid, oname, ocwd)| {
                        let sel = oid == id;
                        view! { <option value=oid.clone() selected=sel>{format!("{oname} · {}", fmt::home(&ocwd, &home))}</option> }
                    })
                    .collect_view();
                view! {
                    <select aria-label=t!("Ext.Session") prop:value=id.clone()
                        on:change=move |e| ext.navigate(&link(&ext.tab.get_untracked(), Some(&event_target_value(&e))), false)>{opts}</select>
                }
                .into_any()
            } else {
                view! { <a href=format!("session/{id}")>{name}</a>{format!(" · {}", fmt::home(&cwd, &home))} }.into_any()
            };
            view! { <span class="sub" title=slash(&cwd)>{inner}</span> }.into_any()
        } else if let Some(p) = past.get() {
            view! { <span class="sub" title=slash(&p.cwd)>{p.title.clone()}{format!(" · {}", fmt::home(&p.cwd, &ext.home.get_value()))}</span> }
                .into_any()
        } else {
            ().into_any()
        }
    };

    let no_live = move || {
        let q = session_q.get();
        let p = past.get();
        let lead = match (&q, &p) {
            (None, _) => t!("Ext.NoSession"),
            (Some(_), Some(p)) => t!("Ext.SessionGoneOf", p.title),
            (Some(_), None) => t!("Ext.SessionUnknown"),
        };
        let acts = if let (Some(q), Some(_)) = (&q, &p) {
            view! {
                <a class="btn primary" href=format!("session/{}", escape(q))>{t!("Ext.OpenSession")}</a>
                <a class="btn" href=""><Icon name="plus" sm=true/>{t!("Rail.NewSession")}</a>
            }
            .into_any()
        } else {
            let open: Vec<(String, String, String)> = app.summary.with(|s| {
                s.sessions.iter().filter(|x| x.has_process).map(|x| (x.id.clone(), x.name.clone(), x.cwd.clone())).collect()
            });
            if open.is_empty() {
                view! { <a class="btn primary" href=""><Icon name="plus" sm=true/>{t!("Rail.NewSession")}</a> }.into_any()
            } else {
                open.into_iter()
                    .map(|(id, name, cwd)| {
                        view! { <a class="btn" href=link(&ext.tab.get_untracked(), Some(&id)) title=slash(&cwd)>{t!("Ext.ShowFor", name)}</a> }
                    })
                    .collect_view()
                    .into_any()
            }
        };
        view! { <div><h2>{t!("Nav.Extensions")}</h2><p class="lead">{lead}</p><div class="acts">{acts}</div></div> }
    };

    let body = move || match ext.live.get() {
        None => view! {
            {no_live}
            <h3>{t!("Ext.Plugins.ForHome")}</h3>
            <p class="d hint">{t!("Ext.Plugins.NoSessionNote")}</p>
            <PluginsTab session=Signal::derive(|| None::<SessionHandle>) cwd=Signal::derive(move || ext.home.get_value())/>
        }
        .into_any(),
        Some(h) => live_body(ext, h).into_any(),
    };

    view! {
        <div class="pane">
            <div class="ph"><h1 class="ttl">{t!("Nav.Extensions")}</h1>{header}</div>
            <div class="ext">{body}</div>
        </div>
        {move || mcp_key.get().and_then(|name| Some(mcp_inspector(ext, ext.live.get_untracked()?, name)))}
        {move || options_key.get().map(|cid| options_inspector(ext, cid))}
    }
}

fn live_body(ext: Ext, h: SessionHandle) -> impl IntoView {
    let id = h.id();
    let proc = {
        let id = id.clone();
        Memo::new(move |_| ext.has_process(&id))
    };
    let srv_count = {
        let id = id.clone();
        Signal::derive(move || {
            let has_status = h.heavy.with(|x| x.mcp_status.is_some());
            (ext.has_process(&id) || has_status).then(|| h.heavy.with(servers).len())
        })
    };
    let skills_n = Signal::derive(move || ext.skills(h).map(|s| s.len()));
    let agents_n = Signal::derive(move || {
        h.heavy.with(|x| (x.initialize_info.is_some() || x.init.is_some()).then(|| agents(x).len()))
    });
    let plugins_n = Signal::derive(move || ext.installed.with(|i| i.as_ref().map(Vec::len)));
    let hooks_n = Signal::derive(move || ext.hooks.with(|l| l.as_ref().map(|l| l.rows.len())));
    let perms_n = Signal::derive(move || ext.perms.with(|p| p.as_ref().map(|p| p.rules.len())));
    let tab = Memo::new(move |_| ext.tab.get());
    let panel = move || {
        let t = tab.get();
        if !proc.get() && t != "plugins" {
            let exit = untrack(|| ext.app.summary.with(|s| s.sessions.iter().find(|x| x.id == h.id()).and_then(|x| x.exit_code)));
            return view! {
                <p class="lead" role="status">{exit.map_or_else(|| t!("Ext.NoProcess"), |c| t!("Ext.NoProcessExited", c))}</p>
                <div class="acts"><button class="btn primary" disabled=move || ext.busy.get() on:click=move |_| ext.start(h)>
                    <Icon name="zap" sm=true/>{t!("Ext.StartProcess")}</button></div>
            }
            .into_any();
        }
        match t.as_str() {
            "hooks" => view! { <HooksTab session=h/> }.into_any(),
            "permissions" => view! { <PermissionsTab session=h/> }.into_any(),
            "plugins" => view! { <PluginsTab session=Signal::derive(move || Some(h)) cwd=Signal::derive(move || ext.cwd(h))/> }.into_any(),
            "skills" => view! { <SkillsTab session=h/> }.into_any(),
            "agents" => view! { <AgentsTab session=h/> }.into_any(),
            _ => view! { <McpTab session=h/> }.into_any(),
        }
    };
    view! {
        <div><h2>{t!("Nav.Extensions")}</h2><p class="lead">{t!("Ext.Lead")}</p></div>
        <div class="subtabs" role="tablist" aria-label=t!("Nav.Extensions")>
            {tab_button(ext, "mcp", "MCP".into(), srv_count)}
            {tab_button(ext, "skills", t!("Ext.Tab.Skills"), skills_n)}
            {tab_button(ext, "agents", t!("Ext.Tab.Agents"), agents_n)}
            {tab_button(ext, "plugins", t!("Ext.Tab.Plugins"), plugins_n)}
            {tab_button(ext, "hooks", t!("Ext.Tab.Hooks"), hooks_n)}
            {tab_button(ext, "permissions", t!("Ext.Tab.Permissions"), perms_n)}
        </div>
        <div id="ext-panel" role="tabpanel" aria-labelledby=move || format!("ext-tab-{}", tab.get())>{panel}</div>
    }
}

/// The MCP inspector: the selected server (failure, sign-in, details, tools).
fn mcp_inspector(ext: Ext, h: SessionHandle, name: String) -> AnyView {
    {
        let sel = {
            let name = name.clone();
            Memo::new(move |prev: Option<&Srv>| {
                h.heavy.with(servers).into_iter().find(|x| x.name == name).or_else(|| prev.cloned()).unwrap_or_default()
            })
        };
        let status = Memo::new(move |_| sel.with(|s| s.status.clone()));
        let auth_link = NodeRef::<leptos::html::A>::new();
        Effect::new(move |_| {
            if let Some(a) = auth_link.get() {
                if ext.focus_auth.get_value() {
                    ext.focus_auth.set_value(false);
                    let _ = a.focus();
                }
            }
        });
        let n = name.clone();
        let busy = move || ext.busy.get();
        let turn_off = move || {
            let n = n.clone();
            view! {
                <button class="btn ghost" disabled=busy title=t!("Ext.EnableHereHint") on:click=move |_| ext.toggle(h, n.clone(), false)>
                    {t!("Ext.TurnOff")}</button>
            }
        };
        let n2 = name.clone();
        let state_section = move || {
            let n = n2.clone();
            match status.get().as_str() {
                "failed" => {
                    let n1 = n.clone();
                    Some(
                        view! {
                            <div class="section">
                                <h3>{t!("Ext.FailedTitle")}</h3>
                                {move || sel.with(|s| s.error.clone()).map(|e| view! { <div class="errbox">{e.trim_end().to_string()}</div> })}
                                <p>{t!("Ext.FailedText")}" "{move || ext.installed.with(|i| sel.with(|s| failed_where(s, i.as_deref())))}</p>
                                <div class="acts">
                                    <button class="btn primary" disabled=busy on:click=move |_| ext.reconnect(h, n1.clone())>
                                        <Icon name="refresh" sm=true/>{t!("Ext.Retry")}</button>
                                    {turn_off()}
                                </div>
                            </div>
                        }
                        .into_any(),
                    )
                }
                "needs-auth" => {
                    let here = sel.with_untracked(|s| signs_in_here(s.transport.as_deref(), &s.tag));
                    let n1 = n.clone();
                    let how = if !here {
                        view! { <p>{t!("Ext.AuthHow")}</p> }.into_any()
                    } else {
                        let n2 = n.clone();
                        view! {
                            {move || match ext.auth.get().filter(|(s, _)| *s == n1) {
                                Some((_, url)) => view! {
                                    <p role="status">{t!("Ext.AuthWaiting")}</p>
                                    <div class="acts"><a class="btn primary" href=url target="_blank" rel="noopener noreferrer" node_ref=auth_link>
                                        {t!("Ext.OpenSignIn")}</a></div>
                                }
                                .into_any(),
                                None => {
                                    let n = n2.clone();
                                    view! {
                                        <div class="acts"><button class="btn primary" disabled=busy on:click=move |_| ext.sign_in(h, n.clone())>
                                            <Icon name="plug" sm=true/>{t!("Ext.SignIn")}</button></div>
                                    }
                                    .into_any()
                                }
                            }}
                            <p>{t!("Ext.AuthHowAlt")}</p>
                        }
                        .into_any()
                    };
                    Some(
                        view! {
                            <div class="section">
                                <h3>{t!("Ext.AuthTitle")}</h3>
                                <p>{t!("Ext.AuthText")}</p>
                                {how}
                                <div class="acts"><code>"/mcp"</code><button type="button" class="btn" data-copy="/mcp"><Icon name="copy" sm=true/>{t!("Md.Copy")}</button></div>
                                <div class="acts">{turn_off()}</div>
                            </div>
                        }
                        .into_any(),
                    )
                }
                "disabled" => Some(view! { <div class="section"><p>{t!("Ext.DisabledText")}</p></div> }.into_any()),
                _ => None,
            }
        };
        let n3 = name.clone();
        let kv = move || {
            let s = sel.get();
            let tools = s.tools.as_ref().map(|t| t.len().to_string()).unwrap_or_else(|| if s.status == "failed" { "0".into() } else { "—".into() });
            let sign_out = (s.status == "connected" && matches!(s.transport.as_deref(), Some("http" | "sse"))).then(|| {
                let n = n3.clone();
                view! { <div class="acts"><button class="btn" disabled=busy on:click=move |_| ext.sign_out(h, n.clone())>{t!("Ext.SignOut")}</button></div> }
            });
            view! {
                <div class="kv">
                    <span>{t!("Ext.Kv.Source")}</span><span>{s.origin.clone()}</span>
                    <span>{t!("Ext.Kv.Transport")}</span><span>{s.transport.clone().unwrap_or_else(|| "—".into())}</span>
                    {s.target.clone().map(|t| view! { <span>{t!("Ext.Kv.Target")}</span><span>{t}</span> })}
                    <span>{t!("Kv.Tools")}</span><span>{tools}</span>
                    <span>{t!("Ext.Kv.Scope")}</span><span>{s.scope.as_deref().map_or_else(|| "—".to_string(), scope_label)}</span>
                    {s.version.clone().map(|v| view! { <span>{t!("Ext.Kv.Version")}</span><span>{v}</span> })}
                </div>
                {sign_out}
            }
        };
        let tools = move || {
            sel.with(|s| s.tools.clone()).filter(|t| !t.is_empty()).map(|tools| {
                view! {
                    <div class="section last">
                        <h3>{t!("Col.Tools")}</h3>
                        <ul class="mcp-tools">
                            {tools.into_iter().map(|tl| view! {
                                <li><code>{tl.name.clone()}</code>
                                    {tl.read_only.then(|| view! { <span class="tag">{t!("Ext.Tool.ReadOnly")}</span> })}
                                    {tl.destructive.then(|| view! { <span class="tag">{t!("Ext.Tool.Destructive")}</span> })}
                                    {tl.open_world.then(|| view! { <span class="tag">{t!("Ext.Tool.OpenWorld")}</span> })}
                                    {tl.description.clone().filter(|d| !d.is_empty()).map(|d| view! { <div class="d">{d}</div> })}
                                </li>
                            }).collect_view()}
                        </ul>
                    </div>
                }
            })
        };
        view! {
            <aside class="pane insp" aria-label=t!("Inspector")>
                <div class="ph"><h2 class="ttl" title=move || sel.with(|s| s.short.clone())>{move || sel.with(|s| s.short.clone())}</h2>
                    <span class="sub">{move || sel.with(|s| s.origin.clone())}</span>{close_inspector(ext.ui)}</div>
                {state_section}
                <div class="section last">{kv}</div>
                {tools}
            </aside>
        }
        .into_any()
    }
}

/// The plugin options inspector (`configuring`).
fn options_inspector(ext: Ext, cid: String) -> AnyView {
    {
        let busy = move || ext.busy.get();
        let rows = move || {
            ext.options.get().unwrap_or_default().into_iter().map(move |o| {
                let id = format!("opt-{}", o.key);
                let key = o.key.clone();
                let saved = o.value.clone();
                let draft = move || ext.option_draft.with(|d| d.get(&key).and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| saved.clone());
                let k = o.key.clone();
                let set = move |v: String| ext.option_draft.update(|d| { d.insert(k.clone(), Value::String(v)); });
                let control = if !o.choices.is_empty() {
                    let set = set.clone();
                    let draft2 = draft.clone();
                    view! {
                        <div class="picks"><select id=id.clone() disabled=busy on:change=move |e| set(event_target_value(&e))>
                            <option value="" selected=move || draft2().is_empty()></option>
                            {o.choices.iter().map(|c| {
                                let (c1, c2) = (c.clone(), c.clone());
                                let d = draft.clone();
                                view! { <option value=c1 selected=move || d() == c2>{c.clone()}</option> }
                            }).collect_view()}
                        </select></div>
                    }
                    .into_any()
                } else {
                    let kind = if o.sensitive { "password" } else if o.kind == "number" { "number" } else { "text" };
                    let ph = (o.sensitive && o.set).then(|| t!("Ext.Plugins.SecretKept"));
                    view! {
                        <div class="input"><input id=id.clone() type=kind prop:value=draft on:input=move |e| set(event_target_value(&e))
                            placeholder=ph spellcheck="false" autocomplete="off" disabled=busy/></div>
                    }
                    .into_any()
                };
                view! {
                    <div class="field">
                        <label for=id>{o.title.clone()}
                            {o.required.then(|| view! { <span class="tag">{t!("Ext.Plugins.Required")}</span> })}
                            {(!o.set).then(|| view! { <span class="tag">{t!("Ext.Plugins.Unset")}</span> })}</label>
                        {control}
                        {o.description.clone().filter(|d| !d.is_empty()).map(|d| view! { <div class="d">{d}</div> })}
                    </div>
                }
            }).collect_view()
        };
        let save_cid = cid.clone();
        view! {
            <aside class="pane insp" aria-label=t!("Inspector")>
                <div class="ph"><h2 class="ttl" title=cid.clone()>{t!("Ext.Plugins.OptionsTitle", cid.split('@').next().unwrap_or_default())}</h2>
                    <span class="sub">{cid.clone()}</span>{close_inspector(ext.ui)}</div>
                <div class="section last">
                    {rows}
                    <div class="acts">
                        <button class="btn primary" disabled=move || busy() || ext.option_draft.with(Map::is_empty) on:click=move |_| {
                            let s = ext.live.get_untracked();
                            let cwd = s.map_or_else(|| ext.home.get_value(), |h| ext.cwd_now(h));
                            spawn_local(ext.save_options(s, cwd, save_cid.clone()));
                        }>{t!("Ext.Plugins.Save")}</button>
                        <button class="btn" on:click=move |_| { ext.configuring.set(None); ext.options.set(None); }>{t!("Action.Close")}</button>
                    </div>
                </div>
            </aside>
        }
        .into_any()
    }
}

/// `i18n::plural` with a usize.
pub fn plural(n: usize, key: &str) -> String {
    i18n::plural(n as i64, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccui_shared::hooks::HookEdit;
    use ccui_shared::i18n::{set_thread_lang, Lang};
    use ccui_shared::model::McpBrief;

    #[test]
    fn split_names() {
        set_thread_lang(Some(Lang::En));
        assert_eq!(split_server("plugin:github:github", None, None), ("github".into(), "plugin:github".into(), "plugin".into()));
        assert_eq!(split_server("claude.ai Gmail", None, Some("claudeai")), ("Gmail".into(), "claude.ai".into(), "claude.ai".into()));
        assert_eq!(split_server("probe", Some("project"), Some("projectSettings")), ("probe".into(), "project".into(), "projectSettings".into()));
        assert_eq!(split_server("x", None, None), ("x".into(), "".into(), "user".into()));
        assert_eq!(split_server("plugin:only", Some("dynamic"), Some("claudeai")).1, "dynamic", "two parts only: not a plugin name");
    }

    #[test]
    fn sign_in_here() {
        assert!(signs_in_here(Some("http"), "user"));
        assert!(signs_in_here(None, "plugin"));
        assert!(!signs_in_here(Some("claudeai-proxy"), "user"));
        assert!(!signs_in_here(None, "claude.ai"));
    }

    #[test]
    fn servers_from_status_or_init() {
        set_thread_lang(Some(Lang::En));
        let mut h = SessionHeavy::default();
        h.init = Some(InitInfo { mcp: vec![McpBrief { name: "plugin:gh:gh".into(), status: "failed".into() }], ..InitInfo::default() });
        let s = servers(&h);
        assert_eq!((s[0].short.as_str(), s[0].status.as_str(), s[0].tools.is_none()), ("gh", "failed", true));
        h.mcp_status = Some(serde_json::json!({"mcpServers":[
            {"name":"probe","status":"connected","scope":"local","config":{"type":"stdio","command":"node","args":["a.js",1]},
             "tools":[{"name":"ask"}],"serverInfo":{"version":"1.2"}},
            {"name":"web","status":"needs-auth","config":{"type":"http","url":"https://x"}}]}));
        let s = servers(&h);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].target.as_deref(), Some("node a.js 1"));
        assert_eq!((s[0].origin.as_str(), s[0].version.as_deref(), s[0].tools.as_ref().map(Vec::len)), ("local", Some("1.2"), Some(1)));
        assert_eq!((s[1].target.as_deref(), s[1].transport.as_deref(), s[1].tools.is_none()), (Some("https://x"), Some("http"), true));
        assert_eq!(row_act(&s[0]), RowAct::Toggle);
        assert_eq!(row_act(&s[1]), RowAct::SignIn);
    }

    #[test]
    fn source_split() {
        assert_eq!(split_source(None), (None, None));
        assert_eq!(split_source(Some("(commit-commands) Commit all")), (Some("Commit all".into()), Some("commit-commands".into())));
        assert_eq!(split_source(Some("Review a PR (user)")), (Some("Review a PR".into()), Some("user".into())));
        assert_eq!(split_source(Some("Run it (a b)")), (Some("Run it (a b)".into()), None));
        assert_eq!(split_source(Some("Plain")), (Some("Plain".into()), None));
        assert_eq!(split_source(Some("x (plugin:p@m)")), (Some("x".into()), Some("plugin:p@m".into())));
    }

    #[test]
    fn labels_and_helpers() {
        set_thread_lang(Some(Lang::En));
        assert_eq!(scope_label("user"), "user");
        assert_eq!(scope_label("dynamic"), "plugin");
        assert_eq!(scope_label("claudeai"), "claude.ai");
        assert_eq!(scope_label("managed"), "managed");
        assert_eq!(source_tag("cliArg"), "command line");
        assert_eq!(breaks("a/b<c", '/'), "a/<wbr>b&lt;c");
        assert_eq!(stop_of(Some("b"), &["a".into(), "b".into()]).as_deref(), Some("b"));
        assert_eq!(stop_of(Some("z"), &["a".into()]).as_deref(), Some("a"));
        assert_eq!(stop_of(None, &[]), None);
        assert!(matches(" GIT ", "commit", Some("Git helper")) && matches("", "x", None) && !matches("q", "x", None));
        assert_eq!(link("hooks", Some("a b")), "extensions?tab=hooks&session=a%20b");
        assert_eq!(link("mcp", None), "extensions?tab=mcp");
    }

    fn hook(source: &str, plugin: Option<&str>) -> HookRow {
        HookRow {
            event: "PreToolUse".into(),
            matcher: "Bash".into(),
            kind: "command".into(),
            command: "x".into(),
            display: None,
            source: source.into(),
            source_label: None,
            plugin_name: plugin.map(String::from),
            timeout: Some(5),
            disabled: false,
            edit: Some(HookEdit { matcher: "Bash".into(), config: serde_json::json!({"onFailure":"block"}) }),
        }
    }

    #[test]
    fn hook_signature_and_names() {
        let l = HookListing { rows: vec![hook("localSettings", None)], ..HookListing::default() };
        assert_eq!(hook_rows_sig(Some(&l)), "PreToolUse|localSettings|Bash|x|Some(5)|true");
        assert_eq!(hook_rows_sig(None), "");
        assert_eq!(hook_name(&l.rows[0]), "PreToolUse (Bash)");
    }

    #[test]
    fn plugin_states() {
        let p = PluginRow { id: "gh@mk".into(), enabled: true, scope: "user".into(), ..PluginRow::default() };
        let srv = |st: &str| Srv { name: format!("plugin:gh:{st}"), status: st.into(), origin: "plugin:gh".into(), ..Srv::default() };
        assert_eq!(plugin_state(&PluginRow { enabled: false, ..p.clone() }, None, None, None, None, true), PState::Off);
        assert_eq!(plugin_state(&p, None, None, None, None, true), PState::On);
        let both = [srv("failed"), srv("needs-auth")];
        assert!(matches!(plugin_state(&p, None, Some(&both), None, None, true), PState::ServerFailing(1, _)));
        assert!(matches!(plugin_state(&p, None, Some(&both[1..]), None, None, true), PState::NeedsAuth(_)));
        assert!(matches!(plugin_state(&p, None, Some(&[srv("disabled")]), None, None, true), PState::ServersOff(1, _)));
        let init = InitInfo { plugins: vec!["gh".into()], ..InitInfo::default() };
        assert_eq!(plugin_state(&p, Some(&init), None, None, None, true), PState::Loaded);
        assert_eq!(plugin_state(&p, Some(&InitInfo::default()), None, None, None, true), PState::Reload);
        assert_eq!(plugin_state(&p, None, None, Some(&["gh:commit".into()]), None, true), PState::Loaded);
        let hooks = HookListing { rows: vec![hook("pluginHook", Some("gh@mk"))], ..HookListing::default() };
        assert_eq!(plugin_state(&p, None, None, None, Some(&hooks), true), PState::Loaded);
        assert_eq!(plugin_state(&p, None, None, None, Some(&hooks), false), PState::On, "the home view has no hooks");
    }

    #[test]
    fn failed_where_text() {
        set_thread_lang(Some(Lang::En));
        let m = Srv { origin: "plugin:gh".into(), ..Srv::default() };
        let rows = [PluginRow { id: "gh@mk".into(), has_options: true, ..PluginRow::default() }];
        assert_eq!(failed_where(&m, Some(&rows)), t!("Ext.FailedWhere.PluginOptions", "gh"));
        assert_eq!(failed_where(&m, None), t!("Ext.FailedWhere.Plugin", "gh"));
        assert_eq!(failed_where(&Srv { scope: Some("project".into()), ..Srv::default() }, None), t!("Ext.FailedWhere.Project"));
        assert_eq!(failed_where(&Srv { tag: "claude.ai".into(), ..Srv::default() }, None), t!("Ext.FailedWhere.Claude"));
    }
}
