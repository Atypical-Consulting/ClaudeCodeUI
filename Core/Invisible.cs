using System.Text;

namespace ClaudeCodeUI;

// Characters a pasted prompt can carry without showing them: tag characters (U+E0000–E007F, "ASCII smuggling"),
// zero-width space and word joiners, the BOM, and bidi embeddings, overrides and isolates (Trojan Source).
// ZWJ / ZWNJ stay: emoji sequences and Persian or Indic text need them, as do the tags of a subdivision flag.
public static class Invisible
{
    public static bool Hidden(int c) => c is 0x200B or (>= 0x2060 and <= 0x2064) or 0xFEFF or (>= 0x202A and <= 0x202E)
                                        or (>= 0x2066 and <= 0x2069) or (>= 0xE0000 and <= 0xE007F);

    // The text without them, and how many went.
    public static (string Text, int Removed) Strip(string text)
    {
        var sb = new StringBuilder(text.Length);
        var n = 0;
        var r = text.EnumerateRunes().ToArray();
        for (var i = 0; i < r.Length; i++)
        {
            // England, Scotland, Wales: U+1F3F4, tag letters, then the cancel tag U+E007F.
            if (r[i].Value == 0x1F3F4)
            {
                var j = i + 1;
                while (j < r.Length && r[j].Value is >= 0xE0020 and <= 0xE007E) j++;
                if (j > i + 1 && j < r.Length && r[j].Value == 0xE007F)
                {
                    for (; i <= j; i++) sb.Append(r[i].ToString());
                    i--;
                    continue;
                }
            }
            if (Hidden(r[i].Value)) n++;
            else sb.Append(r[i].ToString());
        }
        return n == 0 ? (text, 0) : (sb.ToString(), n);
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Invisible: " + what);
        Ok(Strip("plain text, é, 👨‍👩‍👧") is ("plain text, é, 👨‍👩‍👧", 0), "nothing to remove (ZWJ emoji kept)");
        Ok(Strip("ok​﻿") is ("ok", 2), "zero-width and BOM");
        Ok(Strip("a‮b⁦c⁩") is ("abc", 3), "bidi controls");
        Ok(Strip("hi" + char.ConvertFromUtf32(0xE0049) + char.ConvertFromUtf32(0xE0067) + char.ConvertFromUtf32(0xE007F)) is ("hi", 3), "tag characters");
        const string england = "\U0001F3F4\U000E0067\U000E0062\U000E0065\U000E006E\U000E0067\U000E007F";
        Ok(Strip("go " + england) is ("go " + england, 0), "subdivision flag kept");
        Ok(Strip("\U0001F3F4\U000E0067x") is ("\U0001F3F4x", 1), "unterminated tags after a flag still go");
    }
}
