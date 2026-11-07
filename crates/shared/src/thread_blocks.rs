//! Port of `Core/ThreadBlocks.cs` — spec 02 §7. Owner: shared-pure-a.
//! Groups the thread's items into render blocks: consecutive top-level tools form a Run (Ledger, or Workflow row when
//! agents), hooks form a Hooks block that does NOT close the run before it.

use crate::model::{HookItem, Item, ToolItem};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Block {
    /// agents = false → Ledger, true → Workflow row.
    Run { tools: Vec<ToolItem>, agents: bool },
    Hooks(Vec<HookItem>),
    /// Rendered as is (User, Text top-level, Thought, Compact, Fallback, ApiError, Notice, Scheduled, Resumed, and
    /// top-level Workflow / SendUserFile tools as their own card).
    Item(Item),
}

/// Open run = last block is a Run, or the last two are `[Run, Hooks]`. Subagent items and Reset are dropped.
/// `[Bash, hook, hook, Bash]` → `[Run{b1,b2}, Hooks{2}]`. (The UI filters ThoughtItems out first when hidden.)
pub fn of(items: &[Item]) -> Vec<Block> {
    todo!("port: ThreadBlocks.Of")
}
