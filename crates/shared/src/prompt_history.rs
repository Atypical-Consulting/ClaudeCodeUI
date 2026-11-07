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
        todo!("port: PromptHistory.Load")
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    /// +1 older (↑), -1 newer (↓). Out of range → None. Leaving the draft saves `current`.
    pub fn r#move(&mut self, step: i32, current: &str) -> Option<String> {
        todo!("port: PromptHistory.Move")
    }

    /// The saved draft while walking, else None.
    pub fn draft(&self) -> Option<&str> {
        todo!("port: PromptHistory.Draft")
    }

    /// Text edited or sent: it is the draft again.
    pub fn reset(&mut self) {
        todo!("port: PromptHistory.Reset")
    }
}
