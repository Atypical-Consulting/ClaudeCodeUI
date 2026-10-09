// Desktop shell: starts the bundled self-contained Blazor server on a free loopback port, shows the
// splash (src/index.html) until the port answers, then points the window at it. The server watches
// --parent-pid and shuts itself down (killing its claude children) when this process exits.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // ponytail: the port is free when probed, not reserved; a race with another binder only shows the error splash
            let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
            let exe = app.path().resource_dir()?.join("server").join(if cfg!(windows) { "ClaudeCodeUI.exe" } else { "ClaudeCodeUI" });
            let mut cmd = Command::new(&exe);
            cmd.args(["--desktop-port", &port.to_string(), "--parent-pid", &std::process::id().to_string()])
                .current_dir(app.path().home_dir()?) // default working folder offered on the start page
                .stdin(Stdio::null())
                .stdout(Stdio::null())
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
                    let msg = format!("Le serveur n'a pas démarré : {why}");
                    let _ = window.eval(&format!("var m=document.getElementById('msg');m.className='err';m.textContent={msg:?}"));
                };
                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => return fail(format!("{} ({e})", exe.display())),
                };
                let addr = SocketAddr::from(([127, 0, 0, 1], port));
                while TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_err() {
                    if let Ok(Some(status)) = child.try_wait() {
                        return fail(status.to_string());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                let _ = window.navigate(format!("http://127.0.0.1:{port}/").parse().unwrap());
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
