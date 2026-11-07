//! Port of `Core/ListNav.cs` — spec 05 §9. Owner: shared-pure-b.
//! Used by QuestionForm, MentionPopover, ModeMenu, ModelMenu.

/// count ≤ 0 → -1. ArrowDown: in range ? (sel+1)%count : 0; ArrowUp: in range ? (sel-1+count)%count : count-1;
/// Home → 0; End → count-1; other keys → sel.
pub fn next(key: &str, sel: i32, count: i32) -> i32 {
    todo!("port: ListNav.Next")
}
