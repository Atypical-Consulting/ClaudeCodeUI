#![warn(unused, dead_code)]
//! Port of `Components/Pages/NewSession.razor` — spec 06 §6.1. Owner: ui-pages-a.

use crate::common::duck::{Duck, Mood};
use crate::common::effort_slider::EffortSlider;
use crate::common::icon::Icon;
use crate::common::pick_folder::PickFolder;
use crate::ipc;
use crate::session::extra_dirs::{AddDir, ExtraDirs, ExtraDirsCtl, LocalFut};
use crate::state::use_app;
use ccui_shared::api::StartArgs;
use ccui_shared::{i18n, session_rules, t, text};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use leptos_router::NavigateOptions;
use std::rc::Rc;

/// CLI aliases; "" keeps the CLI / settings default.
const MODELS: [&str; 3] = ["opus", "sonnet", "haiku"];
/// `claude --effort` levels; "" keeps the CLI / settings default.
const EFFORTS: [&str; 6] = ["", "low", "medium", "high", "xhigh", "max"];

/// `<PageTitle>{name} · Claude Code</PageTitle>` (shared by the pages of this unit).
pub(crate) fn set_title(name: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        doc.set_title(&format!("{name} · Claude Code"));
    }
}

/// A router navigation every handler of a page can call (`go(path)`, base-relative path without the leading `/`).
pub(crate) type Go = StoredValue<Rc<dyn Fn(&str)>, LocalStorage>;

pub(crate) fn use_go() -> Go {
    let navigate = use_navigate();
    StoredValue::new_local(Rc::new(move |path: &str| navigate(&format!("/{}", path.trim_start_matches('/')), NavigateOptions::default())))
}

pub(crate) fn go(g: Go, path: &str) {
    if let Some(f) = g.try_get_value() {
        f(path);
    }
}

/// Every modifier off (the pages' single-letter keys).
pub(crate) fn plain_key(e: &leptos::ev::KeyboardEvent) -> bool {
    !(e.ctrl_key() || e.meta_key() || e.alt_key())
}

/// `Path.TrimEndingDirectorySeparator`: one trailing separator dropped, a root kept.
pub(crate) fn trim_sep(p: &str) -> &str {
    let t = p.strip_suffix(['/', '\\']).unwrap_or(p);
    if t.is_empty() || t.ends_with(':') { p } else { t }
}

