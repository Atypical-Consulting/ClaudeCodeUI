using Markdig;

namespace ClaudeCodeUI;

// Markdown to HTML. Raw HTML disabled: model output is rendered as markdown only, never as live markup.
// WP1 adds the server-side .cb wrapping and the French alert titles.
public static class Md
{
    static readonly MarkdownPipeline Pipeline = new MarkdownPipelineBuilder().UseAdvancedExtensions().DisableHtml().Build();

    public static string Render(string markdown) => Markdown.ToHtml(markdown, Pipeline);

    internal static void Check() { }
}
