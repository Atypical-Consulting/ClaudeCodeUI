// Desktop shell: starts the bundled self-contained Blazor server on a loopback port it picks itself, shows the
// splash (src/index.html) until the server prints its tokened URL on stdout, then points the window at it. The server watches
// --parent-pid and shuts itself down (killing its claude children) when this process exits.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::webview::NewWindowResponse;
use tauri::{Manager, Url, WebviewWindowBuilder};

// Origin of our own server, known once it prints its URL. The window only ever shows it or the bundled splash.
static SERVER: OnceLock<String> = OnceLock::new();

fn main() {
    tauri::Builder::default()
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
                // The URL comes from our own child (it binds port 0), so no other listener can be mistaken for it.
                let mut lines = BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok);
                match lines.find(|l| l.starts_with("http://127.0.0.1:")).and_then(|u| u.parse::<Url>().ok()) {
                    Some(url) => {
                        let _ = SERVER.set(url.origin().ascii_serialization());
                        let _ = window.navigate(url);
                    }
                    None => return fail(code(child.wait()), false),
                }
                // Keep draining stdout so the server's console logging never blocks; echoed for a terminal launch.
                lines.for_each(|l| drop(writeln!(std::io::stdout(), "{l}")));
                // stdout closed: the server is gone. Say so on the splash instead of Blazor's endless reconnect.
                let status = code(child.wait());
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
