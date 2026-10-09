// Desktop shell: starts the bundled self-contained Blazor server on a loopback port it picks itself, shows the
// splash (src/index.html) until the server prints its tokened URL on stdout, then points the window at it. The server watches
// --parent-pid and shuts itself down (killing its claude children) when this process exits.
// Updates: tauri-plugin-updater, checked silently after startup, from the app menu, or when the server prints
// UPDATE_LINE (the UI's "Check for updates" action); see check_for_updates.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, MenuItemKind, Submenu};
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

// Origin of our own server, known once it prints its URL. The window only ever shows it or the bundled splash.
static SERVER: OnceLock<String> = OnceLock::new();
// The server's tokened URL (to ask it to stop before an update), its process, and the splash URL.
static SERVER_URL: OnceLock<Url> = OnceLock::new();
static CHILD: Mutex<Option<Child>> = Mutex::new(None);
static SPLASH: OnceLock<Url> = OnceLock::new();
// A check is on screen (one at a time); an update is being installed (the server stopping is then expected).
static CHECKING: AtomicBool = AtomicBool::new(false);
static UPDATING: AtomicBool = AtomicBool::new(false);
// Printed on stdout by the server when the user picks "Check for updates" in the UI (Core/Shell.cs). The same pipe
// carries the console log, whose lines never equal it: they start with a level or an indent.
const UPDATE_LINE: &str = "ccui:check-for-updates";
const MENU_CHECK: &str = "check-for-updates";

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            let exe = app.path().resource_dir()?.join("server").join(if cfg!(windows) { "ClaudeCodeUI.exe" } else { "ClaudeCodeUI" });
            let mut cmd = Command::new(&exe);
            cmd.args(["--desktop-port", "0", "--parent-pid", &std::process::id().to_string()])
                .current_dir(app.path().home_dir()?) // fallback working folder when there are no recent projects
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit()); // launched from a terminal, a crash is readable there
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash for the server or claude
            }
            // Links in model output must not replace the app: anything else opens in the system browser.
            let window = WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?
                .on_navigation(|url| {
                    let ours = url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost") // the bundled splash
                        || SERVER.get() == Some(&url.origin().ascii_serialization());
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
            let splash = window.url()?;
            let _ = SPLASH.set(splash.clone());
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(5)); // after the server is up; never in the way of startup
                check_for_updates(handle, false);
            });
            std::thread::spawn(move || {
                // Here, not in setup: the splash is already painted while the login shell runs.
                #[cfg(unix)]
                if let Some(path) = login_path() {
                    cmd.env("PATH", path);
                }
                let fail = |why: String, stopped: bool| {
                    std::thread::sleep(Duration::from_millis(500)); // let the splash finish loading
                    let _ = window.eval(&format!("fail({why:?}, {stopped})")); // index.html words it in the OS language
                };
                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => return fail(format!("{} ({e})", exe.display()), false),
                };
                let stdout = child.stdout.take().unwrap();
                *CHILD.lock().unwrap() = Some(child);
                let wait = || CHILD.lock().unwrap().as_mut().map_or_else(String::new, |c| code(c.wait()));
                // The URL comes from our own child (it binds port 0), so no other listener can be mistaken for it.
                let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
                match lines.find(|l| l.starts_with("http://127.0.0.1:")).and_then(|u| u.parse::<Url>().ok()) {
                    Some(url) => {
                        let _ = SERVER.set(url.origin().ascii_serialization());
                        let _ = SERVER_URL.set(url.clone());
                        let _ = window.navigate(url);
                    }
                    None => return fail(wait(), false),
                }
                // Keep draining stdout so the server's console logging never blocks; echoed for a terminal launch.
                for l in lines {
                    if l == UPDATE_LINE {
                        check_for_updates(window.app_handle().clone(), true);
                    } else {
                        let _ = writeln!(std::io::stdout(), "{l}");
                    }
                }
                // stdout closed: the server is gone. Say so on the splash instead of Blazor's endless reconnect,
                // unless it was stopped on purpose to install an update.
                let status = wait();
                if UPDATING.load(Ordering::SeqCst) {
                    return;
                }
                let _ = window.navigate(splash);
                fail(status, true);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Code UI");
}

