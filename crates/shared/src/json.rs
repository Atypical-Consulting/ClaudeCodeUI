//! Port of the JSON access helpers of `Core/Events.cs` (`Str`, `Prop`, `Bool`, `Long`, `Double`, `Names`, `Time`,
//! `ResultText`) and `WorkflowRuns.Num` — spec 02 §1, spec 05 header. Everything that reads CLI JSON degrades through
//! these: a missing or wrong-typed field is None / false / 0, never a panic. IMPLEMENTED (foundation for every unit).
//! Owner: shared-pure-a.

use crate::model::Time;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;

/// Stand-in for C# `default(JsonElement)`: every helper answers None/false/0 on it.
pub static UNDEFINED: Value = Value::Null;

/// `Str(e, name)`: Some only for a JSON string value.
pub fn str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)?.as_str()
}

/// `Str(...) ?? ""` owned.
pub fn string(v: &Value, k: &str) -> String {
    str(v, k).unwrap_or_default().to_string()
}

/// `Str` → Some(owned) when non-empty.
pub fn non_empty(v: &Value, k: &str) -> Option<String> {
    str(v, k).filter(|s| !s.is_empty()).map(str::to_string)
}

/// `Prop(e, name)`: present and not JSON null.
pub fn prop<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    v.get(k).filter(|x| !x.is_null())
}

/// `Prop(...) ?? default`: the value or `UNDEFINED`.
pub fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    prop(v, k).unwrap_or(&UNDEFINED)
}

/// `Bool(e, name)`: literally `true`.
pub fn bool(v: &Value, k: &str) -> bool {
    v.get(k) == Some(&Value::Bool(true))
}

/// `Long(e, name)`: number truncated toward zero (saturating), else 0.
pub fn long(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(Value::as_f64).map(|f| f as i64).unwrap_or(0)
}

/// `Num(e, name)` (WorkflowRuns): Some(truncated) only for a number.
pub fn num(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(Value::as_f64).map(|f| f as i64)
}

/// `Double(e, name)`: number, else 0.
pub fn double(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// A JSON number as an exact decimal: parsed from its shortest round-trip text (`0.0063816` stays exact).
pub fn decimal(v: &Value, k: &str) -> Option<Decimal> {
    let n = v.get(k)?.as_number()?;
    Decimal::from_str(&n.to_string()).or_else(|_| Decimal::from_scientific(&n.to_string())).ok()
}

/// `Names(e, name)`: array elements → the string, or the object's `name` string, else "". Not an array → [].
pub fn names(v: &Value, k: &str) -> Vec<String> {
    match v.get(k) {
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| match x {
                Value::String(s) => s.clone(),
                o => str(o, "name").unwrap_or_default().to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `Time(e)`: the `timestamp` string as RFC 3339 (milliseconds kept), else None.
pub fn time(v: &Value) -> Option<Time> {
    parse_time(str(v, "timestamp")?)
}

/// Lenient ISO-8601 parse (RFC 3339 first) → UTC.
pub fn parse_time(s: &str) -> Option<Time> {
    chrono::DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.with_timezone(&chrono::Utc))
}

/// `ResultText(content)`: string → itself; array → each block's `text` (or `[<type>]`) joined by `\n`;
/// null/absent → ""; any other kind → its raw JSON text.
pub fn result_text(content: &Value) -> String {
    match content {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(|b| match str(b, "text") {
                Some(t) => t.to_string(),
                None => format!("[{}]", str(b, "type").unwrap_or_default()),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

/// C# `JsonElement.ToString()`: a string's unquoted value, anything else its raw JSON.
pub fn to_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn helpers_degrade() {
        let v = json!({"s":"x","n":1791585746931.5,"t":true,"ts":"true","z":null,"a":["a",{"name":"b"},3]});
        assert_eq!(str(&v, "s"), Some("x"));
        assert_eq!(str(&v, "n"), None);
        assert_eq!(long(&v, "n"), 1791585746931);
        assert!(bool(&v, "t") && !bool(&v, "ts"));
        assert!(prop(&v, "z").is_none());
        assert_eq!(names(&v, "a"), vec!["a", "b", ""]);
        assert_eq!(str(&json!("x"), "s"), None);
        assert_eq!(result_text(&json!([{"type":"text","text":"a"},{"type":"image"},{}])), "a\n[image]\n[]");
        assert_eq!(decimal(&json!({"c":0.0063816}), "c").unwrap().to_string(), "0.0063816");
        assert_eq!(time(&json!({"timestamp":"2026-10-09T12:41:34.602Z"})).unwrap().timestamp_millis(), 1791549694602);
    }
}
