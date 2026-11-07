//! Port of `Core/Elicit.cs` (MCP elicitation) — spec 02 §12. Owner: shared-pure-a.
//! Queued as a PendingPermission (tool `dialog:mcp_elicitation`, input = the whole request).

use crate::json;
use crate::model::PendingPermission;
use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::collections::{BTreeSet, HashMap};

/// The tool_name the CLI itself gives this pending action.
pub const TOOL: &str = "dialog:mcp_elicitation";

/// Field kind: `multi` (array), `boolean`, `number`, `integer`, `enum` (options non-empty), `string`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Multi,
    Boolean,
    Number,
    Integer,
    Enum,
    String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub key: String,
    /// title ?? key.
    pub label: String,
    pub description: Option<String>,
    pub kind: FieldKind,
    pub required: bool,
    /// `(value, label)`.
    pub options: Vec<(String, String)>,
    pub default: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// `<input type>`: email | url | date | datetime-local | text.
    pub input_type: String,
}

/// mode == "url".
pub fn is_url(request: &Value) -> bool {
    json::str(request, "mode") == Some("url")
}

/// `url` only if absolute http(s).
pub fn url(request: &Value) -> Option<String> {
    let u = json::str(request, "url")?;
    let (scheme, rest) = u.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    (matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") && !host.is_empty() && !u.contains(char::is_whitespace))
        .then(|| u.to_string())
}

/// pending.tool == TOOL && input.mcp_server_name == server && input.elicitation_id == id.
pub fn matches(pending: &PendingPermission, server: &str, elicitation_id: &str) -> bool {
    pending.tool == TOOL
        && json::str(&pending.input, "mcp_server_name") == Some(server)
        && json::str(&pending.input, "elicitation_id") == Some(elicitation_id)
}

/// Fields of `requested_schema.properties` in JSON order; missing → [].
pub fn parse(request: &Value) -> Vec<Field> {
    let Some(schema) = json::prop(request, "requested_schema") else { return Vec::new() };
    let Some(Value::Object(props)) = json::prop(schema, "properties") else { return Vec::new() };
    let required: Vec<String> = match json::prop(schema, "required") {
        Some(Value::Array(r)) => r.iter().map(json::to_text).collect(),
        _ => Vec::new(),
    };
    props
        .iter()
        .map(|(key, s)| {
            let ty = json::str(s, "type");
            let options = options(if ty == Some("array") { json::get(s, "items") } else { s });
            let kind = match ty {
                Some("array") => FieldKind::Multi,
                Some("boolean") => FieldKind::Boolean,
                Some("number") => FieldKind::Number,
                Some("integer") => FieldKind::Integer,
                _ if !options.is_empty() => FieldKind::Enum,
                _ => FieldKind::String,
            };
            Field {
                key: key.clone(),
                label: json::str(s, "title").unwrap_or(key).to_string(),
                description: json::str(s, "description").map(str::to_string),
                kind,
                required: required.contains(key),
                options,
                default: json::prop(s, "default").map(json::to_text),
                min: json::prop(s, "minimum").and_then(Value::as_f64),
                max: json::prop(s, "maximum").and_then(Value::as_f64),
                input_type: match json::str(s, "format") {
                    Some("email") => "email",
                    Some("uri") => "url",
                    Some("date") => "date",
                    Some("date-time") => "datetime-local",
                    _ => "text",
                }
                .to_string(),
            }
        })
        .collect()
}

/// enum (+ enumNames), or titled oneOf / anyOf `[{const, title}]`.
fn options(s: &Value) -> Vec<(String, String)> {
    if let Some(Value::Array(e)) = json::prop(s, "enum") {
        let names: Vec<String> = match json::prop(s, "enumNames") {
            Some(Value::Array(n)) => n.iter().map(json::to_text).collect(),
            _ => Vec::new(),
        };
        return e
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let v = json::to_text(v);
                (v.clone(), names.get(i).cloned().unwrap_or(v))
            })
            .collect();
    }
    match json::prop(s, "oneOf").or_else(|| json::prop(s, "anyOf")) {
        Some(Value::Array(o)) => o
            .iter()
            .filter_map(|x| {
                let c = json::to_text(json::prop(x, "const")?);
                Some((c.clone(), json::str(x, "title").map_or(c, str::to_string)))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Form content in schema order, or `(None, Some(label))` for the first field missing or invalid.
/// Integral f64 numbers are written as JSON integers (C# shortest round-trip: `36`, not `36.0`).
/// datetime-local goes out as RFC 3339 with the local offset.
pub fn content(
    fields: &[Field],
    text: &HashMap<String, String>,
    picks: &HashMap<String, BTreeSet<String>>,
) -> (Option<Map<String, Value>>, Option<String>) {
    let mut o = Map::new();
    for f in fields {
        let t = text.get(&f.key).map_or("", |s| s.trim());
        let invalid = || (None, Some(f.label.clone()));
        let v: Option<Value> = match f.kind {
            FieldKind::Boolean => Some(Value::Bool(t == "true")),
            FieldKind::Multi => picks.get(&f.key).filter(|s| !s.is_empty()).map(|set| {
                Value::Array(f.options.iter().filter(|x| set.contains(&x.0)).map(|x| Value::String(x.0.clone())).collect())
            }),
            FieldKind::Number | FieldKind::Integer if !t.is_empty() => {
                let Some(d) = t.parse::<f64>().ok().filter(|d| d.is_finite()) else { return invalid() };
                if (f.kind == FieldKind::Integer && d != d.floor())
                    || f.min.is_some_and(|m| d < m)
                    || f.max.is_some_and(|m| d > m)
                {
                    return invalid();
                }
                Some(number(d, f.kind == FieldKind::Integer))
            }
            FieldKind::String if f.input_type == "datetime-local" && !t.is_empty() => match local_rfc3339(t) {
                Some(s) => Some(Value::String(s)),
                None => return invalid(),
            },
            _ => (!t.is_empty()).then(|| Value::String(t.to_string())),
        };
        match v {
            Some(v) => {
                o.insert(f.key.clone(), v);
            }
            None if f.required => return invalid(),
            None => {} // blank optional fields are left out
        }
    }
    (Some(o), None)
}

/// Integer kind → i64 (truncated, like C# `(long)d`); number → integral values as integers, else f64.
fn number(d: f64, integer: bool) -> Value {
    if integer || (d.fract() == 0.0 && d.abs() < 9.007_199_254_740_992e15) {
        Value::from(d as i64)
    } else {
        Number::from_f64(d).map_or(Value::Null, Value::Number)
    }
}

/// `DateTime.TryParse` (invariant) of a datetime-local value → `yyyy-MM-ddTHH:mm:sszzz` in the local offset.
fn local_rfc3339(t: &str) -> Option<String> {
    let fmt = "%Y-%m-%dT%H:%M:%S%:z";
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(t) {
        return Some(d.with_timezone(&Local).format(fmt).to_string());
    }
    let naive = ["%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(t, f).ok())
        .or_else(|| chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)))?;
    let local = Local.from_local_datetime(&naive);
    Some(local.earliest().or_else(|| local.latest())?.format(fmt).to_string())
}

/// `{"action":…}` or `{"action":…,"content":…}` (that key order).
pub fn reply(action: &str, content: Option<Value>) -> Value {
    let mut o = Map::new();
    o.insert("action".into(), Value::String(action.to_string()));
    if let Some(c) = content {
        o.insert("content".into(), c);
    }
    Value::Object(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    // The probe's requests (claude 2.1.296, fake stdio server), trimmed.
    #[test]
    fn check() {
        let form = json!({"subtype":"elicitation","mcp_server_name":"probe","message":"What is your name?","mode":"form","requested_schema":{"type":"object","properties":{"name":{"type":"string","title":"Name","minLength":1},"age":{"type":"integer","minimum":0},"color":{"type":"string","enum":["red","blue"],"enumNames":["Red","Blue"]},"ok":{"type":"boolean","default":true},"tags":{"type":"array","items":{"anyOf":[{"const":"a","title":"A"},{"const":"b","title":"B"}]}},"when":{"type":"string","format":"date-time"}},"required":["name"]}});
        let fs = parse(&form);
        assert_eq!(fs.len(), 6);
        assert!(fs[0].key == "name" && fs[0].label == "Name" && fs[0].kind == FieldKind::String && fs[0].required);
        assert!(fs[1].kind == FieldKind::Integer && fs[1].min == Some(0.0) && !fs[1].required && fs[1].label == "age");
        assert_eq!((fs[2].kind, fs[2].options.clone()), (FieldKind::Enum, vec![("red".into(), "Red".into()), ("blue".into(), "Blue".into())]));
        assert_eq!((fs[3].kind, fs[3].default.as_deref()), (FieldKind::Boolean, Some("true")));
        assert_eq!((fs[4].kind, fs[4].options.clone()), (FieldKind::Multi, vec![("a".into(), "A".into()), ("b".into(), "B".into())]));
        assert_eq!(fs[5].input_type, "datetime-local");

        let picks: HashMap<String, BTreeSet<String>> = [("tags".to_string(), ["b".to_string(), "a".to_string()].into())].into();
        let (c, bad) = content(&fs, &text(&[("name", " Ada "), ("age", "36"), ("color", "blue"), ("ok", "true")]), &picks);
        assert_eq!(bad, None);
        assert_eq!(Value::Object(c.unwrap()).to_string(), r#"{"name":"Ada","age":36,"color":"blue","ok":true,"tags":["a","b"]}"#);
        let none = HashMap::new();
        assert_eq!(content(&fs, &text(&[("age", "3")]), &none).1.as_deref(), Some("Name"), "required missing");
        assert_eq!(content(&fs, &text(&[("name", "x"), ("age", "-1")]), &none).1.as_deref(), Some("age"));
        assert_eq!(content(&fs, &text(&[("name", "x"), ("age", "1.5")]), &none).1.as_deref(), Some("age"));
        assert_eq!(content(&fs, &text(&[("name", "x"), ("age", "abc")]), &none).1.as_deref(), Some("age"));
        let (c, _) = content(&fs, &text(&[("name", "x"), ("when", "2026-10-11T10:30")]), &none);
        let when = c.unwrap()["when"].as_str().unwrap().to_string();
        assert!(when.starts_with("2026-10-11T10:30:00") && chrono::DateTime::parse_from_rfc3339(&when).is_ok(), "{when}");
        assert_eq!(content(&fs, &text(&[("name", "x"), ("when", "soon")]), &none).1.as_deref(), Some("when"));

        let url_req = json!({"subtype":"elicitation","mcp_server_name":"probe","message":"Open the page to link your account","mode":"url","url":"https://example.com/link?x=1","elicitation_id":"el-42"});
        assert!(is_url(&url_req) && !is_url(&form));
        assert_eq!(url(&url_req).as_deref(), Some("https://example.com/link?x=1"));
        assert!(parse(&url_req).is_empty());
        assert_eq!(url(&json!({"url":"javascript:alert(1)"})), None, "non-http url dropped");
        assert_eq!(url(&json!({"url":"/relative"})), None);

        let mut p = PendingPermission {
            request_id: "r".into(),
            tool: TOOL.into(),
            input: url_req,
            description: None,
            tool_use_id: None,
            suggestions: None,
            at: chrono::DateTime::UNIX_EPOCH,
            info: None,
            target_exists: None,
        };
        assert!(matches(&p, "probe", "el-42") && !matches(&p, "probe", "el-1"));
        p.tool = "Bash".into();
        assert!(!matches(&p, "probe", "el-42"));

        assert_eq!(reply("decline", None).to_string(), r#"{"action":"decline"}"#);
        assert_eq!(reply("accept", Some(json!({"name":"Ada"}))).to_string(), r#"{"action":"accept","content":{"name":"Ada"}}"#);
    }

    #[test]
    fn number_kind_and_options() {
        let fs = parse(&json!({"requested_schema":{"properties":{
            "n":{"type":"number","maximum":100},
            "e":{"type":"string","enum":[1,"x"]},
            "o":{"type":"string","oneOf":[{"const":"a"},{"title":"no const"}]}
        },"required":["n"]}}));
        let none = HashMap::new();
        let ok = |t: &str| content(&fs[..1], &text(&[("n", t)]), &none).0.map(|m| m["n"].to_string());
        assert_eq!(ok("36").as_deref(), Some("36"));
        assert_eq!(ok("2.5").as_deref(), Some("2.5"));
        assert_eq!(ok("101"), None);
        assert_eq!(ok("NaN"), None);
        assert_eq!(fs[1].options, vec![("1".into(), "1".into()), ("x".into(), "x".into())]);
        assert_eq!((fs[2].kind, fs[2].options.clone()), (FieldKind::Enum, vec![("a".into(), "a".into())]));
    }
}
