//! Port of `Core/AskUser.cs` (AskUserQuestion tool) — spec 02 §8. Owner: shared-pure-a.
//! Answer = allow with `updatedInput = input + "answers": {question text: answer}`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    todo!("port: AskUser.Parse")
}

/// Picked indexes (in range, distinct, ascending = option order) mapped to labels, then `other.trim()` if not blank;
/// single-select keeps only the first part; joined ", ". "" = unanswered.
pub fn answer(q: &Question, picked: &[usize], other: Option<&str>) -> String {
    todo!("port: AskUser.Answer")
}

/// Clone of `input` with `answers` set/overwritten (map in question order).
pub fn with_answers(input: &Value, answers: &[(String, String)]) -> Value {
    todo!("port: AskUser.WithAnswers")
}

/// `tool_use_result.answers` (string values only), JSON order.
pub fn answered(structured: Option<&Value>) -> Vec<(String, String)> {
    todo!("port: AskUser.Answered")
}
