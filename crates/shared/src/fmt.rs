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
