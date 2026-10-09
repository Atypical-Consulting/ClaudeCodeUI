using System.Text.RegularExpressions;
using Markdig;

namespace ClaudeCodeUI;

// Markdown to HTML. Raw HTML disabled: model output is rendered as markdown only, never as live markup.
// Code blocks are wrapped in .cb here (server side: Blazor must own every top-level node), alert titles are French.
public static partial class Md
{
    static readonly MarkdownPipeline Pipeline = new MarkdownPipelineBuilder().UseAdvancedExtensions().DisableHtml().Build();

    static readonly Dictionary<string, string> Lang = new()
    {
        ["csharp"] = "C#", ["cs"] = "C#", ["bash"] = "Bash", ["sh"] = "Shell", ["shell"] = "Shell", ["powershell"] = "PowerShell", ["ps1"] = "PowerShell",
        ["json"] = "JSON", ["diff"] = "Diff", ["javascript"] = "JavaScript", ["js"] = "JavaScript", ["typescript"] = "TypeScript", ["ts"] = "TypeScript",
        ["xml"] = "XML", ["html"] = "HTML", ["css"] = "CSS", ["sql"] = "SQL", ["yaml"] = "YAML", ["python"] = "Python",
    };

    static readonly Dictionary<string, string> Alert = new()
        { ["Note"] = "Note", ["Tip"] = "Astuce", ["Important"] = "Important", ["Warning"] = "Attention", ["Caution"] = "Prudence" };

    [GeneratedRegex("""<pre><code(?: class="language-([^"]+)")?>""")] private static partial Regex CodeOpen();
    [GeneratedRegex("""(<p class="markdown-alert-title">.*?)(Note|Tip|Important|Warning|Caution)</p>""")] private static partial Regex AlertTitle();

    public static string Render(string markdown)
    {
        var html = Markdown.ToHtml(markdown, Pipeline);
        html = CodeOpen().Replace(html, m =>
        {
            var lang = m.Groups[1].Success ? m.Groups[1].Value : null;   // already HTML-encoded by Markdig
            var label = lang is null ? "texte" : Lang.GetValueOrDefault(lang.ToLowerInvariant(), lang);
            return $"""<div class="cb"><div class="h"><span>{label}</span><button class="copy" type="button"><svg class="i sm"><use href="#i-copy"/></svg><span>Copier</span></button></div>{m.Value}""";
        });
        html = html.Replace("</code></pre>", "</code></pre></div>");
        return AlertTitle().Replace(html, m => m.Groups[1].Value + Alert[m.Groups[2].Value] + "</p>");
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Md: " + what);
        var cs = Render("```csharp\nvar x = 1;\n```");
        Ok(cs.StartsWith("""<div class="cb"><div class="h"><span>C#</span><button class="copy" """) && cs.Contains("""<code class="language-csharp">""") && cs.TrimEnd().EndsWith("</code></pre></div>"), "```csharp → .cb « C# » : " + cs);
        var bare = Render("```\nplain\n```");
        Ok(bare.Contains("<span>texte</span>") && bare.TrimEnd().EndsWith("</div>"), "bloc sans langage → « texte » : " + bare);
        var warn = Render("> [!WARNING]\n> attention ici");
        Ok(warn.Contains("markdown-alert-warning") && warn.Contains("Attention</p>") && !warn.Contains("Warning</p>"), "[!WARNING] → « Attention » : " + warn);
        Ok(Render("<script>x</script>").Contains("&lt;script&gt;"), "HTML brut désactivé");
        Ok(Render("```foo\"><b\nx\n```") is var odd && !odd.Contains("<b>"), "langage encodé");
    }
}
