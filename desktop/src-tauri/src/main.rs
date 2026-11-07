// Desktop app: the Leptos UI (ui/, built by trunk) runs in the webview and talks to the backend (ccui-core) over
// Tauri commands and events, all in this one process. No server, no sidecar, no token.
// Updates: tauri-plugin-updater, checked silently once at startup, from the app menu, or from the UI (check_for_updates).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod sink;

use ccui_shared::push::Push;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, MenuItemKind, Submenu};
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Emitter, Manager, Url, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

// A check is on screen (one at a time).
static CHECKING: AtomicBool = AtomicBool::new(false);
// A manual request made while a check runs: that check then answers it (up to date, errors) instead of staying silent.
static WANT_MANUAL: AtomicBool = AtomicBool::new(false);
const MENU_CHECK: &str = "check-for-updates";

fn main() {
    // GUI apps on macOS (and most Linux launchers) don't inherit the shell PATH, so `claude` would not be found.
    // Before any thread exists: set_var is only sound single-threaded.
    #[cfg(unix)]
    if let Some(path) = login_path() {
        std::env::set_var("PATH", path);
    }
    // Developer entry point, before any window: `--boot-probe <dir> [seconds=90]` (cold-start stall repro).
    let argv: Vec<String> = std::env::args().collect();
    if let Some(i) = argv.iter().position(|a| a == "--boot-probe") {
        let dir = argv.get(i + 1).cloned().unwrap_or_else(|| ".".into());
        let secs = argv.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(90);
        let code = tauri::async_runtime::block_on(ccui_core::boot_probe::run(&dir, secs));
        std::process::exit(code);
    }
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(not(target_os = "macos"))]
    let builder = builder.plugin(tauri_plugin_notification::init());
    let app = commands::register(builder)
        .menu(|app| {
            let check = MenuItem::with_id(app, MENU_CHECK, t("Check for Updates…", "Rechercher des mises à jour…"), true, None::<&str>)?;
            // macOS: in the app menu, under About. Elsewhere there is no default menu bar: a Help menu holds it.
            if cfg!(target_os = "macos") {
                let menu = Menu::default(app)?;
                if let Some(MenuItemKind::Submenu(m)) = menu.items()?.first() {
                    m.insert(&check, 1)?;
                }
                Ok(menu)
            } else {
                Menu::with_items(app, &[&Submenu::with_items(app, t("Help", "Aide"), true, &[&check])?])
            }
        })
        .on_menu_event(|app, e| {
            if e.id() == MENU_CHECK {
                check_for_updates(app.clone(), true);
            }
        })
        .setup(|app| {
            // Backend state: every claude process, transcript, worktree… (ccui-core), pushing to the UI through the sink.
            let locale = sys_locale::get_locale();
            let core = ccui_core::Core::new(Arc::new(sink::TauriSink::new(app.handle().clone())), locale.clone());
            app.manage(core);
            // Theme, code size, pane widths and language applied before first paint, and BootInfo for the UI.
            let boot = ccui_core::app_flag::boot_info(&app.package_info().version.to_string(), locale.as_deref());
            // Links in model output must not replace the app: anything that is not the bundled UI opens in the system browser.
            WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?
                .initialization_script(ccui_core::app_flag::boot_script(&boot))
                .on_navigation(|url| {
                    let ours = is_ours(url);
                    if !ours {
                        open_external(url);
                    }
                    ours
                })
                .on_new_window(|url, _| {
                    open_external(&url);
                    NewWindowResponse::Deny
                })
                .build()?;
            check_for_updates(app.handle().clone(), false);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Claude Code UI");
    app.run(|app, event| {
        // Window closed / app quitting: claude children die with us.
        if let tauri::RunEvent::Exit = event {
            commands::shutdown(app);
        }
    });
}

fn is_ours(url: &Url) -> bool {
    url.scheme() == "tauri"
        || url.host_str() == Some("tauri.localhost")
        || (cfg!(debug_assertions) && url.host_str() == Some("localhost") && url.port() == Some(1420)) // trunk serve
}

// The shell's few words follow the OS language, like the UI's default.
pub fn t(en: &'static str, fr: &'static str) -> &'static str {
    static FR: OnceLock<bool> = OnceLock::new();
    if *FR.get_or_init(|| sys_locale::get_locale().is_some_and(|l| l.to_ascii_lowercase().starts_with("fr"))) {
        fr
    } else {
        en
    }
}

