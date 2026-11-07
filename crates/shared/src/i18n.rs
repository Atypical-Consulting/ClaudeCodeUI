//! Port of `Core/Strings.cs` + `Resources/Strings.resx` / `Strings.fr.resx` (spec 05 §7). IMPLEMENTED (not a stub).
//!
//! Tables: `crates/shared/i18n/{en,fr}.json`, generated once by `scripts/resx2json.py` (1 177 keys each, values raw).
//! Language: one app-wide value (`set_lang`), as the C# app-wide culture; tests pin theirs per thread with
//! `set_thread_lang` (the C# SelfCheck ran under culture fr). Missing key in fr → en value → the key itself.
//!
//! Formatting follows .NET composite format (`string.Format`): `{0}`, `{1}`, `{0:N0}` (thousands separator per
//! language), `{{` → `{`, `}}` → `}`. `t()` without arguments returns the raw value (like `L["Key"]`), so the
//! `{base}` / `{{base}}` quirk of the Wt.Why strings (spec 04 header) comes out the same as in C#.
//!
//! Usage: `t!("Key")`, `t!("Key", n, name)`, `plural(n, "Ov.Sessions")`, `name("Mode.Name.", Some(id))`, `kbd("Ctrl K")`.

use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Fr];

    /// Two-letter code (`<html lang>`, the persisted culture file).
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    /// `fr`, `fr-BE`, `FR_fr` → Fr; anything else → En (the neutral culture).
    pub fn from_code(code: &str) -> Lang {
        if code.trim().to_ascii_lowercase().starts_with("fr") {
            Lang::Fr
        } else {
            Lang::En
        }
    }

    /// Decimal separator of the culture (`1,5 Mo` / `1.5 MB`).
    pub fn decimal_sep(self) -> char {
        match self {
            Lang::En => '.',
            Lang::Fr => ',',
        }
    }

    /// Group separator used by `{0:N0}` (.NET fr-FR uses U+202F NARROW NO-BREAK SPACE).
    pub fn group_sep(self) -> &'static str {
        match self {
            Lang::En => ",",
            Lang::Fr => "\u{202F}",
        }
    }
}

static GLOBAL: AtomicU8 = AtomicU8::new(0);
static MAC: AtomicBool = AtomicBool::new(cfg!(target_os = "macos"));

thread_local! {
    static OVERRIDE: Cell<Option<Lang>> = const { Cell::new(None) };
}

/// App-wide language (backend: persisted culture; UI: chosen at boot).
pub fn set_lang(lang: Lang) {
    GLOBAL.store(lang as u8, Ordering::Relaxed);
}

/// Tests: pin the language of the current thread (None = back to the app-wide one).
pub fn set_thread_lang(lang: Option<Lang>) {
    OVERRIDE.with(|o| o.set(lang));
}

pub fn lang() -> Lang {
    OVERRIDE.with(|o| o.get()).unwrap_or(if GLOBAL.load(Ordering::Relaxed) == 1 { Lang::Fr } else { Lang::En })
}

/// Whether `kbd` renders macOS glyphs. Native default = compile target; the UI sets it from `BootInfo::mac`.
pub fn set_mac(mac: bool) {
    MAC.store(mac, Ordering::Relaxed);
}

pub fn is_mac() -> bool {
    MAC.load(Ordering::Relaxed)
}

fn table(lang: Lang) -> &'static HashMap<String, String> {
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    let (cell, src) = match lang {
        Lang::En => (&EN, include_str!("../i18n/en.json")),
        Lang::Fr => (&FR, include_str!("../i18n/fr.json")),
    };
    cell.get_or_init(|| serde_json::from_str(src).unwrap_or_default())
}

/// Every key (en table), for the parity test and tooling.
pub fn keys() -> impl Iterator<Item = &'static str> {
    table(Lang::En).keys().map(String::as_str)
}

pub fn has(key: &str) -> bool {
    table(Lang::En).contains_key(key) || table(lang()).contains_key(key)
}

/// Raw value in `lang`, falling back to en, then to the key itself (like IStringLocalizer).
pub fn t_in(lang: Lang, key: &str) -> String {
    table(lang).get(key).or_else(|| table(Lang::En).get(key)).cloned().unwrap_or_else(|| key.to_string())
}

/// Raw value in the current language (`L["Key"]`): no formatting applied.
pub fn t(key: &str) -> String {
    t_in(lang(), key)
}

/// Formatted value (`L["Key", args…]` / `Strings.Get`).
pub fn tf(key: &str, args: &[&dyn Display]) -> String {
    let l = lang();
    format_net(l, &t_in(l, key), args)
}

/// `Strings.Plural(n, key)`: `key1` when n == 1, `key0` when n == 0 and that key exists, else `keyN`; formatted with n.
pub fn plural(n: i64, key: &str) -> String {
    let k = if n == 1 {
        format!("{key}1")
    } else if n == 0 && has(&format!("{key}0")) {
        format!("{key}0")
    } else {
        format!("{key}N")
    };
    tf(&k, &[&n])
}

/// `Strings.Name(prefix, id)`: localized name of a CLI id (`Mode.Name.acceptEdits`), else the id; None → "".
pub fn name(prefix: &str, id: Option<&str>) -> String {
    match id {
        None => String::new(),
        Some(id) => {
            let key = format!("{prefix}{id}");
            if has(&key) {
                t(&key)
            } else {
                id.to_string()
            }
        }
    }
}