// The exit code alone ("1"), not Rust's "exit status: 1"; a signal or a wait error keeps its own wording.
fn code(r: std::io::Result<ExitStatus>) -> String {
    match r {
        Ok(s) => s.code().map_or_else(|| s.to_string(), |c| c.to_string()),
        Err(e) => e.to_string(),
    }
}

// The shell's few words follow the OS language, like the splash.
fn t(en: &'static str, fr: &'static str) -> &'static str {
    static FR: OnceLock<bool> = OnceLock::new();
    if *FR.get_or_init(|| sys_locale::get_locale().is_some_and(|l| l.to_ascii_lowercase().starts_with("fr"))) {
        fr
    } else {
        en
    }
}

// Silent (manual = false): speaks only when an update exists; network and server errors stay quiet.
// Manual: also reports "up to date" and errors. Runs on its own thread: the native dialogs block it, not the app.
fn check_for_updates(app: AppHandle, manual: bool) {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return; // one check at a time; a second click would stack dialogs
    }
    std::thread::spawn(move || {
        if update_flow(&app, manual) {
            app.restart();
        }
        CHECKING.store(false, Ordering::SeqCst);
    });
}

// True when the app must restart (an update was installed, or the server was stopped for one that then failed).
fn update_flow(app: &AppHandle, manual: bool) -> bool {
    let dialog = |title: &str, text: String, kind| app.dialog().message(text).title(title).kind(kind).blocking_show();
    let failed = |text: String| dialog(t("The update could not be installed", "La mise à jour n’a pas pu être installée"), text, MessageDialogKind::Error);
    // A time limit on the check only: the download below may legitimately take longer.
    let found = app.updater_builder().timeout(Duration::from_secs(30)).build().map(|u| tauri::async_runtime::block_on(u.check()));
    let mut update = match found {
        Ok(Ok(Some(u))) => u,
        Ok(Ok(None)) => {
            if manual {
                dialog(
                    t("You’re up to date", "Vous êtes à jour"),
                    t("Claude Code UI {v} is the latest version.", "Claude Code UI {v} est la dernière version.").replace("{v}", &app.package_info().version.to_string()),
                    MessageDialogKind::Info,
                );
            }
            return false;
        }
        Ok(Err(e)) | Err(e) => {
            if manual {
                dialog(
                    t("Could not check for updates", "Impossible de vérifier les mises à jour"),
                    format!("{e}\n\n{}", t("Check your connection and try again.", "Vérifiez votre connexion puis réessayez.")),
                    MessageDialogKind::Error,
                );
            }
            return false;
        }
    };
    update.timeout = None;
    let mut text = t("Version {new} is available (you have {cur}).", "La version {new} est disponible (vous avez la {cur}).")
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
    // The bundled server must be gone before its files are replaced (Windows locks a running executable).
    UPDATING.store(true, Ordering::SeqCst);
    if let (Some(w), Some(s)) = (&window, SPLASH.get()) {
        let mut s = s.clone();
        s.set_query(Some("updating"));
        let _ = w.navigate(s);
    }
    stop_server();
    // Windows: the installer (passive) takes over and this process exits inside install. macOS/Linux: files replaced.
    if let Err(e) = update.install(bytes) {
        failed(format!("{e}\n\n{}", t("The app will restart on the current version.", "L’application va redémarrer sur la version actuelle.")));
    }
    true
}

// Stops the server through its own shutdown path (it disposes the sessions, so claude children exit too), then waits
// for the process; one that has not exited after 15 s is killed.
fn stop_server() {
    if let Some(url) = SERVER_URL.get() {
        let addr = format!("127.0.0.1:{}", url.port().unwrap_or(80));
        if let Ok(mut s) = TcpStream::connect(&addr) {
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let req = format!("POST /quit?{} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", url.query().unwrap_or(""));
            if s.write_all(req.as_bytes()).is_ok() {
                let _ = s.read(&mut [0; 64]); // the answer: shutdown has begun
            }
        }
    }
    if let Some(c) = CHILD.lock().unwrap().as_mut() {
        for _ in 0..150 {
            if matches!(c.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = c.kill();
        let _ = c.wait();
    }
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

fn open_external(url: &Url) {
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

// GUI apps on macOS (and most Linux launchers) don't inherit the shell PATH, so `claude` would not be found.
// Interactive rc files can be slow or hang: after 3 s the inherited PATH is kept.
#[cfg(unix)]
fn login_path() -> Option<String> {
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