/// `Path.GetFileName` of a separator-free end (both separators).
pub(crate) fn file_name(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// The desktop shell starts in ~: Claude could touch every dotfile there, with no worktree to isolate it.
pub fn is_home(cwd: &str, home: &str) -> bool {
    !home.is_empty() && trim_sep(cwd).to_lowercase() == trim_sep(home).to_lowercase()
}

/// `SetCwd`'s normalisation: trimmed, then quotes trimmed (a pasted "C:\x").
pub fn clean_cwd(v: &str) -> String {
    v.trim().trim_matches('"').to_string()
}

/// auto / acceptEdits edit the folder unasked: outside an isolated worktree, or with an extra folder (never
/// isolated), the second Start confirms.
pub fn needs_confirm(mode: &str, add_dirs: usize, isolated: bool, cwd: &str, confirmed: bool) -> bool {
    session_rules::risky(mode) && (add_dirs > 0 || !isolated && text::root_of(cwd) == cwd) && !confirmed
}

/// The session's first name: the worktree, else the folder (the CLI retitles it after the first prompt).
pub fn session_name(wt: Option<&str>, cwd: &str) -> String {
    wt.map_or_else(|| file_name(trim_sep(cwd)).to_string(), str::to_string)
}

fn opt(s: String) -> Option<String> {
    Some(s).filter(|s| !s.is_empty())
}

/// Start card (folder + PickFolder + recents, extra dirs, worktree isolation & name, model,
/// fallback, agent, effort, permission mode, launch flags, from-PR) or the claude-not-found card. `?cwd=`.
#[component]
pub fn NewSession() -> impl IntoView {
    let app = use_app();
    let nav = use_go();
    let query = use_query_map();
    let boot = app.boot.get_value();
    set_title(&t!("Rail.NewSession"));

    let found = RwSignal::new(true); // probed on mount
    let npm_shim = RwSignal::new(false);
    let cwd = RwSignal::new(String::new());
    let mode = RwSignal::new(String::new());
    let model = RwSignal::new(String::new());
    let effort = RwSignal::new(String::new());
    let fallback = RwSignal::new(String::new());
    let from_pr = RwSignal::new(String::new());
    let agent = RwSignal::new(String::new());
    let agent_names = RwSignal::new(Vec::<String>::new());
    let branch = RwSignal::new(None::<String>);
    let probed = RwSignal::new(false); // "branch is null" means "not git" only once the first probe answered
    let wt_name = RwSignal::new(None::<String>);
    let pick_error = RwSignal::new(None::<String>);
    let draft = RwSignal::new(String::new()); // the name as typed: committed by OK / Enter / Start, dropped by Escape
    let edit_from = StoredValue::new(String::new());
    let exists = RwSignal::new(true);
    let worktree = RwSignal::new(true);
    let editing = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let untrusted = RwSignal::new(false);
    let confirm_risk = RwSignal::new(false);
    let restricted = RwSignal::new(false);
    let safe_mode = RwSignal::new(false);
    let pr_missing = RwSignal::new(false);
    let recents = RwSignal::new(Vec::<String>::new());
    let add_dirs = RwSignal::new(Vec::<String>::new());
    let ready = RwSignal::new(false);
    let last_query = StoredValue::new(None::<String>);
    let probe = StoredValue::new(0u64);
    let ctl = ExtraDirsCtl::new();
    // Generated like the terminal's (`claude -w`), one per visit; Edit replaces it.
    let generated = text::random_name((js_sys::Math::random() * 8000.0) as u64);
    let cwd_input = NodeRef::<leptos::html::Input>::new();
    let name_input = NodeRef::<leptos::html::Input>::new();
    let edit_btn = NodeRef::<leptos::html::Button>::new();
    let home = StoredValue::new(boot.home.clone());

    let isolated = move || worktree.get() && branch.get().is_some();
    let wt_label = {
        let generated = generated.clone();
        move || wt_name.get().unwrap_or_else(|| generated.clone())
    };

    // Keyboard-first: Edit hands focus to the input, OK / Escape back to Edit (refs exist only after the render).
    let focus_name = move |to_input: bool| {
        request_animation_frame(move || {
            if to_input {
                if let Some(i) = name_input.get_untracked() {
                    let _ = i.focus();
                }
            } else if let Some(b) = edit_btn.get_untracked() {
                let _ = b.focus();
            }
        });
    };

    // SetCwd: a later call wins (each probe is numbered).
    let set_cwd = move |value: String| async move {
        let value = clean_cwd(&value);
        if value != cwd.get_untracked() {
            untrusted.set(false); // the user picked another folder themselves
        }
        pick_error.set(None);
        cwd.set(value.clone());
        confirm_risk.set(false);
        let my = probe.get_value() + 1;
        probe.set_value(my);
        let ok = ipc::local_dir(&value).await.unwrap_or(false);
        let (b, names) = if ok {
            (ipc::git_branch(&value).await.ok().flatten(), ipc::agent_names(&value).await.unwrap_or_default())
        } else {
            (None, Vec::new())
        };
        if probe.try_get_value() != Some(my) {
            return ok;
        }
        exists.set(ok);
        branch.set(b);
        agent_names.set(names);
        probed.set(true);
        ok
    };

    // Mount: claude on PATH, the last projects (the last one rather than the process folder, ~ on desktop).
    leptos::task::spawn_local(async move {
        if let Ok(f) = ipc::claude_found().await {
            found.set(f.found);
            npm_shim.set(f.npm_shim_only);
        }
        let r = ipc::recent_roots().await.unwrap_or_default();
        let first = match r.first() {
            Some(c) => c.clone(),
            None => ipc::default_cwd().await.unwrap_or_default(),
        };
        if cwd.is_disposed() {
            return;
        }
        recents.set(r);
        cwd.set(first);
        ready.set(true);
    });

    // OnParametersSet: `?cwd=` handed over by a link.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let q = query.with(|m| m.get("cwd")).filter(|q| !q.is_empty());
        let link = q.is_some() && q != last_query.get_value();
        if link {
            last_query.set_value(q.clone());
            cwd.set(q.unwrap_or_default());
        }
        let c = cwd.get_untracked();
        leptos::task::spawn_local(async move {
            let ok = set_cwd(c.clone()).await;
            // A folder handed over by a URL and never used before: claude would load its .claude settings and hooks,
            // so ask first.
            if link && ok {
                let used = ipc::folder_used(&c).await.unwrap_or(false) || ipc::folder_used(&text::root_of(&c)).await.unwrap_or(false);
                untrusted.set(!used);
            }
        });
    });

    // A long path shows its distinguishing end, not its start.
    Effect::new(move |_| {
        cwd.track();
        found.track();
        request_animation_frame(move || {
            if let Some(i) = cwd_input.get_untracked() {
                crate::interop::scroll_to_end(&i);
            }
        });
    });

    let commit_name = move || {
        // A draft that sanitizes to nothing keeps the generated name.
        let d = draft.get_untracked();
        if d != edit_from.get_value() {
            wt_name.set(text::worktree_name(&d));
        }
        editing.set(false);
        focus_name(false);
    };
    let edit_name = {
        let wt_label = wt_label.clone();
        move |_| {
            let n = wt_label();
            draft.set(n.clone());
            edit_from.set_value(n);
            editing.set(true);
            focus_name(true);
        }
    };

    let resume_pr = move || {
        if busy.get_untracked() || !exists.get_untracked() || from_pr.get_untracked().trim().is_empty() {
            return;
        }
        let (pr, c) = (from_pr.get_untracked(), cwd.get_untracked());
        leptos::task::spawn_local(async move {
            match ipc::linked_to_pr(&pr, &c).await {
                Ok(Some(p)) => go(nav, &format!("session/{}", p.id)),
                _ => pr_missing.set(true),
            }
        });
    };

    let start = {
        let generated = generated.clone();
        move || {
            if busy.get_untracked() || untrusted.get_untracked() {
                return;
            }
            let generated = generated.clone();
            leptos::task::spawn_local(async move {
                let c = cwd.get_untracked();
                let ok = ipc::local_dir(&c).await.unwrap_or(false);
                if exists.is_disposed() {
                    return;
                }
                exists.set(ok);
                if !ok || busy.get_untracked() {
                    return;
                }
                busy.set(true);
                if !ctl.commit().await {
                    busy.set(false); // a typed folder is not dropped
                    return;
                }
                if editing.get_untracked() {
                    commit_name(); // nor a typed worktree name
                }
                // A folder edit blurred into this click: SetCwd's probe may still be pending.
                let b = ipc::git_branch(&c).await.ok().flatten();
                if busy.is_disposed() {
                    return;
                }
                branch.set(b.clone());
                let iso = worktree.get_untracked() && b.is_some();
                let m = mode.get_untracked();
                let dirs = add_dirs.get_untracked();
                if needs_confirm(&m, dirs.len(), iso, &c, confirm_risk.get_untracked()) {
                    confirm_risk.set(true);
                    busy.set(false);
                    return;
                }
                let wt = if iso {
                    match wt_name.get_untracked() {
                        Some(n) => Some(n),
                        None => Some(ipc::free_worktree_name(&c, &generated).await.unwrap_or(generated)),
                    }
                } else {
                    None
                };
                if busy.is_disposed() {
                    return;
                }
                let md = model.get_untracked();
                let fb = fallback.get_untracked();
                let args = StartArgs {
                    name: session_name(wt.as_deref(), &c),
                    cwd: c,
                    mode: m,
                    worktree: wt,
                    model: opt(md.clone()),
                    effort: opt(effort.get_untracked()),
                    add_dirs: dirs,
                    fallback: opt(fb).filter(|f| *f != md),
                    restricted: restricted.get_untracked(),
                    safe_mode: safe_mode.get_untracked(),
                    agent: opt(agent.get_untracked().trim().to_string()),
                };
                match ipc::session_start(&args).await {
                    Ok(id) => go(nav, &format!("session/{id}")), // busy stays: the page leaves
                    Err(e) => {
                        leptos::logging::error!("session_start: {e}");
                        busy.set(false);
                    }
                }
            });
        }
    };

    let on_add: AddDir = Callback::new(move |d: String| {
        add_dirs.update(|v| v.push(d));
        confirm_risk.set(false);
        Box::pin(async { Ok(()) }) as LocalFut<Result<(), String>>
    });
    let on_remove = Callback::new(move |d: String| {
        add_dirs.update(|v| {
            if let Some(i) = v.iter().position(|x| *x == d) {
                v.remove(i);
            }
        });
        confirm_risk.set(false);
    });

    let not_found = move || {
        view! {
            <div class="start-card">
                <div class="hello">
                    <div class="duck"><Duck mood=Mood::Ko size=88/></div>
                    <div><h2>{t!("New.NotFound")}</h2><p inner_html=move || t!(if npm_shim.get() { "New.NpmShimText" } else { "New.NotFoundText" })></p></div>
                </div>
                <div class="field">
                    <h4 class="lbl">{t!("New.Step1")}</h4>
                    <p>{t!("New.Unix")}" "<code>"curl -fsSL https://claude.ai/install.sh | bash"</code></p>
                    <p>{t!("New.Windows")}" "<code>"irm https://claude.ai/install.ps1 | iex"</code></p>
                    <p>{t!("New.Guide")}" "<a href=t!("New.GuideUrl") target="_blank" rel="noopener" style="color:var(--syn-fn);text-decoration-color:color-mix(in srgb,var(--syn-fn) 40%,transparent);text-underline-offset:3px">"docs.claude.com"</a></p>
                </div>
                <div class="field">
                    <h4 class="lbl">{t!("New.Step2")}</h4>
                    <p inner_html=t!("New.Step2Text")></p>
                </div>
                <div class="field">
                    <h4 class="lbl">{t!("New.Step3")}</h4>
                    <p inner_html=t!("New.Step3Text")></p>
                </div>
                <div class="row"><button class="btn primary" on:click=move |_| {
                    leptos::task::spawn_local(async move {
                        if let Ok(f) = ipc::claude_found().await {
                            found.set(f.found);
                            npm_shim.set(f.npm_shim_only);
                        }
                    });
                }>{t!("New.CheckAgain")}</button></div>
            </div>
        }
    };

    let card = {
        let start = start.clone();
        let start2 = start.clone();
        let wt_label = wt_label.clone();
        move || {
            let start = start.clone();
            let start2 = start2.clone();
            let wt_label = wt_label.clone();
            let edit_name = edit_name.clone();
            let folder_msg = move || {
                if let Some(e) = pick_error.get() {
                    view! { <div id="cwd-err" role="alert" class="fmsg">{e}</div> }.into_any()
                } else if !exists.get() {
                    view! { <div id="cwd-err" role="alert" class="fmsg">{t!("New.NoFolder")}</div> }.into_any()
                } else if untrusted.get() {
                    view! {
                        <div role="alert" class="fmsg" style="display:flex;align-items:center;gap:10px;flex-wrap:wrap">
                            <span>{t!("New.Untrusted")}</span>
                            <button class="btn" on:click=move |_| untrusted.set(false)>{t!("New.Trust")}</button>
                        </div>
                    }
                    .into_any()
                } else if is_home(&cwd.get(), &home.get_value()) {
                    view! { <div role="status" class="fmsg risk">{t!("New.HomeFolder")}</div> }.into_any()
                } else {
                    ().into_any()
                }
            };
            let recent_chips = move || {
                let r = recents.get();
                (!r.is_empty()).then(|| {
                    let (h, s) = (home.get_value(), app.boot.with_value(|b| b.sep.clone()));
                    view! {
                        <div class="recents">
                            {r.into_iter().map(|d| {
                                let chip = text::chip(&d, &h, &s);
                                let d2 = d.clone();
                                view! { <button class="chip" title=d on:click=move |_| { leptos::task::spawn_local(set_cwd(d2.clone()).map_unit()); }>{chip}</button> }
                            }).collect_view()}
                        </div>
                    }
                })
            };
            let iso_tail = move || {
                if branch.get().is_none() {
                    return probed.get().then(|| view! { <span class="branch">{t!("New.NotGit")}</span> }).into_any();
                }
                if !worktree.get() {
                    return ().into_any();
                }
                if editing.get() {
                    return view! {
                        <span class="branch pre"><Icon name="branch"/>"worktree-"</span>
                        <input node_ref=name_input prop:value=move || draft.get() on:input=move |e| draft.set(event_target_value(&e))
                            on:keydown=move |e: leptos::ev::KeyboardEvent| {
                                if e.key() == "Enter" && !e.ctrl_key() && !e.meta_key() {
                                    commit_name();
                                } else if e.key() == "Escape" {
                                    editing.set(false);
                                    focus_name(false);
                                }
                            }
                            title=move || draft.get() aria-label=t!("New.WorktreeName") spellcheck="false" autocomplete="off"/>
                        <button class="b btn ghost" on:click=move |_| commit_name()>{t!("Action.Ok")}</button>
                    }
                    .into_any();
                }
                let label = t!("New.WorktreeFrom", format!("worktree-{}", wt_label()), branch.get().unwrap_or_default());
                let title = label.clone();
                view! {
                    <span class="branch" title=title><Icon name="branch"/>{label}</span>
                    <button class="b btn ghost" node_ref=edit_btn on:click=edit_name.clone()>{t!("New.Edit")}</button>
                }
                .into_any()
            };
            let modes = std::iter::once("").chain(session_rules::MODES).map(|m| {
                let on = move || mode.get() == m;
                view! {
                    <button class=move || if on() { "mode on" } else { "mode" } role="radio" aria-checked=move || if on() { "true" } else { "false" }
                        tabindex=move || if on() { 0 } else { -1 } on:click=move |_| { mode.set(m.to_string()); confirm_risk.set(false); }>
                        <b class=session_rules::risky(m).then_some("risk")>{i18n::name("Mode.Name.", Some(m))}</b><span>{t!(&format!("Mode.{m}"))}</span>
                    </button>
                }
            }).collect_view();
            let toggle = |sig: RwSignal<bool>, title: String, label: String| {
                view! {
                    <button class=move || if sig.get() { "toggle on" } else { "toggle" } role="switch" aria-checked=move || if sig.get() { "true" } else { "false" }
                        title=title on:click=move |_| sig.update(|v| *v = !*v)><span class="sw"></span>{label}</button>
                }
            };
            view! {
                <div class="start-card" on:keydown=move |e: leptos::ev::KeyboardEvent| {
                    if e.key() == "Enter" && (e.ctrl_key() || e.meta_key()) {
                        start();
                    }
                }>
                    <div class="hello">
                        <div class="duck"><Duck mood=Mood::Happy size=88/></div>
                        <div><h2>{t!("New.Hello")}</h2><p>{t!("New.HelloText")}</p></div>
                    </div>
                    <div class="field">
                        <label for="cwd">{t!("New.Folder")}</label>
                        <div class="input">
                            <PickFolder start=Signal::derive(move || cwd.get())
                                on_pick=Callback::new(move |d: String| leptos::task::spawn_local(set_cwd(d).map_unit()))
                                on_error=Callback::new(move |m: String| pick_error.set(Some(m)))/>
                            <input id="cwd" node_ref=cwd_input prop:value=move || cwd.get()
                                aria-invalid=move || (!exists.get()).then_some("true")
                                aria-describedby=move || (!exists.get() || pick_error.get().is_some()).then_some("cwd-err")
                                on:change=move |e| leptos::task::spawn_local(set_cwd(event_target_value(&e)).map_unit())
                                spellcheck="false" autocomplete="off"/>
                        </div>
                        {folder_msg}
                        {recent_chips}
                    </div>
                    <div class="field">
                        <label for="add-dir">{t!("Dirs.Label")}</label>
                        <ExtraDirs id="add-dir" dirs=Signal::derive(move || add_dirs.get()) cwd=Signal::derive(move || cwd.get())
                            on_add=on_add on_remove=on_remove ctl=ctl/>
                    </div>
                    <div class="field">
                        <span class="lbl" id="iso-lbl">{t!("New.Isolation")}</span>
                        <div class="input iso" role="group" aria-labelledby="iso-lbl">
                            <button class=move || if isolated() { "toggle on" } else { "toggle" } role="switch"
                                aria-checked=move || if isolated() { "true" } else { "false" } disabled=move || branch.get().is_none()
                                on:click=move |_| { worktree.update(|w| *w = !*w); confirm_risk.set(false); }><span class="sw"></span>{t!("New.Worktree")}</button>
                            {iso_tail}
                        </div>
                    </div>
                    <div class="field">
                        <label for="model">{t!("Model")}</label>
                        <div class="picks">
                            <select id="model" prop:value=move || model.get() on:change=move |e| model.set(event_target_value(&e))>
                                <option value="">{t!("New.DefaultModel")}</option>
                                {MODELS.iter().map(|m| view! { <option value=*m>{*m}</option> }).collect_view()}
                            </select>
                        </div>
                    </div>
                    <div class="field">
                        <label for="fallback">{t!("New.Fallback")}</label>
                        <div class="picks">
                            // --fallback-model: used when the model is overloaded. A fallback equal to the model is pointless.
                            <select id="fallback" title=t!("New.FallbackHint") prop:value=move || fallback.get() on:change=move |e| fallback.set(event_target_value(&e))>
                                <option value="">{t!("New.NoFallback")}</option>
                                {move || MODELS.into_iter().filter(|m| *m != model.get()).map(|m| view! { <option value=m selected=fallback.get_untracked() == m>{m}</option> }).collect_view()}
                            </select>
                        </div>
                    </div>
                    <div class="field">
                        <label for="agent">{t!("New.Agent")}</label>
                        // --agent: the main thread runs as that agent (its prompt, tools and model). Free text: any agent the CLI knows.
                        <div class="input"><input id="agent" list="agent-names" prop:value=move || agent.get() on:change=move |e| agent.set(event_target_value(&e))
                            placeholder=t!("New.AgentHint") spellcheck="false" autocomplete="off"/></div>
                        <datalist id="agent-names">
                            {move || agent_names.get().into_iter().map(|a| view! { <option value=a></option> }).collect_view()}
                        </datalist>
                    </div>
                    <div class="field">
                        <label for="effort">{t!("New.Effort")}</label>
                        <EffortSlider id="effort" levels=EFFORTS.map(String::from).to_vec() value=Signal::derive(move || effort.get())
                            on_change=Callback::new(move |e: String| effort.set(e))
                            name=Callback::new(|e: String| i18n::name("Effort.", Some(if e.is_empty() { "default" } else { &e })))/>
                    </div>
                    <div class="field">
                        <span class="lbl" id="perm-lbl">{t!("New.Permissions")}</span>
                        // "" first: no --permission-mode, the CLI applies permissions.defaultMode as the terminal does.
                        <div class="modes" role="radiogroup" aria-labelledby="perm-lbl">{modes}</div>
                    </div>
                    <div class="field">
                        <span class="lbl" id="launch-lbl">{t!("New.Launch")}</span>
                        // --restricted / --safe-mode: kept by the session, so a restart relaunches it the same way.
                        <div class="input iso" role="group" aria-labelledby="launch-lbl">
                            {toggle(restricted, t!("New.RestrictedHint"), t!("New.Restricted"))}
                            {toggle(safe_mode, t!("New.SafeModeHint"), t!("New.SafeMode"))}
                        </div>
                    </div>
                    <div class="field">
                        <label for="from-pr">{t!("New.FromPr")}</label>
                        // --from-pr: the folder's session linked to that PR, resumed under its own id.
                        <div class="input"><input id="from-pr" prop:value=move || from_pr.get() on:input=move |e| from_pr.set(event_target_value(&e))
                                on:keydown=move |e: leptos::ev::KeyboardEvent| {
                                    pr_missing.set(false);
                                    // Enter here resumes the PR's session, not Ctrl Enter's new one.
                                    if e.key() == "Enter" && !e.ctrl_key() && !e.meta_key() {
                                        resume_pr();
                                    }
                                }
                                placeholder=t!("New.FromPrHint") aria-invalid=move || pr_missing.get().then_some("true")
                                aria-describedby=move || pr_missing.get().then_some("pr-err") spellcheck="false" autocomplete="off"/>
                            <button class="btn ghost" disabled=move || busy.get() || from_pr.get().trim().is_empty() || !exists.get()
                                on:click=move |_| resume_pr()>{t!("New.FromPrGo")}</button></div>
                        {move || pr_missing.get().then(|| view! { <div id="pr-err" role="alert" class="fmsg">{t!("New.FromPrNone", from_pr.get().trim().to_string())}</div> })}
                    </div>
                    // As in the terminal: the session opens first, the prompt is written in its composer.
                    <div class="row">
                        {move || confirm_risk.get().then(|| {
                            let key = if add_dirs.get().is_empty() { "New.ConfirmRisky" } else { "Dirs.ConfirmRisky" };
                            view! { <span class="risk" role="alert">{t!(key, i18n::name("Mode.Name.", Some(&mode.get())))}</span> }
                        })}
                        <button class="btn primary go" disabled=move || busy.get() || !exists.get() || untrusted.get() on:click=move |_| start2()>
                            {move || if confirm_risk.get() { t!("New.StartAnyway") } else { t!("New.Start") }}" "<kbd>{i18n::kbd("Ctrl ⏎")}</kbd>
                        </button>
                    </div>
                    <div class="hint">
                        <span><kbd>{i18n::kbd("Ctrl K")}</kbd>" "{t!("New.HintSwitch")}</span>
                        <span><kbd>{i18n::kbd("Alt N")}</kbd>" "{t!("New.HintNew")}</span>
                        <span><kbd>{i18n::kbd("Ctrl I")}</kbd>" "{t!("New.HintInspector")}</span>
                    </div>
                </div>
            }
        }
    };

    view! {
        <div class="pane">
            <div class="ph"><h1 class="ttl">{t!("Rail.NewSession")}</h1></div>
            <div class="start">
                {move || if found.get() { card.clone()().into_any() } else { not_found().into_any() }}
            </div>
        </div>
    }
}

