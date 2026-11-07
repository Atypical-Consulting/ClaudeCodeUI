//! Port of `Core/AppFlag.cs` (`AppFlag` + `AppPref`), the culture choice and the boot info — spec 03 §9, 06 §1.
//! Owner: core-store-sessions. Files under `paths::app_data()` (existing users keep their choices):
//! flags = empty file (exists = on); prefs = plain text value (trimmed; outside the whitelist reads None).
//! Names are whitelisted (`ccui_shared::api::{PREF_*, FLAG_*}`): anything else is refused.

use crate::paths;
use ccui_shared::api::*;
use ccui_shared::i18n::Lang;
use std::fs;

const FLAG_NAMES: [&str; 3] = [FLAG_THINKING, FLAG_TIMESTAMPS, FLAG_NOTIFY];
const PREF_NAMES: [&str; 4] = [PREF_THEME, PREF_CODE_SIZE, PREF_RAIL_WIDTH, PREF_INSPECTOR_WIDTH];

/// `Npx` with N in range (every pixel between the bounds: a hand-edited file cannot break the grid).
fn px_in(value: &str, range: std::ops::RangeInclusive<u32>) -> bool {
    value.strip_suffix("px").filter(|n| n.bytes().all(|b| b.is_ascii_digit())).and_then(|n| n.parse().ok()).is_some_and(|n| range.contains(&n))
}

/// The AppPref value lists (same rule as `api::valid_pref`, kept here so the backend owns the check).
fn valid(name: &str, value: &str) -> bool {
    match name {
        PREF_THEME => THEMES.contains(&value),
        PREF_CODE_SIZE => CODE_SIZES.contains(&value),
        PREF_RAIL_WIDTH => px_in(value, RAIL_WIDTHS),
        PREF_INSPECTOR_WIDTH => px_in(value, INSPECTOR_WIDTHS),
        _ => false,
    }
}

/// Whether flag file `name` exists (IO errors → false).
pub fn flag(name: &str) -> bool {
    FLAG_NAMES.contains(&name) && paths::app_data().join(name).is_file()
}

/// Create/delete flag file `name`; errors swallowed ("the getter shows what stuck"). Unknown name → Err.
pub fn set_flag(name: &str, on: bool) -> Result<(), String> {
    if !FLAG_NAMES.contains(&name) {
        return Err(format!("unknown flag: {name}"));
    }
    let path = paths::app_data().join(name);
    let _ = if on { fs::create_dir_all(paths::app_data()).and_then(|_| fs::write(&path, "")) } else { fs::remove_file(&path).or(Ok(())) };
    Ok(())
}

/// Pref value if valid for `name`, else None (never chosen, unreadable, hand-edited outside the list).
pub fn pref(name: &str) -> Option<String> {
    let v = fs::read_to_string(paths::app_data().join(name)).ok()?;
    let v = v.trim();
    valid(name, v).then(|| v.to_string())
}

/// Set (Some valid value) or delete (None) a pref; returns whether the file changed (caller emits PrefChanged).
/// Invalid / unknown / equal to current → Ok(false).
pub fn set_pref(name: &str, value: Option<&str>) -> Result<bool, String> {
    if !PREF_NAMES.contains(&name) {
        return Err(format!("unknown pref: {name}"));
    }
    let path = paths::app_data().join(name);
    match value {
        // MainLayout.OnResize: a splitter double-click goes back to the CSS default.
        None => Ok(fs::remove_file(&path).is_ok()),
        Some(v) if !valid(name, v) || pref(name).as_deref() == Some(v) => Ok(false),
        Some(v) => Ok(fs::create_dir_all(paths::app_data()).and_then(|_| fs::write(&path, v)).is_ok()),
    }
}

pub fn prefs() -> Prefs {
    Prefs {
        theme: pref(PREF_THEME),
        code_size: pref(PREF_CODE_SIZE),
        rail_width: pref(PREF_RAIL_WIDTH),
        inspector_width: pref(PREF_INSPECTOR_WIDTH),
    }
}

pub fn flags() -> Flags {
    Flags { thinking: flag(FLAG_THINKING), timestamps: flag(FLAG_TIMESTAMPS), notify: flag(FLAG_NOTIFY) }
}

/// Persisted culture, else the OS locale, else en.
pub fn culture(os_locale: Option<&str>) -> Lang {
    let saved = fs::read_to_string(paths::app_data().join(PREF_CULTURE)).ok();
    match saved.as_deref().map(str::trim) {
        Some(c @ ("en" | "fr")) => Lang::from_code(c),
        _ => os_locale.map(Lang::from_code).unwrap_or_default(),
    }
}

/// Persist the culture (`culture` file) and switch the app-wide i18n language.
pub fn set_culture(lang: Lang) -> Result<(), String> {
    fs::create_dir_all(paths::app_data())
        .and_then(|_| fs::write(paths::app_data().join(PREF_CULTURE), lang.code()))
        .map_err(|e| e.to_string())?;
    ccui_shared::i18n::set_lang(lang);
    Ok(())
}