// Silent (manual = false): speaks only when an update exists; network errors stay quiet.
// Manual: also reports "up to date" and errors. Runs on its own thread: the native dialogs block it, not the app.
pub fn check_for_updates(app: AppHandle, manual: bool) {
    if CHECKING.swap(true, Ordering::SeqCst) {
        // One check at a time: a second one would stack dialogs. An explicit request is still answered, by the running check.
        if manual {
            WANT_MANUAL.store(true, Ordering::SeqCst);
            app.dialog()
                .message(t("An update check is already in progress.", "Une recherche de mises à jour est déjà en cours."))
                .title(t("Check for Updates", "Rechercher des mises à jour"))
                .kind(MessageDialogKind::Info)
                .show(|_| {});
        }
        return;
    }
    std::thread::spawn(move || {
        if update_flow(&app, manual) {
            app.restart();
        }
        WANT_MANUAL.store(false, Ordering::SeqCst);
        CHECKING.store(false, Ordering::SeqCst);
    });
}

// True when the app must restart (an update was installed, or sessions were stopped for one that then failed).
fn update_flow(app: &AppHandle, manual: bool) -> bool {
    let dialog = |title: &str, text: String, kind| app.dialog().message(text).title(title).kind(kind).blocking_show();
    let failed = |text: String| dialog(t("The update could not be installed", "La mise à jour n’a pas pu être installée"), text, MessageDialogKind::Error);
    // A time limit on the check only: the download below may legitimately take longer.
    let found = app.updater_builder().timeout(Duration::from_secs(30)).build().map(|u| tauri::async_runtime::block_on(u.check()));
    let manual = manual || WANT_MANUAL.swap(false, Ordering::SeqCst);
    let mut update = match found {
        Ok(Ok(Some(u))) => u,
        Ok(Ok(None)) => {
            if manual {
                dialog(
                    t("You’re up to date", "Tu es à jour"),
                    t("Claude Code UI {v} is the latest version.", "Claude Code UI {v} est la dernière version.").replace("{v}", &app.package_info().version.to_string()),
                    MessageDialogKind::Info,
                );
            }
            return false;
        }
        // A 404 on latest.json: a release was just published and its manifest is not uploaded yet (the checksums job
        // runs after every installer is built). Not an error for the user.
        Ok(Err(tauri_plugin_updater::Error::ReleaseNotFound)) => {
            if manual {
                dialog(
                    t("No update information yet", "Pas encore d’information de mise à jour"),
                    t(
                        "Update information isn’t available yet. Try again in a few minutes.",
                        "L’information de mise à jour n’est pas encore disponible. Réessaie dans quelques minutes.",
                    )
                    .into(),
                    MessageDialogKind::Info,
                );
            }
            return false;
        }
        Ok(Err(e)) | Err(e) => {
            if manual {
                dialog(
                    t("Could not check for updates", "Impossible de vérifier les mises à jour"),
                    format!("{e}\n\n{}", t("Check your connection and try again.", "Vérifie ta connexion puis réessaie.")),
                    MessageDialogKind::Error,
                );
            }
            return false;
        }
    };
    update.timeout = None;
    let mut text = t("Version {new} is available (you have {cur}).", "La version {new} est disponible (tu as la {cur}).")
        .replace("{new}", &update.version)
        .replace("{cur}", &update.current_version);
    if let Some(notes) = update.body.as_deref().map(excerpt).filter(|n| !n.is_empty()) {
        text = format!("{text}\n\n{notes}");
    }
    let install = app
        .dialog()
        .message(text)
        .title(t("Update available", "Mise à jour disponible"))
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(t("Install and restart", "Installer et redémarrer").into(), t("Later", "Plus tard").into()))
        .blocking_show();
    if !install {
        return false;
    }
    // Download while the app keeps working; the title bar shows the progress.
    let window = app.get_webview_window("main");
    let title = |s: &str| drop(window.as_ref().map(|w| w.set_title(s)));
    let (mut got, mut shown) = (0u64, u64::MAX);
    let bytes = tauri::async_runtime::block_on(update.download(
        |n, total| {
            got += n as u64;
            if let Some(pct) = total.filter(|&t| t > 0).map(|t| got * 100 / t).filter(|&p| p != shown) {
                shown = pct;
                title(&t("Claude Code UI — downloading the update ({p} %)", "Claude Code UI — téléchargement de la mise à jour ({p} %)").replace("{p}", &pct.to_string()));
            }
        },
        || {},
    ));
    title(&app.package_info().name);
    let bytes = match bytes {
        Ok(b) => b,
        Err(e) => {
            failed(e.to_string());
            return false;
        }
    };
    // claude children must be gone before files are replaced (and the passive Windows installer kills this process).
    let _ = app.emit(ccui_shared::push::PUSH_EVENT, Push::Updating);
    commands::shutdown(app);
    // Windows: the installer (passive) takes over and this process exits inside install. macOS/Linux: files replaced.
    if let Err(e) = update.install(bytes) {
        failed(format!("{e}\n\n{}", t("The app will restart on the current version.", "L’application va redémarrer sur la version actuelle.")));
    }
    true
}

