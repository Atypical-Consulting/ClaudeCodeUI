using System.Text.RegularExpressions;
using Markdig;
using Markdig.Renderers.Html;
using Markdig.Syntax;
using Markdig.Syntax.Inlines;

namespace ClaudeCodeUI;

// Markdown to HTML. Raw HTML and {attributes} disabled: model output is rendered as markdown only, never as live markup.
// Links keep http(s)/mailto only, http(s) in a new tab (the desktop shell hands those to the system browser); any other or
// relative link (`Program.cs:12`, `docs/a.md`) becomes its text, since it would navigate away from the session. Images
// become links: nothing the model writes makes the browser fetch or run anything.
// Code blocks are wrapped in .cb here (server side: Blazor must own every top-level node), alert titles are French.
public static partial class Md
{
    static readonly MarkdownPipeline Pipeline = Build();

    static MarkdownPipeline Build()
    {
        var b = new MarkdownPipelineBuilder().UseAdvancedExtensions().DisableHtml();
        b.Extensions.RemoveAll(e => e is Markdig.Extensions.GenericAttributes.GenericAttributesExtension);
        return b.Build();
    }

    static bool External(string? url, out bool web)
    {
        web = false;
        if (url is null || !Uri.TryCreate(url.Trim(), UriKind.Absolute, out var u) || u.Scheme is not ("http" or "https" or "mailto")) return false;
        web = u.Scheme != "mailto";
        return true;
    }

    static readonly Dictionary<string, string> Lang = new()
    {
        ["csharp"] = "C#", ["cs"] = "C#", ["bash"] = "Bash", ["sh"] = "Shell", ["shell"] = "Shell", ["powershell"] = "PowerShell", ["ps1"] = "PowerShell",
        ["json"] = "JSON", ["diff"] = "Diff", ["javascript"] = "JavaScript", ["js"] = "JavaScript", ["typescript"] = "TypeScript", ["ts"] = "TypeScript",
        ["xml"] = "XML", ["html"] = "HTML", ["css"] = "CSS", ["sql"] = "SQL", ["yaml"] = "YAML", ["python"] = "Python",
    };

    [GeneratedRegex("""<pre><code(?: class="language-([^"]+)")?>""")] private static partial Regex CodeOpen();
    [GeneratedRegex("""(<p class="markdown-alert-title">.*?)(Note|Tip|Important|Warning|Caution)</p>""")] private static partial Regex AlertTitle();

    public static string Render(string markdown)
    {
        var doc = Markdown.Parse(markdown, Pipeline);
        foreach (var l in doc.Descendants<LinkInline>().ToList())
        {
            l.IsImage = false;
            if (External(l.Url, out var web))
            {
                if (web) { var a = l.GetAttributes(); a.AddProperty("target", "_blank"); a.AddProperty("rel", "noopener noreferrer"); }
                continue;
            }
            l.ReplaceBy(new ContainerInline());   // keeps the label (children moved over), renders without a tag
        }
        foreach (var l in doc.Descendants<AutolinkInline>().ToList())
            if (!l.IsEmail && !External(l.Url, out _)) l.ReplaceBy(new LiteralInline(l.Url));
        var html = doc.ToHtml(Pipeline);
        html = CodeOpen().Replace(html, m =>
        {
            var lang = m.Groups[1].Success ? m.Groups[1].Value : null;   // already HTML-encoded by Markdig
            var label = lang is null ? Strings.Get("Md.PlainText") : Lang.GetValueOrDefault(lang.ToLowerInvariant(), lang);
            return $"""<div class="cb"><div class="h"><span>{label}</span><button class="copy" type="button"><svg class="i sm"><use href="#i-copy"/></svg><span>{Strings.Get("Md.Copy")}</span></button></div>{m.Value.Replace("<pre>", "<pre tabindex=\"0\">")}""";
        });
        html = html.Replace("</code></pre>", "</code></pre></div>");
        return AlertTitle().Replace(html, m => m.Groups[1].Value + Strings.Get("Md.Alert." + m.Groups[2].Value) + "</p>");
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Md: " + what);
        var cs = Render("```csharp\nvar x = 1;\n```");
        Ok(cs.StartsWith("""<div class="cb"><div class="h"><span>C#</span><button class="copy" """) && cs.Contains("""<code class="language-csharp">""") && cs.TrimEnd().EndsWith("</code></pre></div>"), "```csharp → .cb « C# » : " + cs);
        var bare = Render("```\nplain\n```");
        Ok(bare.Contains("<span>texte</span>") && bare.TrimEnd().EndsWith("</div>"), "block without language → « texte »: " + bare);
        var warn = Render("> [!WARNING]\n> attention ici");
        Ok(warn.Contains("markdown-alert-warning") && warn.Contains("Attention</p>") && !warn.Contains("Warning</p>"), "[!WARNING] → « Attention » : " + warn);
        Ok(Render("<script>x</script>").Contains("&lt;script&gt;"), "raw HTML disabled");
        var at = Render("# a {onclick=x}") + Render("[x](https://a){onmouseover=\"y\"}");
        Ok(!at.Contains(" onclick=\"") && !at.Contains(" onmouseover=\""), "generic attributes disabled: " + at);
        var js = Render("[x](javascript:alert(1)) <javascript:alert(2)> [y](data:text/html,z) [r][1]\n\n[1]: javascript:alert(3)");
        Ok(!js.Contains("href=\"javascript:") && !js.Contains("href=\"data:") && !js.Contains("<a "), "javascript:/data: links neutralized: " + js);
        var img = Render("![a](https://evil.example/?d=s) [ok](https://x.y/z) [rel](docs/a.md) [*f*](Program.cs:12) [#](#top)");
        Ok(!img.Contains("<img") && img.Contains("href=\"https://x.y/z\" target=\"_blank\" rel=\"noopener noreferrer\"") && img.Contains("rel <em>f</em> #")
            && img.Split("<a ").Length == 3, "images turned into links, http kept in a new tab, relative and file links become text: " + img);
        Ok(Render("```foo\"><b\nx\n```") is var odd && !odd.Contains("<b>"), "encoded language");
    }
}
