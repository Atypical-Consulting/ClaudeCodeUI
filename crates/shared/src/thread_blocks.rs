//! Port of `Core/ThreadBlocks.cs` — spec 02 §7. Owner: shared-pure-a.
//! Groups the thread's items into render blocks: consecutive top-level tools form a Run (Ledger, or Workflow row when
//! agents), hooks form a Hooks block that does NOT close the run before it.

//!
//! Blocks hold INDEXES into the item list (never clones): re-grouping on every patch is cheap, and each row reads
//! its own item signal (`ui::state::SessionHandle::row`).

use crate::model::Item;
use crate::{tool_kinds, workflow};
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
    let mut blocks: Vec<Block> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item.borrow() {
            Item::Tool(t) if t.parent_tool_use_id.is_none() => {
                if t.name == workflow::TOOL || t.name == tool_kinds::SEND_FILE {
                    blocks.push(Block::Item(i));
                    continue;
                }
                let agent = tool_kinds::is_agent(&t.name);
                // Hooks fire between a tool_use and its result, or between two tool calls: a hook row follows the
                // run without closing it. Only a prompt, text or an API error ends a run.
                let n = blocks.len();
                let open = match blocks.as_slice() {
                    [.., Block::Run { .. }] => Some(n - 1),
                    [.., Block::Run { .. }, Block::Hooks(_)] => Some(n - 2),
                    _ => None,
                };
                match open.map(|at| &mut blocks[at]) {
                    Some(Block::Run { tools, agents }) if *agents == agent => tools.push(i),
                    _ => blocks.push(Block::Run { tools: vec![i], agents: agent }),
                }
            }
            Item::Hook(_) => match blocks.last_mut() {
                Some(Block::Hooks(h)) => h.push(i),
                _ => blocks.push(Block::Hooks(vec![i])),
            },
            Item::Text(t) if t.parent_tool_use_id.is_none() => blocks.push(Block::Item(i)),
            Item::Thought(_) if !hide_thoughts => blocks.push(Block::Item(i)),
            Item::User(_)
            | Item::ApiError(_)
            | Item::Notice(_)
            | Item::Scheduled(_)
            | Item::Resumed
            | Item::Compact(_)
            | Item::Fallback(_) => blocks.push(Block::Item(i)),
            _ => {}
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use serde_json::json;

    fn tool(id: &str, name: &str) -> Item {
        Item::Tool(ToolItem {
            id: id.into(),
            name: name.into(),
            input: json!({}),
            parent_tool_use_id: None,
            state: ToolState::Running,
            result_text: None,
            structured: None,
            started_at: chrono::DateTime::UNIX_EPOCH,
            ended_at: None,
            task_id: None,
            task_status: None,
            progress: None,
            tokens: 0,
            sub_tool_uses: 0,
            background: false,
            model: None,
        })
    }

    fn hook(name: &str, outcome: &str) -> Item {
        Item::Hook(HookItem {
            hook: HookEvt {
                name: name.into(),
                outcome: outcome.into(),
                exit_code: Some(if outcome == "error" { 2 } else { 0 }),
                output: "x".into(),
                id: None,
            },
        })
    }

    fn text() -> Item {
        Item::Text(TextItem { markdown: "t".into(), parent_tool_use_id: None, uuid: None, at: None })
    }

    fn thought() -> Item {
        Item::Thought(ThoughtItem { text: "t".into() })
    }

    fn run(tools: &[usize], agents: bool) -> Block {
        Block::Run { tools: tools.to_vec(), agents }
    }

    #[test]
    fn check() {
        let of = |b: Vec<Item>| of(&b, false);
        // claude 2.1.296 order: tool_use Bash, PreToolUse:Bash, tool_result, PostToolBatch, tool_use Bash.
        assert_eq!(
            of(vec![tool("b1", "Bash"), hook("PreToolUse:Bash", "success"), hook("PostToolBatch", "success"), tool("b2", "Bash")]),
            vec![run(&[0, 3], false), Block::Hooks(vec![1, 2])],
            "tool hooks keep one ledger"
        );
        assert_eq!(
            of(vec![tool("a1", "Agent"), hook("SubagentStart:general-purpose", "error"), tool("a2", "Agent"), tool("b", "Bash")]),
            vec![run(&[0, 2], true), Block::Hooks(vec![1]), run(&[3], false)],
            "agent fan-out keeps one workflow"
        );
        assert_eq!(
            of(vec![tool("b1", "Bash"), text(), tool("b2", "Bash")]),
            vec![run(&[0], false), Block::Item(1), run(&[2], false)],
            "text ends a run"
        );
        let user = Item::User(UserItem {
            text: "u".into(),
            at: chrono::DateTime::UNIX_EPOCH,
            ultracode: false,
            images: None,
            uuid: None,
        });
        assert_eq!(
            of(vec![user, Item::Resumed, tool("b", "Bash")]),
            vec![Block::Item(0), Block::Item(1), run(&[2], false)],
            "the re-run turn's label is shown"
        );
        assert_eq!(
            of(vec![tool("b1", "Bash"), tool("w", workflow::TOOL), tool("b2", "Bash")]),
            vec![run(&[0], false), Block::Item(1), run(&[2], false)],
            "a workflow is its own block"
        );
        assert_eq!(
            of(vec![tool("b1", "Bash"), tool("f", tool_kinds::SEND_FILE), thought(), tool("b2", "Bash")]),
            vec![run(&[0], false), Block::Item(1), Block::Item(2), run(&[3], false)],
            "a sent file and a thought are blocks"
        );
    }

    #[test]
    fn hidden_thoughts_and_dropped_items() {
        let mut sub = tool("s", "Bash");
        if let Item::Tool(t) = &mut sub {
            t.parent_tool_use_id = Some("a".into());
        }
        let items = vec![tool("b1", "Bash"), thought(), Item::Reset, sub, tool("b2", "Bash")];
        assert_eq!(of(&items, true), vec![run(&[0, 4], false)]);
        assert_eq!(of(&items, false), vec![run(&[0], false), Block::Item(1), run(&[4], false)]);
        let refs: Vec<&Item> = items.iter().collect();
        assert_eq!(of(&refs, true), vec![run(&[0, 4], false)]);
    }
}