/// `fut.map_unit()`: a `bool` future as the `()` future `spawn_local` takes.
trait MapUnit: std::future::Future + Sized + 'static {
    fn map_unit(self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        Box::pin(async move {
            self.await;
        })
    }
}
impl<F: std::future::Future + 'static> MapUnit for F {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_folder() {
        assert!(is_home("/Users/me/", "/Users/me"));
        assert!(is_home("C:\\Users\\Me", "c:\\users\\me\\"));
        assert!(!is_home("/Users/me/repo", "/Users/me"));
        assert!(!is_home("/", ""));
    }

    #[test]
    fn cwd_cleaning() {
        assert_eq!(clean_cwd("  \"C:\\x\"  "), "C:\\x");
        assert_eq!(trim_sep("/"), "/");
        assert_eq!(trim_sep("C:\\"), "C:\\");
        assert_eq!(trim_sep("/a/b/"), "/a/b");
    }

    #[test]
    fn risk_confirm() {
        // plain repo root, risky mode: confirm once
        assert!(needs_confirm("auto", 0, false, "/r", false));
        assert!(!needs_confirm("auto", 0, false, "/r", true));
        // isolated: no confirm, unless an extra folder is added
        assert!(!needs_confirm("acceptEdits", 0, true, "/r", false));
        assert!(needs_confirm("acceptEdits", 1, true, "/r", false));
        // inside a worktree folder: already isolated
        assert!(!needs_confirm("auto", 0, false, "/r/.claude/worktrees/x", false));
        assert!(!needs_confirm("plan", 2, false, "/r", false));
    }

    #[test]
    fn names() {
        assert_eq!(session_name(Some("brave-x"), "/r"), "brave-x");
        assert_eq!(session_name(None, "/a/repo/"), "repo");
        assert_eq!(session_name(None, "C:\\a\\api"), "api");
    }
}
