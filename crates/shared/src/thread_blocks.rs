//! Port of `Core/ThreadBlocks.cs` — spec 02 §7. Owner: shared-pure-a.
//! Groups the thread's items into render blocks: consecutive top-level tools form a Run (Ledger, or Workflow row when
//! agents), hooks form a Hooks block that does NOT close the run before it.

//!
//! Blocks hold INDEXES into the item list (never clones): re-grouping on every patch is cheap, and each row reads
//! its own item signal (`ui::state::SessionHandle::row`).

use crate::model::Item;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Block {
    /// Indexes of ToolItems; agents = false → Ledger, true → Workflow row.
    Run { tools: Vec<usize>, agents: bool },
    /// Indexes of HookItems.
    Hooks(Vec<usize>),
    /// Index of an item rendered as is (User, Text top-level, Thought, Compact, Fallback, ApiError, Notice,
    /// Scheduled, Resumed, and top-level Workflow / SendUserFile tools as their own card).
    Item(usize),
}

/// Open run = last block is a Run, or the last two are `[Run, Hooks]`. Subagent items and Reset are dropped.
/// `hide_thoughts`: ThoughtItems are skipped as if absent (the C# UI filtered them out before grouping, so a hidden
/// thought never closes a run); indexes stay those of `items`.
/// `[Bash, hook, hook, Bash]` → `[Run{[0,3]}, Hooks{[1,2]}]`.
pub fn of<I: Borrow<Item>>(items: &[I], hide_thoughts: bool) -> Vec<Block> {
    todo!("port: ThreadBlocks.Of")
}
