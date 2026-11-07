//! Port of `Components/Session/MemoryPanel.razor` — spec 07 §34, 05 §3. Owner: ui-session-inspector.
//! Memory (CLAUDE.md) files of the session, the /memory equivalent: the CLI's list (get_context_usage.memoryFiles)
//! plus the standard locations it did not load. A row opens the file rendered; Edit swaps in a textarea, Save writes it
//! back unless it changed on disk since it was read. Only a listed path is ever written (the backend checks).
//! The open file and the draft live on the session (`SessionHandle::memory`): a permission request unmounts this panel.
//! Then auto-memory, from get_memory_dialog: its switch and the saved memories, which open in the same viewer and can
//! be deleted.

use crate::common::markdown::Markdown;
use crate::ipc;
use crate::state::{use_app, SessionHandle};
use ccui_shared::memory::{self, AutoMemory, MemoryFile, MemorySave};
use ccui_shared::model::*;
use ccui_shared::{fmt, i18n, json, t};
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// The `.c` column of a memory file row.
pub fn file_note(f: &MemoryFile, has_context: bool) -> String {
    match f.tokens {
        Some(n) => t!("Workflow.Tokens", fmt::tokens(n)),
        None if !f.exists => t!("Mem.Absent"),
        // Without get_context_usage yet (a past session) nothing is known about what the CLI loaded.
        None if !has_context => String::new(),
        None => t!("Mem.NotLoaded"),
    }
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

/// Memory files + auto memories: open, edit (SessionHandle.memory), save with conflict, delete.
#[component]
pub fn MemoryPanel(
    session: SessionHandle,
) -> impl IntoView {
    let home = use_app().boot.with_value(|b| b.home.clone());
    let m = session.memory;
    let files = RwSignal::new(Vec::<MemoryFile>::new());
    let memories = RwSignal::new(Vec::<AutoMemory>::new());
    let error = RwSignal::new(None::<String>);
    let delete_armed = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let saved = RwSignal::new(false);
    let area = NodeRef::<html::Textarea>::new();
    let cwd = Memo::new(move |_| session.meta.with(|x| x.cwd.clone()));
    // Per field: a keystroke in the draft re-renders nothing but the textarea's value.
    let open_m = Memo::new(move |_| m.with(|x| x.open.clone()));
    let editing_m = Memo::new(move |_| m.with(|x| x.editing));
    let conflict_m = Memo::new(move |_| m.with(|x| x.conflict));
    let loaded_m = Memo::new(move |_| m.with(|x| x.loaded.clone()));

    let focus_area = move || {
        request_animation_frame(move || {
            if let Some(a) = area.get_untracked() {
                let _ = a.focus();
            }
        })
    };
    let reload_lists = move || {
        let id = session.id();
        spawn_local(async move {
            if let Ok(f) = ipc::memory_list(&id).await {
                files.set(f);
            }
            if let Ok(a) = ipc::memory_autos(&id).await {
                memories.set(a);
            }
        });
    };
    // The list follows what the CLI reports (context, memory dialog) and the folder.
    Effect::new(move |_| {
        session.heavy.with(|h| (h.context.is_some(), h.memory_dialog.clone()));
        cwd.track();
        reload_lists();
    });
    // get_memory_dialog again after each turn, as ContextPanel does get_context_usage: the model saves memories then.
    let asked_for = StoredValue::new(None::<Option<Time>>);
    Effect::new(move |_| {
        let (ready, at) = session.meta.with(|x| (x.has_process && x.status == SessionStatus::Idle, x.last_result_at));
        if !ready || asked_for.get_value() == Some(at) {
            return;
        }
        asked_for.set_value(Some(at));
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_refresh_memory(&id).await {
                leptos::logging::error!("[{id}] get_memory_dialog : {e}");
            }
        });
    });

    // Reads the open file into `loaded` (None = absent); a failure shows why.
    let load = move || async move {
        error.set(None);
        let Some(open) = m.with_untracked(|x| x.open.clone()) else { return };
        let got = ipc::memory_read(&session.id(), &open).await;
        match got {
            Ok(text) => m.update(|x| x.loaded = text),
            Err(e) => {
                m.update(|x| x.loaded = None);
                error.set(Some(t!("Mem.ReadFailed", e)));
            }
        }
    };
    // Back from an unmount: a file only viewed is re-read (it may have changed), an edit keeps its baseline and draft.
    if m.with_untracked(|x| x.open.is_some() && !x.editing) {
        spawn_local(load());
    }

    let open = move |path: String| {
        // Re-clicking the open row closes it; an unsaved draft goes with it, as with Cancel.
        m.update(|x| {
            x.open = if x.open.as_deref() == Some(path.as_str()) { None } else { Some(path) };
            x.editing = false;
            x.conflict = false;
        });
        saved.set(false);
        delete_armed.set(None);
        if m.with_untracked(|x| x.open.is_some()) {
            spawn_local(load());
        }
    };
    let edit = move || {
        m.update(|x| {
            x.draft = Some(x.loaded.clone().unwrap_or_default());
            x.editing = true;
        });
        saved.set(false);
        focus_area();
    };
    // Back to the view of what is on disk now, not of the baseline the edit started from.
    let cancel = move || {
        m.update(|x| {
            x.editing = false;
            x.conflict = false;
            x.draft = None;
        });
        spawn_local(load());
    };
    // Discard the draft for what is on disk now.
    let reload = move || {
        spawn_local(async move {
            load().await;
            m.update(|x| {
                x.draft = Some(x.loaded.clone().unwrap_or_default());
                x.conflict = false;
            });
            focus_area();
        });
    };
    // overwrite: the user saw the conflict and keeps their version, so the baseline becomes what is on disk now.
    let save = move |overwrite: bool| {
        let Some(path) = m.with_untracked(|x| x.open.clone()) else { return };
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let id = session.id();
        spawn_local(async move {
            let res = async {
                if overwrite {
                    let now = ipc::memory_read(&id, &path).await?;
                    m.update(|x| x.loaded = now);
                }
                let (loaded, draft) = m.with_untracked(|x| (x.loaded.clone(), x.draft.clone().unwrap_or_default()));
                ipc::memory_save(&id, &path, loaded.as_deref(), &draft).await
            }
            .await;
            match res {
                Ok(MemorySave::Saved) => {
                    load().await;
                    m.update(|x| {
                        x.editing = false;
                        x.conflict = false;
                        x.draft = None;
                    });
                    saved.set(true);
                    reload_lists();
                }
                Ok(MemorySave::Conflict) => m.update(|x| x.conflict = true),
                Ok(MemorySave::NotAllowed) => error.set(Some(t!("Mem.NotAllowed"))),
                Err(e) => error.set(Some(t!("Mem.SaveFailed", e))),
            }
            busy.set(false);
        });
    };
    // Two clicks: the first arms the button, which then says what it will do.
    let delete = move |path: String| {
        if delete_armed.get_untracked().as_deref() != Some(path.as_str()) {
            delete_armed.set(Some(path));
            return;
        }
        delete_armed.set(None);
        let id = session.id();
        spawn_local(async move {
            match ipc::memory_delete(&id, &path).await {
                Ok(false) => error.set(Some(t!("Mem.NotAllowed"))),
                Ok(true) => {
                    m.update(|x| {
                        x.open = None;
                        x.editing = false;
                        x.loaded = None;
                    });
                    if let Err(e) = ipc::session_refresh_memory(&id).await {
                        error.set(Some(t!("Mem.DeleteFailed", e)));
                    }
                }
                Err(e) => error.set(Some(t!("Mem.DeleteFailed", e))),
            }
        });
    };
    let set_auto = move |on: bool| {
        busy.set(true);
        error.set(None);
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_set_auto_memory(&id, on).await {
                error.set(Some(t!("Mem.AutoFailed", e)));
            }
            busy.set(false);
        });
    };

    let home2 = home.clone();
    let file_rows = move || {
        let has_context = session.heavy.with(|h| h.context.is_some());
        let cur = open_m.get();
        let c = cwd.get();
        files
            .get()
            .into_iter()
            .map(|f| {
                let on = cur.as_deref() == Some(f.path.as_str());
                let path = f.path.clone();
                view! {
                    <li>
                        <button type="button" class=on.then_some("on") title=f.path.clone() aria-expanded=if on { "true" } else { "false" }
                            aria-controls=on.then_some("mem-file") on:click=move |_| open(path.clone())>
                            <span class="n">{memory::display(&f.path, &c, &home2)}</span>
                            <span class="tag">{i18n::name("Mem.Type.", Some(&f.kind))}</span>
                            <span class="c">{file_note(&f, has_context)}</span>
                        </button>
                    </li>
                }
            })
            .collect_view()
    };

    let home3 = home.clone();
    let auto = move || {
        let md = session.heavy.with(|h| h.memory_dialog.clone())?;
        let am = json::prop(&md, "auto_memory")?.clone();
        let on = json::bool(&am, "enabled");
        let toggleable = json::bool(&am, "toggleable");
        let folder = memory::auto_folder(&md).map(|f| {
            let shown = fmt::home(&f, &home3);
            view! { <p class="why" title=f>{t!("Mem.AutoFolder", shown)}</p> }
        });
        let list = move || {
            let ms = memories.get();
            if ms.is_empty() {
                return on.then(|| view! { <p class="why">{t!("Mem.AutoNone")}</p> }.into_any());
            }
            let cur = open_m.get();
            let n = ms.len();
            let rows = ms
                .into_iter()
                .map(|a| {
                    let mon = cur.as_deref() == Some(a.path.as_str());
                    let path = a.path.clone();
                    view! {
                        <li>
                            <button type="button" class=mon.then_some("on") title=a.description.clone().unwrap_or_else(|| a.path.clone())
                                aria-expanded=if mon { "true" } else { "false" } aria-controls=mon.then_some("mem-file") on:click=move |_| open(path.clone())>
                                <span class="n">{a.name.clone()}</span>
                                {a.kind.clone().map(|k| view! { <span class="tag">{k}</span> })}
                                <span class="c">{a.description.clone()}</span>
                            </button>
                        </li>
                    }
                })
                .collect_view();
            Some(view! {
                <span class="cap" id="automem-cap">{t!("Mem.AutoList", n)}</span>
                <ul class="mem" aria-labelledby="automem-cap">{rows}</ul>
            }
            .into_any())
        };
        Some(view! {
            <div class="acts">
                <button type="button" class=if on { "toggle on" } else { "toggle" } role="switch" aria-checked=if on { "true" } else { "false" }
                    title=json::string(&am, "status") disabled=move || busy.get() || !toggleable on:click=move |_| set_auto(!on)>
                    <span class="sw"></span>{t!("Mem.Auto")}
                </button>
            </div>
            {move || open_m.with(Option::is_none).then(|| error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> })).flatten()}
            {folder}
            {list}
        })
    };

    let viewer = move || {
        let open = open_m.get()?;
        let name = memory::display(&open, &cwd.get(), &home);
        let editing = editing_m.get();
        let body = if editing {
            let n2 = name.clone();
            view! {
                <label class="sr-only" for="mem-text">{t!("Mem.EditLabel", name)}</label>
                // oninput: every keystroke reaches the session store, so an unmount loses nothing.
                <div class="input"><textarea id="mem-text" node_ref=area rows="14" spellcheck="false"
                    prop:value=move || m.with(|x| x.draft.clone().unwrap_or_default())
                    on:input=move |e| {
                        let v = event_target_value(&e);
                        m.update(|x| x.draft = Some(v));
                    }></textarea></div>
                <div class="acts">
                    <button type="button" class="btn primary" disabled=busy on:click=move |_| save(false)>{t!("Mem.Save")}</button>
                    <button type="button" class="btn" on:click=move |_| cancel()>{t!("Action.Cancel")}</button>
                </div>
                {move || conflict_m.get().then(|| view! {
                    <div class="errbox" role="alert">{t!("Mem.Conflict", n2)}</div>
                    <div class="acts">
                        <button type="button" class="btn" on:click=move |_| reload()>{t!("Mem.Reload")}</button>
                        <button type="button" class="btn danger" disabled=busy on:click=move |_| save(true)>{t!("Mem.Overwrite")}</button>
                    </div>
                })}
            }
            .into_any()
        } else {
            let open2 = open.clone();
            (move || {
                if error.with(Option::is_some) {
                    return None;
                }
                let loaded = loaded_m.get();
                let content = match &loaded {
                    None => view! { <p>{t!("Mem.Missing")}</p> }.into_any(),
                    Some(l) if l.trim().is_empty() => view! { <p>{t!("Mem.Empty")}</p> }.into_any(),
                    Some(l) => view! { <Markdown text=l.clone() /> }.into_any(),
                };
                let deletable = loaded.is_some() && memories.with(|ms| ms.iter().any(|a| a.path == open2));
                let del = deletable.then(|| {
                    let (p, p2) = (open2.clone(), open2.clone());
                    view! {
                        <button type="button" class="btn danger" disabled=busy on:click=move |_| delete(p.clone())>
                            {move || if delete_armed.get().as_deref() == Some(p2.as_str()) { t!("Mem.DeleteConfirm", file_name(&p2)) } else { t!("Mem.Delete") }}
                        </button>
                    }
                });
                let k = if loaded.is_none() { "Mem.Create" } else { "Mem.Edit" };
                Some(view! {
                    {content}
                    <div class="acts"><button type="button" class="btn" on:click=move |_| edit()>{t!(k)}</button>{del}</div>
                })
            })
            .into_any()
        };
        Some(view! {
            <div id="mem-file" class="ctx">
                {body}
                {move || error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> })}
                <div role="status">{move || saved.get().then(|| view! { <p>{t!("Mem.Saved")}</p> })}</div>
            </div>
        })
    };

    view! {
        <div class="section last">
            <span class="cap" id="mem-cap">{t!("Mem.Title")}</span>
            <ul class="mem" aria-labelledby="mem-cap">{file_rows}</ul>
            <p class="why">{t!("Mem.Hint")}</p>
            {auto}
            {viewer}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes() {
        i18n::set_thread_lang(Some(i18n::Lang::En));
        let f = |tokens, exists| MemoryFile { path: "/w/CLAUDE.md".into(), kind: "Project".into(), tokens, exists };
        assert_eq!(file_note(&f(Some(1200), true), true), "1k tokens");
        assert_eq!(file_note(&f(None, false), false), "not created");
        assert_eq!(file_note(&f(None, true), false), "");
        assert_eq!(file_note(&f(None, true), true), "not loaded yet");
        assert_eq!(file_name("/a/b/note.md"), "note.md");
        i18n::set_thread_lang(None);
    }
}