// The first entries of release-please notes, without headings, commit/issue links or bold markers.
fn excerpt(notes: &str) -> String {
    let lines: Vec<String> = notes
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split(" ([").next().unwrap_or(l).replace("**", ""))
        .collect();
    let mut out = lines.iter().take(6).cloned().collect::<Vec<_>>().join("\n");
    if lines.len() > 6 {
        out.push_str("\n…");
    }
    out
}

pub fn open_external(url: &Url) {
    if !matches!(url.scheme(), "http" | "https" | "mailto") {
        return;
    }
    #[cfg(target_os = "macos")]
    let mut c = Command::new("open");
    #[cfg(windows)]
    let mut c = Command::new("rundll32");
    #[cfg(windows)]
    c.arg("url.dll,FileProtocolHandler");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut c = Command::new("xdg-open");
    c.arg(url.as_str()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    std::thread::spawn(move || c.spawn().and_then(|mut p| p.wait())); // waited, so no zombie
}

// Posts one notification for a session; a click brings the window up and tells the UI to open that session.
// ccui-core (notifications observer) only asks while the window is in the background: macOS does not present the frontmost
// app's own notifications.
pub fn notify(app: &AppHandle, session_id: String, title: String, body: String) {
    let Some(window) = app.get_webview_window("main") else { return };
    // A thread per notification: on macOS it waits for the click.
    std::thread::spawn(move || {
        if post_notification(&window, &title, &body) {
            // set_focus alone is a no-op on a minimized or hidden window (tao, macOS): restore it first.
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.emit(ccui_shared::push::PUSH_EVENT, Push::Navigate { path: format!("session/{session_id}") });
        }
    });
}

// True once the user clicks it.
#[cfg(target_os = "macos")]
fn post_notification(window: &tauri::WebviewWindow, title: &str, body: &str) -> bool {
    use mac_notification_sys::{set_application, Notification, NotificationResponse};
    // Posted as this app (as Terminal under `tauri dev`, like tauri-plugin-notification); only the first call sets it.
    let _ = set_application(if tauri::is_dev() { "com.apple.Terminal" } else { &window.config().identifier });
    // ponytail: until the click, or until the notification is cleared from Notification Center, this thread parks and the
    // crate polls deliveredNotifications every 0.5 s on the main run loop: N unclicked notifications cost N threads and N
    // timers. The crate has no call to withdraw one; replacing the previous one per session needs that (or UNUserNotificationCenter).
    matches!(Notification::new().title(title).message(body).wait_for_click(true).send(), Ok(NotificationResponse::Click))
}

// Windows / Linux: tauri-plugin-notification posts it but reports no click on desktop.
#[cfg(not(target_os = "macos"))]
fn post_notification(window: &tauri::WebviewWindow, title: &str, body: &str) -> bool {
    use tauri_plugin_notification::NotificationExt;
    let _ = window.app_handle().notification().builder().title(title).body(body).show();
    false
}

// Interactive rc files can be slow or hang: after 3 s the inherited PATH is kept.
#[cfg(unix)]
fn login_path() -> Option<String> {
    use std::io::Read;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut child = Command::new(shell).args(["-ilc", "printenv PATH"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut out = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        let _ = tx.send(s);
    });
    let text = rx.recv_timeout(Duration::from_secs(3));
    if text.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    text.ok()?.lines().rev().map(str::trim).find(|l| l.contains('/')).map(String::from)
}

#[cfg(test)]
mod tests {
    #[test]
    fn excerpt_strips_release_please_noise() {
        let notes = "## 0.2.0 (2026-10-11)\n\n\n### Features\n\n* **desktop:** check for updates ([#71](https://x/71)) ([abc1234](https://x/c))\n* a\n* b\n* c\n* d\n* e\n* f";
        assert_eq!(super::excerpt(notes), "* desktop: check for updates\n* a\n* b\n* c\n* d\n* e\n…");
        assert_eq!(super::excerpt(""), "");
    }
}
