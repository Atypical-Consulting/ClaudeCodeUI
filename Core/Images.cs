using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// An image or document block of a user message: pasted or dropped in the Composer, or read back from a transcript.
// Url is a data: URL; null when the block carries no inline base64 the thread can show (it shows a placeholder).
// Name: a document's title (its file name), which the model sees.
public sealed record UserImage(string MediaType, string? Url, string? Name = null)
{
    // The base64 payload, as the stream-json image source wants it.
    public string Data => Url is { } u ? u[(u.IndexOf(',') + 1)..] : "";
    public bool Document => MediaType is Images.Pdf or Images.Text;
}

public static class Images
{
    // The Messages API caps an image at 5 MB. The CLI re-encodes what it is given before the API sees it (a 12 MB PNG
    // and an 8550 px one were read correctly, docs/PLAN.md §1.1), so this is the UI's own cap on what a message carries
    // through the circuit and the session's memory, not the CLI's. Raise it here if screenshots hit it.
    public const long MaxBytes = 5 * 1024 * 1024;
    public const int MaxCount = 10;   // per message

    // Media type from the magic bytes, not from the browser's claim: the four types the API accepts, else null.
    public static string? Sniff(ReadOnlySpan<byte> b) => b switch
    {
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x47, 0x49, 0x46, 0x38, 0x37 or 0x39, 0x61, ..] => "image/gif",                 // GIF87a, GIF89a
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x45, 0x42, 0x50, ..] => "image/webp", // RIFF….WEBP
        _ => null,
    };

    public const string Pdf = "application/pdf", Text = "text/plain";
    static readonly System.Text.UTF8Encoding Strict = new(false, true);

    // An image, else a PDF ("%PDF-"), else UTF-8 text with no NUL: the document blocks the API takes (base64 and text
    // sources, both answered by the model on claude 2.1.296). Anything else is refused.
    public static UserImage? From(byte[] bytes, string? name = null)
    {
        var type = Sniff(bytes) ?? (bytes.AsSpan().StartsWith("%PDF-"u8) ? Pdf : IsText(bytes) ? Text : null);
        return type is null ? null : new(type, $"data:{type};base64,{Convert.ToBase64String(bytes)}", type is Pdf or Text ? name : null);
    }

    static bool IsText(byte[] b)
    {
        if (b.Length == 0 || b.Contains((byte)0)) return false;
        try { Strict.GetString(b); return true; }
        catch (System.Text.DecoderFallbackException) { return false; }
    }

    // A user content image block, as stream-json carries it and the transcript stores it. Anything but an inline
    // base64 source of a known type becomes a placeholder.
    internal static UserImage Parse(JsonElement block)
    {
        var src = Events.Prop(block, "source") ?? default;
        var type = Events.Str(src, "media_type") ?? "";
        if (Events.Str(block, "type") == "document")
            return (Events.Str(src, "type"), Events.Str(src, "data")) switch
            {
                ("base64", { Length: > 0 } d) when type == Pdf => new(Pdf, $"data:{Pdf};base64,{d}", Events.Str(block, "title")),
                ("text", { } t) => new(Text, $"data:{Text};base64,{Convert.ToBase64String(System.Text.Encoding.UTF8.GetBytes(t))}", Events.Str(block, "title")),
                _ => new(type.Length > 0 ? type : Pdf, null, Events.Str(block, "title")),
            };
        return Events.Str(src, "type") == "base64" && type is "image/png" or "image/jpeg" or "image/gif" or "image/webp"
               && Events.Str(src, "data") is { Length: > 0 } data
            ? new(type, $"data:{type};base64,{data}")
            : new(type, null);
    }

    // The `content` of a user message: the plain string, or a text block (when there is text) then one image block per
    // image. Both shapes, image-only included, verified by --probe-cli image.
    internal static JsonNode Content(string text, IReadOnlyList<UserImage>? images)
    {
        if (images is not { Count: > 0 }) return text;
        var blocks = new JsonArray();
        if (text.Length > 0) blocks.Add(new JsonObject { ["type"] = "text", ["text"] = text });
        foreach (var i in images)
        {
            var block = new JsonObject
            {
                ["type"] = i.Document ? "document" : "image",
                ["source"] = i.MediaType == Text
                    ? new JsonObject { ["type"] = "text", ["media_type"] = Text, ["data"] = System.Text.Encoding.UTF8.GetString(Convert.FromBase64String(i.Data)) }
                    : new JsonObject { ["type"] = "base64", ["media_type"] = i.MediaType, ["data"] = i.Data },
            };
            if (i is { Document: true, Name: { } n }) block["title"] = n;
            blocks.Add(block);
        }
        return blocks;
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Images: " + what);
        Ok(Sniff(CliProbe.DigitsPng("42")) == "image/png" && Sniff([0xFF, 0xD8, 0xFF, 0xE0]) == "image/jpeg"
           && Sniff("GIF89a.."u8) == "image/gif" && Sniff("RIFF\0\0\0\0WEBPVP8L"u8) == "image/webp", "sniff");
        Ok(Sniff("RIFF\0\0\0\0WAVE"u8) is null && Sniff("<svg"u8) is null && Sniff([]) is null, "sniff rejects");
        var img = From([0x89, (byte)'P', (byte)'N', (byte)'G', 13, 10, 26, 10, 1]);
        Ok(img is { MediaType: "image/png", Url: "data:image/png;base64,iVBORw0KGgoB", Data: "iVBORw0KGgoB" }, "From");

        // What the Composer writes on stdin reads back through Events as the same images (transcript replay).
        static UserTextEvt[] RoundTrip(string text, UserImage[] images) =>
            [.. Events.ParseAll(JsonDocument.Parse(new JsonObject
            {
                ["type"] = "user",
                ["message"] = new JsonObject { ["role"] = "user", ["content"] = Content(text, images) },
            }.ToJsonString()).RootElement.Clone()).OfType<UserTextEvt>()];
        Ok(Content("hi", null).GetValueKind() == JsonValueKind.String && Content("hi", []).GetValueKind() == JsonValueKind.String, "text only stays a string");
        Ok(RoundTrip("what is this?", [img!, img!]) is [{ Text: "what is this?", Images: [{ Url: "data:image/png;base64,iVBORw0KGgoB" }, _] }], "text + images");
        Ok(RoundTrip("", [img!]) is [{ Text: "", Images: [{ MediaType: "image/png" }] }], "image only");

        // Real transcript line shape (CLI 2.1.296), then a source the thread cannot show.
        var line = Events.ParseAll(JsonDocument.Parse("""{"type":"user","message":{"role":"user","content":[{"type":"text","text":"What number?"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}]},"imagePasteIds":[1],"timestamp":"2026-10-09T21:27:11.608Z"}""").RootElement.Clone()).ToArray();
        Ok(line is [UserTextEvt { Text: "What number?", At: not null, Images: [{ Url: "data:image/png;base64,iVBORw0KGgo=" }] }], "transcript line");
        var s = new LiveSession("i", "i", @"C:\w", "default");
        s.BeginTurn("", [img!]);
        s.Apply(line[0]);
        Ok(s.Items is [UserItem { Text: "", Images: [_] }, UserItem { Text: "What number?", Images: [_] }], "reducer keeps the images");
        // Documents: the blocks the probe sent (a PDF and a text file, both read by the model), written then read back.
        var pdf = From("%PDF-1.4\n"u8.ToArray(), "a.pdf");
        var txt = From("The word is GIRAFFE88."u8.ToArray(), "notes.txt");
        Ok(pdf is { MediaType: Pdf, Document: true, Name: "a.pdf" } && txt is { MediaType: Text, Name: "notes.txt" } && From([0x41, 0, 0x42], "x.bin") is null
           && From([0xC3, 0x28], "bad.txt") is null && From(CliProbe.DigitsPng("1"), "p.png") is { Name: null, Document: false }, "documents sniffed");
        var sent = Content("both words?", [pdf!, txt!]).ToJsonString();
        Ok(sent.Contains("\"type\":\"document\",\"source\":{\"type\":\"base64\",\"media_type\":\"application/pdf\"") && sent.Contains("\"title\":\"a.pdf\"")
           && sent.Contains("{\"type\":\"text\",\"media_type\":\"text/plain\",\"data\":\"The word is GIRAFFE88.\"}"), $"document blocks: {sent}");
        Ok(RoundTrip("both words?", [pdf!, txt!]) is [{ Images: [{ MediaType: Pdf, Name: "a.pdf", Url: not null }, { MediaType: Text, Name: "notes.txt" } t] }]
           && t.Data == txt!.Data, "documents read back");

        var odd = Events.ParseAll(JsonDocument.Parse("""{"type":"user","message":{"role":"user","content":[{"type":"image","source":{"type":"url","url":"https://x/y.png"}},{"type":"image","source":{"type":"base64","media_type":"image/svg+xml","data":"PHN2Zz4="}},{"type":"image"}]}}""").RootElement.Clone()).ToArray();
        Ok(odd is [UserTextEvt { Text: "", Images: [{ Url: null }, { Url: null }, { Url: null }] }], "placeholders");
    }
}
