//! Port of `Core/AppFlag.cs` (`AppFlag` + `AppPref`), the culture choice and the boot info — spec 03 §9, 06 §1.
//! Owner: core-store-sessions. Files under `paths::app_data()` (existing users keep their choices):
//! flags = empty file (exists = on); prefs = plain text value (trimmed; outside the whitelist reads None).
//! Names are whitelisted (`ccui_shared::api::{PREF_*, FLAG_*}`): anything else is refused.

use ccui_shared::api::{BootInfo, Flags, Prefs};
use ccui_shared::i18n::Lang;

/// Whether flag file `name` exists (IO errors → false).
pub fn flag(name: &str) -> bool {
    todo!("port: AppFlag.On (get)")
}

/// Create/delete flag file `name`; errors swallowed ("the getter shows what stuck"). Unknown name → Err.
pub fn set_flag(name: &str, on: bool) -> Result<(), String> {
    todo!("port: AppFlag.On (set)")
}

/// Pref value if valid for `name`, else None (never chosen, unreadable, hand-edited outside the list).
pub fn pref(name: &str) -> Option<String> {
    todo!("port: AppPref.Value (get)")
}

/// Set (Some valid value) or delete (None) a pref; returns whether the file changed (caller emits PrefChanged).
/// Invalid / unknown / equal to current → Ok(false).
pub fn set_pref(name: &str, value: Option<&str>) -> Result<bool, String> {
    todo!("port: AppPref.Value (set) + MainLayout.OnResize delete")
}

pub fn prefs() -> Prefs {
    todo!("port: (AppPref.Theme/CodeSize/RailWidth/InspectorWidth)")
}

pub fn flags() -> Flags {
    todo!("port: (AppFlag.Thinking/Timestamps, Notifications.Shell)")
}

/// Persisted culture, else the OS locale, else en.
pub fn culture(os_locale: Option<&str>) -> Lang {
    todo!("port: request culture (cookie / Accept-Language)")
}

/// Persist the culture (`culture` file) and switch the app-wide i18n language.
pub fn set_culture(lang: Lang) -> Result<(), String> {
    todo!("port: POST /culture")
}

/// Everything the UI needs before mounting.
pub fn boot_info(version: &str, os_locale: Option<&str>) -> BootInfo {
    todo!("port: App.razor head bootstrap data")
}

/// The window initialization script: `window.__CCUI_BOOT__ = <json>;` + apply `data-theme`, `--code-size`,
/// `--rail-w`, `--insp-w` and `<html lang>` on `document.documentElement` before first paint.
///
/// STALENESS: Tauri registers this script once per webview (built in `setup`), so a reload — `set_culture` then
/// `location.reload()` (spec 06 §6.4), or any reload — would re-inject the values of app start. The script
/// therefore overlays `localStorage[ccui_shared::api::BOOT_MIRROR_KEY]` (`{culture, prefs, flags}`, kept current by
/// the UI: `ipc::save_boot_mirror`) on the injected JSON, inside try/catch (missing / bad mirror → injected values),
/// BEFORE applying anything; `version`, `mac`, `home`, `sep` always come from the injection.
pub fn boot_script(boot: &BootInfo) -> String {
    todo!("port: App.razor inline theme bootstrap script")
}
