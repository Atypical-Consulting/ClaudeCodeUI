//! Leptos CSR entry. Owner: ui-shell.
//! Boot: read `BootInfo` synchronously (injected by the shell before first paint), set the i18n language and the
//! macOS key glyphs, then mount `App`.
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

fn main() {
    console_error_panic_hook::set_once();
    let boot = ipc::boot_info();
    i18n::set_lang(Lang::from_code(&boot.culture));
    i18n::set_mac(boot.mac);
    leptos::mount::mount_to_body(move || leptos::view! { <app::App boot=boot/> });
}
