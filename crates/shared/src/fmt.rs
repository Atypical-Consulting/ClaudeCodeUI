//! Port of `Core/Fmt.cs` (culture-aware formatting) — spec 05 §6. Owner: shared-pure-b.
//! Numbers follow the UI language (`i18n::lang()`): fr "1,5 Mo", en "1.5 MB"; costs are invariant.
//! Every clock-dependent function takes `now` so tests are deterministic. Local time = chrono `Local`.

use crate::model::Time;
use rust_decimal::{Decimal, RoundingStrategy};

/// .NET `Math.Round` default (MidpointRounding.ToEven).
pub fn round_half_even(v: f64) -> f64 {
    v.round_ties_even()
}

/// .NET custom numeric format with `decimals` optional (`0.#`) or fixed (`0.00`) places: half away from zero,
/// culture decimal separator.
fn fixed(v: f64, decimals: usize, optional: bool) -> String {
    let p = 10f64.powi(decimals as i32);
    let r = (v * p).round() / p;
    let mut s = format!("{r:.decimals$}");
    if optional && s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" {
        s = "0".into();
    }
    s.replace('.', &lang().decimal_sep().to_string())
}

/// `0.#` in the current culture (one optional decimal).
pub fn one_decimal(v: f64) -> String {
    fixed(v, 1, true)
}

/// `Fmt.Dur`: <60 s `Fmt.Seconds` (0.#), ≥1 h `Fmt.HoursMinutes` (mm), whole minutes `Fmt.Minutes`, else
/// `Fmt.MinutesSeconds` (ss). en `4.5s` `3m` `3m 07s` `1h 05m`; fr `4,5 s` `3 min` `3 min 07` `1 h 05`.
pub fn dur(ms: i64) -> String {
    let (mins, secs) = ((ms / 60_000) % 60, (ms / 1000) % 60);
    if ms < 60_000 {
        crate::t!("Fmt.Seconds", one_decimal(ms as f64 / 1000.0))
    } else if ms >= 3_600_000 {
        crate::t!("Fmt.HoursMinutes", ms / 3_600_000, format!("{mins:02}"))
    } else if secs == 0 {
        crate::t!("Fmt.Minutes", mins)
    } else {
        crate::t!("Fmt.MinutesSeconds", mins, format!("{secs:02}"))
    }
}

/// `Fmt.Size`: ≥1 GiB `Fmt.GB` (0.00), ≥1 MiB `Fmt.MB` (0.#), ≥1 KiB `Fmt.KB` (bytes>>10), else `Fmt.Bytes`.
pub fn size(bytes: i64) -> String {
    if bytes >= 1 << 30 {
        crate::t!("Fmt.GB", fixed(bytes as f64 / (1u64 << 30) as f64, 2, false))
    } else if bytes >= 1 << 20 {
        crate::t!("Fmt.MB", one_decimal(bytes as f64 / (1u64 << 20) as f64))
    } else if bytes >= 1 << 10 {
        crate::t!("Fmt.KB", bytes >> 10)
    } else {
        crate::t!("Fmt.Bytes", bytes)
    }
}

/// `Fmt.Pct` with round_half_even(f*100).
pub fn pct(fraction: f64) -> String {
    crate::t!("Fmt.Pct", round_half_even(fraction * 100.0) as i64)
}

/// `Fmt.Tokens`: ≥999 500 → `0.#` of n/1e6 + "M"; ≥1000 → round_half_even(n/1000) + "k"; else n.
pub fn tokens(n: i64) -> String {
    if n >= 999_500 {
        format!("{}M", one_decimal(n as f64 / 1_000_000.0))
    } else if n >= 1_000 {
        format!("{}k", round_half_even(n as f64 / 1_000.0) as i64)
    } else {
        n.to_string()
    }
}

/// `"$" + v` with `decimals` places, invariant (`$0.0391`).
pub fn cost(usd: Decimal, decimals: u32) -> String {
    let r = usd.round_dp_with_strategy(decimals, RoundingStrategy::MidpointAwayFromZero);
    format!("${r:.0$}", decimals as usize)
}

/// Local `HH:mm`.
pub fn time(t: Time) -> String {
    local(t).format("%H:%M").to_string()
}

/// days = today − local date: <0 → `ddd HH:mm` (fr weekday abbreviations `lun.`…), 0 → time, 1 → `Day.Yesterday`,
/// else `Day.Ago`.
pub fn day(t: Time, now: Time) -> String {
    day_at(local(t), local(now))
}

