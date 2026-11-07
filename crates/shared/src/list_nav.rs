//! Port of `Core/ListNav.cs` — spec 05 §9. Owner: shared-pure-b.
//! Used by QuestionForm, MentionPopover, ModeMenu, ModelMenu.

/// count ≤ 0 → -1. ArrowDown: in range ? (sel+1)%count : 0; ArrowUp: in range ? (sel-1+count)%count : count-1;
/// Home → 0; End → count-1; other keys → sel.
pub fn next(key: &str, sel: i32, count: i32) -> i32 {
    if count <= 0 {
        return -1;
    }
    let in_range = (0..count).contains(&sel);
    match key {
        "ArrowDown" => if in_range { (sel + 1) % count } else { 0 },
        "ArrowUp" => if in_range { (sel - 1 + count) % count } else { count - 1 },
        "Home" => 0,
        "End" => count - 1,
        _ => sel,
    }
}

#[cfg(test)]
mod tests {
    use super::next;

    #[test]
    fn list_nav_check() {
        assert_eq!(next("ArrowDown", 0, 3), 1);
        assert_eq!(next("ArrowDown", 2, 3), 0);
        assert_eq!(next("ArrowUp", 0, 3), 2);
        assert_eq!(next("ArrowUp", 2, 3), 1);
        assert_eq!(next("Home", 2, 3), 0);
        assert_eq!(next("End", 0, 3), 2);
        assert_eq!(next("a", 1, 3), 1);
        assert_eq!(next("ArrowDown", -1, 3), 0);
        assert_eq!(next("ArrowUp", -1, 3), 2);
        assert_eq!(next("ArrowDown", 0, 0), -1);
    }
}
