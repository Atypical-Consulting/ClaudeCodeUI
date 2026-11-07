//! Port of `Core/UserSettings.cs` (default model of new sessions) — spec 03 §6. Owner: core-store-sessions.
//! `model` in `config_dir()/settings.json`, which `update_settings` refuses to write. Atomic: write
//! `settings.json.ccui-tmp`, rename over. A file that does not parse is left alone (Err says why).

use crate::CoreResult;
use std::path::PathBuf;

pub fn file() -> PathBuf {
    todo!("port: UserSettings.File")
}

pub fn set_model(model: &str) -> CoreResult<()> {
    todo!("port: UserSettings.SetModel")
}

/// Every other key kept (order too), "default" removes the key; indented. Not an object → Err.
pub fn with_model(json: &str, model: &str) -> CoreResult<String> {
    todo!("port: UserSettings.WithModel")
}