/// Everything the UI needs before mounting.
pub fn boot_info(version: &str, os_locale: Option<&str>) -> BootInfo {
    BootInfo {
        culture: culture(os_locale).code().to_string(),
        mac: cfg!(target_os = "macos"),
        version: version.to_string(),
        home: paths::home().to_string_lossy().into_owned(),
        sep: std::path::MAIN_SEPARATOR_STR.to_string(),
        prefs: prefs(),
        flags: flags(),
    }
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
    let json = serde_json::to_string(boot).unwrap_or_else(|_| "{}".into());
    let key = serde_json::to_string(BOOT_MIRROR_KEY).unwrap_or_default();
    let themes = serde_json::to_string(&THEMES).unwrap_or_default();
    let sizes = serde_json::to_string(&CODE_SIZES).unwrap_or_default();
    // Mirror fields are copied one by one and only with the right type: a malformed value must not make the UI's
    // BootInfo deserialization fail (it would fall back to defaults wholesale).
    format!(
        r#"(function(){{var b={json};try{{var m=JSON.parse(localStorage.getItem({key})||"null");if(m&&typeof m==="object"){{if(m.culture==="en"||m.culture==="fr")b.culture=m.culture;var c=function(s,d,t){{if(s&&typeof s==="object")for(var k in d)if(Object.prototype.hasOwnProperty.call(s,k)&&(typeof s[k]===t||(t==="string"&&s[k]===null)))d[k]=s[k];}};c(m.prefs,b.prefs,"string");c(m.flags,b.flags,"boolean");}}}}catch(e){{}}window.__CCUI_BOOT__=b;var a=function(){{var d=document.documentElement,p=b.prefs;d.dataset.theme={themes}.indexOf(p.theme)>=0?p.theme:"graphite";d.style.setProperty("--code-size",{sizes}.indexOf(p.code_size)>=0?p.code_size:"13px");var w=function(v,k){{if(/^\d+px$/.test(v||""))d.style.setProperty(k,v);}};w(p.rail_width,"--rail-w");w(p.inspector_width,"--insp-w");d.lang=b.culture==="fr"?"fr":"en";}};if(document.documentElement)a();else new MutationObserver(function(_,o){{if(document.documentElement){{o.disconnect();a();}}}}).observe(document,{{childList:true}});}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_and_flags() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(Some(dir.path()));
        assert_eq!(pref(PREF_THEME), None, "unset reads None");
        assert_eq!(set_pref(PREF_THEME, Some("encre")), Ok(true), "a value sticks and changes");
        assert_eq!(pref(PREF_THEME).as_deref(), Some("encre"));
        assert_eq!(set_pref(PREF_THEME, Some("encre")), Ok(false), "same value: no change");
        assert_eq!(set_pref(PREF_THEME, Some("evil")), Ok(false), "unknown value: no change");
        assert_eq!(pref(PREF_THEME).as_deref(), Some("encre"));
        assert!(set_pref("culture", Some("fr")).is_err() && set_pref("../x", Some("a")).is_err(), "names whitelisted");
        fs::write(paths::app_data().join(PREF_THEME), "evil\n").unwrap();
        assert_eq!(pref(PREF_THEME), None, "a value outside the list reads None");
        // C# wrote the plain value; the reader trims.
        fs::write(paths::app_data().join(PREF_CODE_SIZE), "14px\r\n").unwrap();
        assert_eq!(prefs().code_size.as_deref(), Some("14px"));
        assert_eq!(set_pref(PREF_RAIL_WIDTH, Some("300px")), Ok(true));
        assert_eq!(set_pref(PREF_RAIL_WIDTH, Some("401px")), Ok(false));
        assert_eq!(set_pref(PREF_INSPECTOR_WIDTH, Some("+400px")), Ok(false));
        assert_eq!(set_pref(PREF_RAIL_WIDTH, None), Ok(true), "delete");
        assert_eq!(set_pref(PREF_RAIL_WIDTH, None), Ok(false), "nothing to delete");

        assert!(!flag(FLAG_NOTIFY), "toggle off by default");
        set_flag(FLAG_NOTIFY, true).unwrap();
        assert!(flag(FLAG_NOTIFY) && flags().notify);
        set_flag(FLAG_NOTIFY, false).unwrap();
        set_flag(FLAG_NOTIFY, false).unwrap();
        assert!(!flag(FLAG_NOTIFY), "back off (twice is fine)");
        assert!(set_flag("nope", true).is_err());
        paths::set_test_root(None);
    }

    #[test]
    fn culture_and_boot() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(Some(dir.path()));
        assert_eq!(culture(None), Lang::En);
        assert_eq!(culture(Some("fr-BE")), Lang::Fr);
        fs::create_dir_all(paths::app_data()).unwrap();
        fs::write(paths::app_data().join("culture"), "en").unwrap();
        assert_eq!(culture(Some("fr-BE")), Lang::En, "the persisted choice wins");
        set_pref(PREF_THEME, Some("mousse")).unwrap();
        let boot = boot_info("1.2.3", Some("fr"));
        assert_eq!((boot.culture.as_str(), boot.version.as_str()), ("en", "1.2.3"));
        assert_eq!(boot.prefs.theme.as_deref(), Some("mousse"));
        let js = boot_script(&boot);
        assert!(js.contains(r#""theme":"mousse""#) && js.contains("\"ccui.boot\"") && js.contains("--insp-w"));
        assert!(js.starts_with("(function(){") && js.ends_with("})();"));
        paths::set_test_root(None);
    }
}
