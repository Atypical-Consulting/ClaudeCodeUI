//! Port of `Core/AskUser.cs` (AskUserQuestion tool) — spec 02 §8. Owner: shared-pure-a.
//! Answer = allow with `updatedInput = input + "answers": {question text: answer}`.

use crate::json;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const TOOL: &str = "AskUserQuestion";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AskOption {
    pub label: String,
    pub description: String,
    /// Markdown shown while the option is focused (only if non-empty).
    pub preview: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub text: String,
    pub header: String,
    pub options: Vec<AskOption>,
    pub multi: bool,
}

/// `questions[]` of the tool input; none → [].
pub fn parse(input: &Value) -> Vec<Question> {
    let Some(Value::Array(qs)) = json::prop(input, "questions") else { return Vec::new() };
    qs.iter()
        .map(|q| Question {
            text: json::string(q, "question"),
            header: json::string(q, "header"),
            options: match json::prop(q, "options") {
                Some(Value::Array(os)) => os
                    .iter()
                    .map(|o| AskOption {
                        label: json::string(o, "label"),
                        description: json::string(o, "description"),
                        preview: json::non_empty(o, "preview"),
                    })
                    .collect(),
                _ => Vec::new(),
            },
            multi: json::bool(q, "multiSelect"),
        })
        .collect()
}

/// Picked indexes (in range, distinct, ascending = option order) mapped to labels, then `other.trim()` if not blank;
/// single-select keeps only the first part; joined ", ". "" = unanswered.
pub fn answer(q: &Question, picked: &[usize], other: Option<&str>) -> String {
    let mut idx: Vec<usize> = picked.iter().copied().filter(|&i| i < q.options.len()).collect();
    idx.sort_unstable();
    idx.dedup();
    let mut parts: Vec<&str> = idx.iter().map(|&i| q.options[i].label.as_str()).collect();
    if let Some(o) = other.map(str::trim).filter(|o| !o.is_empty()) {
        parts.push(o);
    }
    if !q.multi {
        parts.truncate(1);
    }
    parts.join(", ")
}

/// Clone of `input` with `answers` set/overwritten (map in question order).
pub fn with_answers(input: &Value, answers: &[(String, String)]) -> Value {
    let mut o = input.as_object().cloned().unwrap_or_default();
    let a: Map<String, Value> = answers.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
    o.insert("answers".into(), Value::Object(a));
    Value::Object(o)
}

/// `tool_use_result.answers` (string values only), JSON order.
pub fn answered(structured: Option<&Value>) -> Vec<(String, String)> {
    match structured.and_then(|s| json::prop(s, "answers")) {
        Some(Value::Object(a)) => {
            a.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Real can_use_tool input and tool_use_result from the probe captures (claude 2.1.296).
    #[test]
    fn check() {
        let input = json!({"questions":[{"question":"Which toppings do you want?","header":"Toppings","options":[{"label":"Cheese","description":"Cheese topping"},{"label":"Ham","description":"Ham topping"},{"label":"Olives","description":"Olives topping"}],"multiSelect":true},{"question":"Which color do you prefer?","header":"Color","options":[{"label":"Red","description":"Choose red"},{"label":"Blue","description":"Choose blue"}],"multiSelect":false}]});
        let qs = parse(&input);
        assert_eq!(qs.len(), 2);
        assert!(qs[0].header == "Toppings" && qs[0].multi && qs[0].options.len() == 3);
        assert_eq!((qs[0].options[0].label.as_str(), qs[0].options[0].description.as_str()), ("Cheese", "Cheese topping"));
        assert!(qs[1].text == "Which color do you prefer?" && !qs[1].multi && qs[1].options.len() == 2);
        assert!(parse(&json!({})).is_empty(), "parse without questions");

        let p = parse(&json!({"questions":[{"question":"Which layout?","header":"Layout","options":[{"label":"Grid","description":"Cards","preview":"```\n[ ][ ]\n```"},{"label":"List","description":"Rows","preview":""}],"multiSelect":false}]}));
        assert_eq!(p[0].options[0].preview.as_deref(), Some("```\n[ ][ ]\n```"));
        assert_eq!(p[0].options[1].preview, None);
        assert_eq!(qs[0].options[0].preview, None);

        assert_eq!(answer(&qs[0], &[2, 0], None), "Cheese, Olives", "multi joins labels in option order");
        assert_eq!(answer(&qs[0], &[1, 1], Some(" Pineapple ")), "Ham, Pineapple", "multi + free text");
        assert_eq!(answer(&qs[1], &[1], None), "Blue");
        assert_eq!(answer(&qs[1], &[], Some("Chartreuse")), "Chartreuse");
        assert_eq!(answer(&qs[1], &[], Some("  ")), "");
        assert_eq!(answer(&qs[0], &[7], None), "");

        let upd = with_answers(&input, &[(qs[1].text.clone(), "Blue".into())]);
        assert_eq!(upd["questions"].as_array().map(Vec::len), Some(2));
        assert_eq!(upd["answers"]["Which color do you prefer?"], "Blue", "updatedInput = input + answers");
        let again = with_answers(&upd, &[("Q".into(), "A".into())]);
        assert_eq!(again["answers"], json!({"Q":"A"}), "answers overwritten");

        let result = json!({"questions":[],"answers":{"Which color do you prefer?":"Chartreuse","n":1}});
        assert_eq!(answered(Some(&result)), vec![("Which color do you prefer?".to_string(), "Chartreuse".to_string())]);
        assert!(answered(None).is_empty() && answered(Some(&json!({"answers":{}}))).is_empty(), "not answered");
    }
}