fn day_at(t: NaiveDateTime, now: NaiveDateTime) -> String {
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const FR: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
    match (now.date() - t.date()).num_days() {
        d if d < 0 => {
            let wd = t.weekday().num_days_from_monday() as usize;
            format!("{} {}", if lang() == Lang::Fr { FR[wd] } else { EN[wd] }, t.format("%H:%M"))
        }
        0 => t.format("%H:%M").to_string(),
        1 => crate::t!("Day.Yesterday"),
        d => crate::t!("Day.Ago", d),
    }
}

/// <1 min `Ov.AgeNow`; <1 h `Ov.AgeMin`; <24 h `Ov.AgeH`; else `day`.
pub fn age(at: Time, now: Time) -> String {
    let secs = (now - at).num_milliseconds() as f64 / 1000.0;
    if secs < 60.0 {
        crate::t!("Ov.AgeNow")
    } else if secs < 3600.0 {
        crate::t!("Ov.AgeMin", (secs / 60.0) as i64)
    } else if secs < 86_400.0 {
        crate::t!("Ov.AgeH", (secs / 3600.0) as i64)
    } else {
        day(at, now)
    }
}

/// HTML-encode `<>&'"` and non-ASCII as `&#x…;` (only where HTML strings are concatenated).
/// Mirrors `HtmlEncoder.Default`: printable BasicLatin allowed except `<>&'"+`, everything else `&#xHEX;`.
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' | '+' => out.push_str(&format!("&#x{:X};", c as u32)),
            ' '..='~' => out.push(c),
            _ => out.push_str(&format!("&#x{:X};", c as u32)),
        }
    }
    out
}

/// `\` → `/` in both; path starting with home (case-insensitive) → `~` + rest.
pub fn home(path: &str, home: &str) -> String {
    let (h, p) = (home.replace('\\', "/"), path.replace('\\', "/"));
    match p.get(..h.len()) {
        Some(head) if head.to_lowercase() == h.to_lowercase() => format!("~{}", &p[h.len()..]),
        _ => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::set_thread_lang;

    #[test]
    fn numbers_fr_en() {
        set_thread_lang(Some(Lang::Fr));
        assert_eq!(tokens(999_600), "1M");
        assert_eq!(tokens(999_400), "999k");
        assert_eq!(tokens(2_500), "2k"); // banker's
        assert_eq!(tokens(1_250_000), "1,3M");
        assert_eq!(dur(4_500), "4,5 s");
        assert_eq!(dur(180_000), "3 min");
        assert_eq!(dur(187_000), "3 min 07");
        assert_eq!(dur(3_900_000), "1 h 05");
        assert_eq!(size(262_144), "256 Ko");
        assert_eq!(size(1_572_864), "1,5 Mo");
        assert_eq!(size(3 << 30), "3,00 Go");
        assert_eq!(size(12), "12 o");
        assert_eq!(pct(0.125), "12 %");
        set_thread_lang(Some(Lang::En));
        assert_eq!(dur(4_500), "4.5s");
        assert_eq!(dur(4_250), "4.3s"); // 0.# rounds half away from zero
        assert_eq!(dur(180_000), "3m");
        assert_eq!(dur(187_000), "3m 07s");
        assert_eq!(dur(3_900_000), "1h 05m");
        assert_eq!(size(1_572_864), "1.5 MB");
        assert_eq!(pct(0.135), "14%");
        assert_eq!(tokens(999), "999");
        set_thread_lang(None);
        assert_eq!(cost(Decimal::new(391, 4), 4), "$0.0391");
        assert_eq!(cost(Decimal::new(125, 3), 2), "$0.13");
        assert_eq!(cost(Decimal::new(5, 0), 2), "$5.00");
        assert_eq!(round_half_even(2.5), 2.0);
    }

    #[test]
    fn days_and_ages() {
        let at = |d: u32, h: u32| NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(h, 5, 0).unwrap();
        set_thread_lang(Some(Lang::Fr));
        assert_eq!(day_at(at(9, 19), at(7, 10)), "lun. 19:05");
        assert_eq!(day_at(at(7, 9), at(7, 10)), "09:05");
        assert_eq!(day_at(at(6, 9), at(7, 10)), "hier");
        assert_eq!(day_at(at(4, 9), at(7, 10)), "il y a 3 j");
        set_thread_lang(Some(Lang::En));
        assert_eq!(day_at(at(9, 19), at(7, 10)), "Mon 19:05");
        assert_eq!(day_at(at(4, 9), at(7, 10)), "3d ago");
        let now = chrono::Utc::now();
        assert_eq!(age(now - chrono::Duration::seconds(30), now), "just now");
        assert_eq!(age(now - chrono::Duration::minutes(3), now), "3 min ago");
        assert_eq!(age(now - chrono::Duration::hours(2), now), "2 h ago");
        assert_eq!(age(now - chrono::Duration::days(3), now), "3d ago");
        set_thread_lang(None);
    }

    #[test]
    fn enc_home() {
        assert_eq!(enc("<a href='x'>é & \"+\"</a> 😀"), "&lt;a href=&#x27;x&#x27;&gt;&#xE9; &amp; &quot;&#x2B;&quot;&lt;/a&gt; &#x1F600;");
        assert_eq!(home("C:\\Users\\Me\\src\\a", "c:\\users\\me"), "~/src/a");
        assert_eq!(home("/Users/me/x", "/Users/me"), "~/x");
        assert_eq!(home("/opt/x", "/Users/me"), "/opt/x");
    }
}

// ------------------------------------------------------------------------------------------- .NET date/time formats
// IMPLEMENTED (small, name tables hard-coded). The `*_at` variants take the local wall-clock value (tests); the
// public ones convert a UTC `Time` to chrono `Local` (browser offset in wasm via chrono's `wasmbind`).
// en = en-US, fr = fr-FR as .NET renders them. ponytail: en AM/PM uses a plain space (ICU 72+ emits U+202F); swap
// in "\u{202F}" if the C# screenshots show the narrow space.

use crate::i18n::{lang, Lang};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};

