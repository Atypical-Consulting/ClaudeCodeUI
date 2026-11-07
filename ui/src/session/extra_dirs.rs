//! Port of `Components/Session/ExtraDirs.razor` — spec 07 §20. Owner: ui-session-inspector.
//! Additional folders (--add-dir) and the field that adds one: an absolute path that exists on this machine. The
//! parent renders the `<label for={id}>`. Without on_remove the list is read-only: nothing verified takes a folder
//! back from a running claude.

use crate::common::icon::Icon;
use crate::common::pick_folder::PickFolder;
use crate::ipc;
use crate::state::use_app;
use ccui_shared::{t, text};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

/// A local (non-Send) future, as the wasm UI produces them.
pub type LocalFut<T> = Pin<Box<dyn Future<Output = T>>>;

/// `OnAdd(d)` (async, may fail): Ok = added (ExtraDirs clears its draft and confirm step), Err(msg) = shown as
/// `Dirs.AddFailed(msg)`. NewSession adds to its list (always Ok); ContextPanel calls `session_add_dir`.
pub type AddDir = Callback<String, LocalFut<Result<(), String>>>;

/// The parent's handle on an ExtraDirs (NewSession: "a typed folder is not dropped" — its Start awaits
/// `commit()` before starting). ExtraDirs installs its commit closure on mount.
#[derive(Clone, Copy)]
pub struct ExtraDirsCtl {
    commit: StoredValue<Option<Rc<dyn Fn() -> LocalFut<bool>>>, LocalStorage>,
}

impl ExtraDirsCtl {
    pub fn new() -> ExtraDirsCtl {
        ExtraDirsCtl { commit: StoredValue::new_local(None) }
    }

    /// `Commit()`: draft blank → true; else run Add and return whether the draft is now empty (false = the reason
    /// is shown). True when no ExtraDirs is mounted.
    pub async fn commit(&self) -> bool {
        match self.commit.try_with_value(|c| c.clone()).flatten() {
            Some(f) => f().await,
            None => true,
        }
    }

    /// Called by ExtraDirs on mount (and cleared on cleanup).
    pub fn install(&self, commit: Option<Rc<dyn Fn() -> LocalFut<bool>>>) {
        self.commit.try_update_value(|c| *c = commit);
    }
}

impl Default for ExtraDirsCtl {
    fn default() -> Self {
        ExtraDirsCtl::new()
    }
}

