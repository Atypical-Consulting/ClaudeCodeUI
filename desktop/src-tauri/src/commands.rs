//! Tauri commands: thin wrappers over ccui-core. Each area adds its commands to `register`.
use tauri::{AppHandle, Builder, Wry};

#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn check_for_updates(app: AppHandle) {
    crate::check_for_updates(app, true);
}

pub fn register(builder: Builder<Wry>) -> Builder<Wry> {
    builder.invoke_handler(tauri::generate_handler![app_version, check_for_updates])
}

/// Stops every claude process (app exit, before an update install).
pub fn shutdown(_app: &AppHandle) {}
