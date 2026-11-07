//! Leptos CSR entry. Owner: ui-shell.
//! Boot: read `BootInfo` synchronously (injected by the shell before first paint), set the i18n language and the
//! macOS key glyphs, `<body data-copied>` (copy-button label, App.razor), the panic hook (the `#blazor-error-ui`
//! toast), then mount `App`.
#![allow(dead_code, unused_variables, unused_imports)] // skeleton phase: stubs are todo!()

mod app;
mod common;
mod interop;
mod ipc;
mod layout;
mod pages;
mod session;
mod state;

use ccui_shared::i18n::{self, Lang};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        show_error_toast();
    }));
    let boot = ipc::boot_info();
    i18n::set_lang(Lang::from_code(&boot.culture));
    i18n::set_mac(boot.mac);
    if let Some(body) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
        let _ = body.set_attribute("data-copied", &ccui_shared::t!("Copy.Done"));
    }
    leptos::mount::mount_to_body(move || leptos::view! { <app::App boot=boot/> });
}

/// What blazor.web.js did on a circuit error: show `#blazor-error-ui`, `.reload` reloads the webview, `.dismiss`
/// hides it. Wired here (not by Leptos) because the reactive runtime may be gone after a panic; the listeners are
/// plain closures that only touch the DOM. ponytail: they still call into the wasm instance after the trap — fine
/// in practice; move them to a static interop.js if a panic ever leaves the toast's buttons dead.
fn show_error_toast() {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Some(toast) = doc.get_element_by_id("blazor-error-ui").and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()) else {
        return;
    };
    let _ = toast.style().set_property("display", "block");
    if toast.get_attribute("data-wired").is_some() {
        return;
    }
    let _ = toast.set_attribute("data-wired", "");
    if let Ok(Some(reload)) = toast.query_selector(".reload") {
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| {
            e.prevent_default();
            if let Some(w) = web_sys::window() {
                let _ = w.location().reload();
            }
        });
        let _ = reload.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    if let Ok(Some(dismiss)) = toast.query_selector(".dismiss") {
        let t = toast.clone();
        let cb = Closure::<dyn FnMut()>::new(move || {
            let _ = t.style().set_property("display", "none");
        });
        let _ = dismiss.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }
}