/// `--add-dir` list + input; used by NewSession and ContextPanel. Add: `ipc::validate_extra_dir(draft, cwd, dirs)`
/// (Err = the message to show), then the `confirm` step, then busy + `on_add(d).await`.
#[component]
pub fn ExtraDirs(
    #[prop(into)] id: String,
    #[prop(into)] dirs: Signal<Vec<String>>,
    #[prop(into)] cwd: Signal<String>,
    on_add: AddDir,
    #[prop(optional)] on_remove: Option<Callback<String>>,
    #[prop(optional, into)] disabled: Option<Signal<bool>>,
    /// Set when the session's mode edits unasked: an extra folder is never isolated, so the first Add shows this and
    /// the second adds.
    #[prop(optional, into)]
    confirm: Option<String>,
    #[prop(optional)] ctl: Option<ExtraDirsCtl>,
) -> impl IntoView {
    let boot = use_app().boot;
    let draft = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let confirmed = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let has_confirm = confirm.is_some();
    let dis = move || disabled.is_some_and(|d| d.get());

    let add = move || -> LocalFut<()> {
        Box::pin(async move {
            let raw = draft.get_untracked();
            if busy.get_untracked() || disabled.is_some_and(|d| d.get_untracked()) || raw.trim().is_empty() {
                return;
            }
            error.set(None);
            let d = match ipc::validate_extra_dir(&raw, &cwd.get_untracked(), &dirs.get_untracked()).await {
                Ok(d) => d,
                Err(m) => {
                    error.set(Some(m));
                    return;
                }
            };
            if draft.is_disposed() {
                return;
            }
            if has_confirm && confirmed.get_untracked().as_deref() != Some(d.as_str()) {
                confirmed.set(Some(d));
                return;
            }
            busy.set(true);
            match on_add.run(d).await {
                Ok(()) => {
                    draft.set(String::new());
                    confirmed.set(None);
                }
                Err(e) => error.set(Some(t!("Dirs.AddFailed", e))),
            }
            busy.set(false);
        })
    };
    let edited = move || {
        error.set(None);
        confirmed.set(None);
    };

    // Adds a folder typed but not yet added (Start calls it, so it is not dropped); false, the reason shown, when it can't be.
    if let Some(ctl) = ctl {
        ctl.install(Some(Rc::new(move || -> LocalFut<bool> {
            Box::pin(async move {
                if draft.get_untracked().trim().is_empty() {
                    return true;
                }
                add().await;
                draft.try_with_untracked(String::is_empty).unwrap_or(false)
            })
        })));
        on_cleanup(move || ctl.install(None));
    }

    // A picked folder goes through the same checks as a typed one.
    let picked = Callback::new(move |d: String| {
        draft.set(d);
        edited();
        spawn_local(add());
    });

    let list = move || {
        let ds = dirs.get();
        (!ds.is_empty()).then(|| {
            let (home, sep) = boot.with_value(|b| (b.home.clone(), b.sep.clone()));
            let items = ds
                .into_iter()
                .map(|d| {
                    let chip = text::chip(&d, &home, &sep);
                    let remove = on_remove.map(|cb| {
                        let label = t!("Dirs.Remove", d);
                        let d = d.clone();
                        view! {
                            <button type="button" class="x" aria-label=label.clone() title=label on:click=move |_| cb.run(d.clone())>
                                <Icon name="x" sm=true />
                            </button>
                        }
                    });
                    view! { <li class="chip" title=d><Icon name="folder" sm=true />{chip}{remove}</li> }
                })
                .collect_view();
            view! { <ul class="dirs" aria-label=t!("Dirs.Label")>{items}</ul> }
        })
    };
    let err_id = format!("{id}-err");
    let described = {
        let err_id = err_id.clone();
        move || (error.with(Option::is_some) || confirmed.with(Option::is_some)).then(|| err_id.clone())
    };
    let msg = move || {
        if let Some(e) = error.get() {
            Some(view! { <div id=err_id.clone() class="fmsg" role="alert">{e}</div> }.into_any())
        } else if confirmed.with(Option::is_some) {
            Some(view! { <div id=err_id.clone() class="fmsg risk" role="alert">{confirm.clone()}</div> }.into_any())
        } else {
            None
        }
    };

    view! {
        {list}
        <div class="input">
            <PickFolder start=Signal::derive(move || draft.get().trim().to_string()) disabled=Signal::derive(dis)
                on_pick=picked on_error=Callback::new(move |m| error.set(Some(m))) />
            <input id=id prop:value=draft placeholder=t!("Dirs.Placeholder") disabled=dis
                aria-invalid=move || error.with(Option::is_some).then_some("true") aria-describedby=described
                spellcheck="false" autocomplete="off"
                on:input=move |e| {
                    draft.set(event_target_value(&e));
                    edited();
                }
                on:keydown=move |e: web_sys::KeyboardEvent| {
                    if e.key() == "Enter" && !e.ctrl_key() && !e.meta_key() {
                        spawn_local(add());
                    }
                } />
            <button type="button" class="b btn ghost" disabled=move || dis() || busy.get() || draft.with(|d| d.trim().is_empty())
                on:click=move |_| spawn_local(add())>
                <Icon name="plus" sm=true />{move || t!(if confirmed.with(Option::is_some) { "Dirs.AddAnyway" } else { "Dirs.Add" })}
            </button>
        </div>
        {msg}
    }
}