/// `Strings.Kbd(hint)`: hints are written "Ctrl K" / "Alt N"; macOS shows ⌘ / ⌥.
pub fn kbd(hint: &str) -> String {
    if is_mac() {
        hint.replace("Ctrl", "⌘").replace("Alt", "⌥")
    } else {
        hint.to_string()
    }
}

/// .NET composite formatting over a template: `{n}`, `{n:N0}`, `{{`, `}}`. An index without an argument stays as is.
pub fn format_net(lang: Lang, template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut it = template.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        match c {
            '{' if matches!(it.peek(), Some((_, '{'))) => {
                it.next();
                out.push('{');
            }
            '}' if matches!(it.peek(), Some((_, '}'))) => {
                it.next();
                out.push('}');
            }
            '{' => {
                let rest = &template[i + 1..];
                let Some(end) = rest.find('}') else {
                    out.push(c);
                    continue;
                };
                let spec = &rest[..end];
                let (idx, fmt) = spec.split_once(':').map_or((spec, None), |(a, b)| (a, Some(b)));
                match idx.trim().parse::<usize>().ok().and_then(|n| args.get(n)) {
                    Some(arg) => {
                        let s = arg.to_string();
                        out.push_str(&match fmt {
                            Some("N0") => group_digits(lang, &s),
                            _ => s,
                        });
                        for _ in 0..spec.chars().count() + 1 {
                            it.next();
                        }
                    }
                    None => out.push(c),
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// `{0:N0}`: the number rounded to an integer with the culture's group separator ("20,000" / "20 000").
fn group_digits(lang: Lang, s: &str) -> String {
    let Ok(v) = s.trim().parse::<f64>() else { return s.to_string() };
    let n = v.round() as i64;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push_str(lang.group_sep());
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// `t!("Key")` = raw value; `t!("Key", a, b)` = formatted.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::t($key)
    };
    ($key:expr, $($arg:expr),+ $(,)?) => {
        $crate::i18n::tf($key, &[$(&$arg as &dyn ::std::fmt::Display),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_keys_in_both_tables() {
        let en: Vec<_> = table(Lang::En).keys().collect();
        assert_eq!(en.len(), 1177);
        assert!(en.iter().all(|k| table(Lang::Fr).contains_key(*k)));
        assert_eq!(table(Lang::Fr).len(), 1177);
    }

    #[test]
    fn lookup_format_plural() {
        set_thread_lang(Some(Lang::Fr));
        assert_eq!(crate::t!("Session.Exited", 3), "claude s’est arrêté (3)");
        assert_eq!(plural(0, "Ov.Sessions"), "0 session");
        assert_eq!(crate::t!("Mention.Truncated", 20000), "20\u{202F}000 premières entrées");
        assert_eq!(t("No.Such.Key"), "No.Such.Key");
        set_thread_lang(Some(Lang::En));
        assert_eq!(plural(0, "Ov.Sessions"), "0 sessions");
        assert_eq!(crate::t!("Mention.Truncated", 20000), "first 20,000 entries");
        assert_eq!(name("Mode.Name.", Some("bypassPermissions")), "bypassPermissions");
        assert_eq!(name("Mode.Name.", None), "");
        set_thread_lang(None);
    }

    /// Strings.Check: French typography (no-break space before `? ! : ; »`, after `«`; `’` not `'`), outside code/tags.
    #[test]
    fn fr_typography() {
        let strip = regex::Regex::new(r"<code>.*?</code>|`[^`]*`|<[^>]*>").unwrap();
        let bad = regex::Regex::new(r" [?!:;»]|« |'").unwrap();
        let mut bad_keys: Vec<_> =
            table(Lang::Fr).iter().filter(|(_, v)| bad.is_match(&strip.replace_all(v, ""))).map(|(k, _)| k.as_str()).collect();
        bad_keys.sort();
        assert!(bad_keys.is_empty(), "fr typography: {bad_keys:?}");
    }

    /// Strings.Check: en / fr lookups, plurals, names.
    #[test]
    fn strings_check() {
        set_thread_lang(Some(Lang::En));
        assert_eq!(t("Rail.NewSession"), "New session");
        assert_eq!(t("Wt.State.Safe"), "Safe");
        assert_eq!(plural(0, "Ov.Sessions"), "0 sessions");
        assert_eq!(plural(1, "Ov.Sessions"), "1 session");
        assert_eq!(plural(0, "Tools"), "0 tools");
        assert_eq!(name("Mode.Name.", Some("acceptEdits")), "Accept edits");
        set_thread_lang(Some(Lang::from_code("fr-BE")));
        assert_eq!(t("Rail.NewSession"), "Nouvelle session");
        assert_eq!(t("Wt.State.Safe"), "Sûr");
        assert_eq!(plural(0, "Ov.Sessions"), "0 session");
        assert_eq!(plural(2, "Ov.Sessions"), "2 sessions");
        set_thread_lang(None);
    }

    #[test]
    fn net_braces() {
        assert_eq!(format_net(Lang::En, "a {{base}} {0}", &[&"x"]), "a {base} x");
        assert_eq!(format_net(Lang::En, "{1} {0} {2}", &[&1, &2]), "2 1 {2}");
    }
}