const EN_DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const FR_DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const EN_MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const FR_MONTHS: [&str; 12] =
    ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

fn local(t: Time) -> NaiveDateTime {
    t.with_timezone(&Local).naive_local()
}

/// `h:mm` + ` AM|PM` (en), `HH:mm` (fr); `secs` adds `:ss`.
fn clock(t: NaiveDateTime, l: Lang, secs: bool) -> String {
    let s = if secs { format!(":{:02}", t.second()) } else { String::new() };
    match l {
        Lang::En => {
            let h = t.hour() % 12;
            format!("{}:{:02}{s} {}", if h == 0 { 12 } else { h }, t.minute(), if t.hour() < 12 { "AM" } else { "PM" })
        }
        Lang::Fr => format!("{:02}:{:02}{s}", t.hour(), t.minute()),
    }
}

/// `ToString("D")` of a local date: en "Saturday, November 7, 2026", fr "samedi 7 novembre 2026" (Thread dividers).
pub fn long_date_in(d: NaiveDate, l: Lang) -> String {
    let (wd, m) = (d.weekday().num_days_from_monday() as usize, d.month0() as usize);
    match l {
        Lang::En => format!("{}, {} {}, {}", EN_DAYS[wd], EN_MONTHS[m], d.day(), d.year()),
        Lang::Fr => format!("{} {} {} {}", FR_DAYS[wd], d.day(), FR_MONTHS[m], d.year()),
    }
}

pub fn long_date(d: NaiveDate) -> String {
    long_date_in(d, lang())
}

/// `ToString("g")`: en "11/7/2026 3:04 PM", fr "07/11/2026 15:04" (QuotaMeter, Overview, Background titles).
pub fn short_date_time_at(t: NaiveDateTime, l: Lang) -> String {
    match l {
        Lang::En => format!("{}/{}/{} {}", t.month(), t.day(), t.year(), clock(t, l, false)),
        Lang::Fr => format!("{:02}/{:02}/{} {}", t.day(), t.month(), t.year(), clock(t, l, false)),
    }
}

pub fn short_date_time(t: Time) -> String {
    short_date_time_at(local(t), lang())
}

/// `ToString("T")`: en "3:04:05 PM", fr "15:04:05".
pub fn long_time_at(t: NaiveDateTime, l: Lang) -> String {
    clock(t, l, true)
}

pub fn long_time(t: Time) -> String {
    long_time_at(local(t), lang())
}

/// `ToString("HH:mm:ss", Invariant)` of the local time.
pub fn hms(t: Time) -> String {
    local(t).format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod date_tests {
    use super::*;

    #[test]
    fn net_date_formats() {
        let t = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap().and_hms_opt(15, 4, 5).unwrap();
        assert_eq!(long_date_in(t.date(), Lang::En), "Saturday, November 7, 2026");
        assert_eq!(long_date_in(t.date(), Lang::Fr), "samedi 7 novembre 2026");
        assert_eq!(short_date_time_at(t, Lang::En), "11/7/2026 3:04 PM");
        assert_eq!(short_date_time_at(t, Lang::Fr), "07/11/2026 15:04");
        assert_eq!(long_time_at(t, Lang::En), "3:04:05 PM");
        assert_eq!(long_time_at(t, Lang::Fr), "15:04:05");
        let midnight = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap().and_hms_opt(0, 7, 0).unwrap();
        assert_eq!(long_time_at(midnight, Lang::En), "12:07:00 AM");
    }
}
