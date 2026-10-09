namespace ClaudeCodeUI;

// Key -> active-option index for keyboard-driven listboxes.
public static class ListNav
{
    public static int Next(string key, int sel, int count)
    {
        if (count <= 0) return -1;
        var inRange = sel >= 0 && sel < count;
        return key switch
        {
            "ArrowDown" => inRange ? (sel + 1) % count : 0,
            "ArrowUp" => inRange ? (sel - 1 + count) % count : count - 1,
            "Home" => 0,
            "End" => count - 1,
            _ => sel,
        };
    }

    public static void Check()
    {
        SelfCheck.Assert(Next("ArrowDown", 0, 3) == 1, "ListNav: ArrowDown 0/3 -> 1");
        SelfCheck.Assert(Next("ArrowDown", 2, 3) == 0, "ListNav: ArrowDown wraps to 0");
        SelfCheck.Assert(Next("ArrowUp", 0, 3) == 2, "ListNav: ArrowUp wraps to count-1");
        SelfCheck.Assert(Next("ArrowUp", 2, 3) == 1, "ListNav: ArrowUp 2/3 -> 1");
        SelfCheck.Assert(Next("Home", 2, 3) == 0, "ListNav: Home -> 0");
        SelfCheck.Assert(Next("End", 0, 3) == 2, "ListNav: End -> count-1");
        SelfCheck.Assert(Next("a", 1, 3) == 1, "ListNav: other key keeps sel");
        SelfCheck.Assert(Next("ArrowDown", -1, 3) == 0, "ListNav: ArrowDown from none -> 0");
        SelfCheck.Assert(Next("ArrowUp", -1, 3) == 2, "ListNav: ArrowUp from none -> last");
        SelfCheck.Assert(Next("ArrowDown", 0, 0) == -1, "ListNav: empty list -> -1");
    }
}
