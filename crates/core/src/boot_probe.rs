//! Port of `Core/BootProbe.cs` (`--boot-probe <dir> [seconds=90]`) — spec 03 §10. Owner: core-events.
//! Hidden CLI flag of the Tauri binary handled before `tauri::Builder` (see desktop main.rs): spawns claude with the
//! args of a fresh session, sends `initialize`, prints `initialize OK in <ms> ms (<n> models, <n> commands)` (exit 0),
//! `initialize TIMEOUT after <s> s` / `initialize ERROR <msg>` (exit 1); process exit printed as `exited <code>: <text>`.
//! (`--self-check` becomes `cargo test`; `--probe-cli` is not ported.)

/// Run the probe; returns the process exit code.
pub async fn run(dir: &str, seconds: u64) -> i32 {
    todo!("port: BootProbe.Run")
}
