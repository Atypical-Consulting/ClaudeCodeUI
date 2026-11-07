//! Port of `Core/PromptHistory.cs` (Composer ↑/↓ recall) — spec 03 §3. Owner: shared-pure-b.
//! Front-end state, one per Composer; entries = this session's prompts then `past_prompts(cwd, except_id)`.

#[derive(Clone, Debug, PartialEq)]
pub struct PromptHistory {
    /// Newest first.
    entries: Vec<String>,
    /// -1 = the draft being typed.
    index: i32,
    draft: String,
}

impl Default for PromptHistory {
    fn default() -> Self {
        PromptHistory { entries: Vec::new(), index: -1, draft: String::new() }
    }
}

impl PromptHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop blank, keep the first (newest) of duplicates; index = -1.
    pub fn load(&mut self, newest_first: Vec<String>) {
        let mut seen = std::collections::HashSet::new();
        self.entries = newest_first.into_iter().filter(|s| !s.trim().is_empty() && seen.insert(s.clone())).collect();
        self.index = -1;
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    /// +1 older (↑), -1 newer (↓). Out of range → None. Leaving the draft saves `current`.
    pub fn r#move(&mut self, step: i32, current: &str) -> Option<String> {
        let next = self.index + step;
        if next < -1 || next >= self.entries.len() as i32 {
            return None;
        }
        if self.index == -1 {
            self.draft = current.to_string();
        }
        self.index = next;
        Some(if next == -1 { self.draft.clone() } else { self.entries[next as usize].clone() })
    }

    /// The saved draft while walking, else None.
    pub fn draft(&self) -> Option<&str> {
        (self.index >= 0).then_some(self.draft.as_str())
    }

    /// Text edited or sent: it is the draft again.
    pub fn reset(&mut self) {
        self.index = -1;
    }
}

#[cfg(test)]
mod tests {
    use super::PromptHistory;

    #[test]
    fn prompt_history_check() {
        let mut h = PromptHistory::new();
        assert!(h.r#move(1, "x").is_none() && h.index() == -1, "empty history");
        h.load(["b", "a", "b", " ", "c"].map(String::from).to_vec());
        assert_eq!(h.count(), 3, "dedup + blanks");
        assert!(h.r#move(-1, "draft").is_none(), "↓ at the draft");
        assert_eq!(h.r#move(1, "draft").as_deref(), Some("b"));
        assert_eq!(h.r#move(1, "b").as_deref(), Some("a"));
        assert_eq!(h.r#move(1, "a").as_deref(), Some("c"));
        assert!(h.r#move(1, "c").is_none() && h.index() == 2, "↑ stops at the oldest");
        assert_eq!(h.draft(), Some("draft"));
        assert_eq!(h.r#move(-1, "c").as_deref(), Some("a"));
        assert_eq!(h.r#move(-1, "a").as_deref(), Some("b"));
        assert_eq!(h.r#move(-1, "b").as_deref(), Some("draft"));
        assert_eq!(h.draft(), None);
        h.r#move(1, "draft 2");
        h.reset();
        assert_eq!(h.r#move(1, "edited").as_deref(), Some("b"));
        assert_eq!(h.r#move(-1, "b").as_deref(), Some("edited"), "an edit becomes the draft");
    }
}
