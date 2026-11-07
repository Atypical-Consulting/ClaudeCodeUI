//! Port of `Core/Fmt.cs` (culture-aware formatting) — spec 05 §6. Owner: shared-pure-b.
//! Numbers follow the UI language (`i18n::lang()`): fr "1,5 Mo", en "1.5 MB"; costs are invariant.
//! Every clock-dependent function takes `now` so tests are deterministic. Local time = chrono `Local`.

use crate::model::Time;
use rust_decimal::Decimal;

/// .NET `Math.Round` default (MidpointRounding.ToEven).
pub fn round_half_even(v: f64) -> f64 {
    todo!("port: Math.Round (banker's)")
}

/// `0.#` in the current culture (one optional decimal).
pub fn one_decimal(v: f64) -> String {
    todo!("port: ToString(\"0.#\", culture)")
}

/// `Fmt.Dur`: <60 s `Fmt.Seconds` (0.#), ≥1 h `Fmt.HoursMinutes` (mm), whole minutes `Fmt.Minutes`, else
/// `Fmt.MinutesSeconds` (ss). en `4.5s` `3m` `3m 07s` `1h 05m`; fr `4,5 s` `3 min` `3 min 07` `1 h 05`.
pub fn dur(ms: i64) -> String {
    todo!("port: Fmt.Dur")
}

/// `Fmt.Size`: ≥1 GiB `Fmt.GB` (0.00), ≥1 MiB `Fmt.MB` (0.#), ≥1 KiB `Fmt.KB` (bytes>>10), else `Fmt.Bytes`.
pub fn size(bytes: i64) -> String {
    todo!("port: Fmt.Size")
}

/// `Fmt.Pct` with round_half_even(f*100).
pub fn pct(fraction: f64) -> String {
    todo!("port: Fmt.Pct")
}

/// `Fmt.Tokens`: ≥999 500 → `0.#` of n/1e6 + "M"; ≥1000 → round_half_even(n/1000) + "k"; else n.
pub fn tokens(n: i64) -> String {
    todo!("port: Fmt.Tokens")
}

/// `"$" + v` with `decimals` places, invariant (`$0.0391`).
pub fn cost(usd: Decimal, decimals: u32) -> String {
    todo!("port: Fmt.Cost")
}

/// Local `HH:mm`.
pub fn time(t: Time) -> String {
    todo!("port: Fmt.Time")
}

/// days = today − local date: <0 → `ddd HH:mm` (fr weekday abbreviations `lun.`…), 0 → time, 1 → `Day.Yesterday`,
/// else `Day.Ago`.
pub fn day(t: Time, now: Time) -> String {
    todo!("port: Fmt.Day")
}

/// <1 min `Ov.AgeNow`; <1 h `Ov.AgeMin`; <24 h `Ov.AgeH`; else `day`.
pub fn age(at: Time, now: Time) -> String {
    todo!("port: Fmt.Age")
}

/// HTML-encode `<>&'"` and non-ASCII as `&#x…;` (only where HTML strings are concatenated).
pub fn enc(s: &str) -> String {
    todo!("port: Fmt.Enc")
}

/// `\` → `/` in both; path starting with home (case-insensitive) → `~` + rest.
pub fn home(path: &str, home: &str) -> String {
    todo!("port: Fmt.Home")
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
