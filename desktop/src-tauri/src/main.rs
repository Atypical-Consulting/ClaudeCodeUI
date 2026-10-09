// Desktop shell: starts the bundled self-contained Blazor server on a loopback port it picks itself, shows the
// splash (src/index.html) until the server prints its tokened URL on stdout, then points the window at it. The server watches
// --parent-pid and shuts itself down (killing its claude children) when this process exits.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let exe = app.path().resource_dir()?.join("server").join(if cfg!(windows) { "ClaudeCodeUI.exe" } else { "ClaudeCodeUI" });
            let mut cmd = Command::new(&exe);
            cmd.args(["--desktop-port", "0", "--parent-pid", &std::process::id().to_string()])
                .current_dir(app.path().home_dir()?) // default working folder offered on the start page
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            #[cfg(unix)]
            if let Some(path) = login_path() {
                cmd.env("PATH", path);
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash for the server or claude
            }
            let window = app.get_webview_window("main").unwrap();
            std::thread::spawn(move || {
                let fail = |why: String| {
                    std::thread::sleep(Duration::from_millis(500)); // let the splash finish loading
                    let _ = window.eval(&format!("fail({why:?})")); // index.html words it in the OS language
                };
                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => return fail(format!("{} ({e})", exe.display())),
                };
                // The URL comes from our own child (it binds port 0), so no other listener can be mistaken for it.
                let mut lines = BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok);
                match lines.find(|l| l.starts_with("http://127.0.0.1:")).and_then(|u| u.parse().ok()) {
                    Some(url) => {
                        let _ = window.navigate(url);
                    }
                    None => return fail(child.wait().map_or_else(|e| e.to_string(), |s| s.to_string())),
                }
                lines.for_each(drop); // keep draining stdout so the server's console logging never blocks
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Code UI");
}

// GUI apps on macOS (and most Linux launchers) don't inherit the shell PATH, so `claude` would not be found.
#[cfg(unix)]
fn login_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = Command::new(shell).args(["-ilc", "printenv PATH"]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    text.lines().rev().map(str::trim).find(|l| l.contains('/')).map(String::from)
}
