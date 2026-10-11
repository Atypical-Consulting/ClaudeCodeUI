using System.Globalization;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// An MCP server asking the user for input mid-call (MCP elicitation). The CLI forwards it to us as control_request
// {subtype:"elicitation", mcp_server_name, message, mode:form|url, requested_schema | url + elicitation_id, title} and waits
// for {action:"accept"|"decline"|"cancel", content?}. It is queued as a PendingPermission (Tool, request as Input), so the
// card, the queue and "deny and send" treat it like any request. A URL-mode one is answered "accept" once the user is done
// in the browser, or on its own when the server sends system/elicitation_complete. Verified end to end against a fake
// stdio MCP server on claude 2.1.296 (form content and URL accept both reach the server; the complete frame arrives).
public static class Elicit
{
    public const string Tool = "dialog:mcp_elicitation";   // the tool_name the CLI itself gives this pending action

    // Kind: string | number | integer | boolean | enum | multi. InputType: the <input type> of a string (email, url, date…).
    public record Field(string Key, string Label, string? Description, string Kind, bool Required,
                        (string Value, string Label)[] Options, string? Default, double? Min, double? Max, string InputType = "text");

    public static bool IsUrl(JsonElement request) => Events.Str(request, "mode") == "url";

    // Only an http(s) url ends up in an href.
    public static string? Url(JsonElement request) =>
        Events.Str(request, "url") is { } u && Uri.TryCreate(u, UriKind.Absolute, out var uri) && uri.Scheme is "http" or "https" ? u : null;

    public static bool Matches(PendingPermission p, string server, string id) =>
        p.Tool == Tool && Events.Str(p.Input, "mcp_server_name") == server && Events.Str(p.Input, "elicitation_id") == id;

    // requested_schema: a flat object of primitive properties (MCP restricts elicitation schemas to these).
    public static Field[] Parse(JsonElement request)
    {
        if (Events.Prop(request, "requested_schema") is not { } schema || Events.Prop(schema, "properties") is not { ValueKind: JsonValueKind.Object } props) return [];
        var required = Events.Prop(schema, "required") is { ValueKind: JsonValueKind.Array } r ? r.EnumerateArray().Select(x => x.ToString()).ToHashSet() : [];
        return [.. props.EnumerateObject().Select(p =>
        {
            var s = p.Value;
            var type = Events.Str(s, "type");
            var items = Events.Prop(s, "items") ?? default;
            var options = Options(type == "array" ? items : s);
            var kind = type switch
            {
                "array" => "multi",
                "boolean" or "number" or "integer" => type,
                _ => options.Length > 0 ? "enum" : "string",
            };
            var def = Events.Prop(s, "default") is { } d ? d.ValueKind == JsonValueKind.String ? d.GetString() : d.GetRawText() : null;
            return new Field(p.Name, Events.Str(s, "title") ?? p.Name, Events.Str(s, "description"), kind, required.Contains(p.Name), options, def,
                Num(s, "minimum"), Num(s, "maximum"),
                Events.Str(s, "format") switch { "email" => "email", "uri" => "url", "date" => "date", "date-time" => "datetime-local", _ => "text" });
        })];

        // enum (+ enumNames), or titled oneOf / anyOf [{const, title}].
        static (string, string)[] Options(JsonElement s)
        {
            if (Events.Prop(s, "enum") is { ValueKind: JsonValueKind.Array } e)
            {
                var names = Events.Prop(s, "enumNames") is { ValueKind: JsonValueKind.Array } n ? n.EnumerateArray().Select(x => x.ToString()).ToArray() : [];
                return [.. e.EnumerateArray().Select((v, i) => (v.ToString(), i < names.Length ? names[i] : v.ToString()))];
            }
            return (Events.Prop(s, "oneOf") ?? Events.Prop(s, "anyOf")) is { ValueKind: JsonValueKind.Array } o
                ? [.. o.EnumerateArray().Where(x => Events.Prop(x, "const") is not null).Select(x => (x.GetProperty("const").ToString(), Events.Str(x, "title") ?? x.GetProperty("const").ToString()))]
                : [];
        }
        static double? Num(JsonElement s, string n) => Events.Prop(s, n) is { ValueKind: JsonValueKind.Number } v ? v.GetDouble() : null;
    }

    // The accept content from what the form holds (text by key; picks for multi), or the label of the first field that
    // is missing or not a valid number. Blank optional fields are left out. A date-time goes out as RFC 3339, local offset.
    public static (JsonObject? Content, string? Invalid) Content(Field[] fields, IReadOnlyDictionary<string, string> text, IReadOnlyDictionary<string, HashSet<string>> picks)
    {
        var o = new JsonObject();
        foreach (var f in fields)
        {
            var t = text.GetValueOrDefault(f.Key)?.Trim() ?? "";
            JsonNode? v;
            switch (f.Kind)
            {
                case "boolean": v = t == "true"; break;
                case "multi":
                    var set = picks.GetValueOrDefault(f.Key) ?? [];
                    v = set.Count > 0 ? new JsonArray([.. f.Options.Where(x => set.Contains(x.Value)).Select(x => (JsonNode?)x.Value)]) : null;
                    break;
                case "number" or "integer" when t.Length > 0:
                    if (!double.TryParse(t, NumberStyles.Float, CultureInfo.InvariantCulture, out var d) || f.Kind == "integer" && d != Math.Floor(d)
                        || d < f.Min || d > f.Max) return (null, f.Label);
                    v = f.Kind == "integer" ? JsonValue.Create((long)d) : JsonValue.Create(d);
                    break;
                case "string" when f.InputType == "datetime-local" && t.Length > 0:
                    if (!DateTime.TryParse(t, CultureInfo.InvariantCulture, out var dt)) return (null, f.Label);
                    v = new DateTimeOffset(dt).ToString("yyyy-MM-ddTHH:mm:sszzz", CultureInfo.InvariantCulture);
                    break;
                default: v = t.Length > 0 ? t : null; break;
            }
            if (v is null) { if (f.Required) return (null, f.Label); continue; }
            o[f.Key] = v;
        }
        return (o, null);
    }

    public static JsonObject Reply(string action, JsonElement? content) =>
        content is { } c ? new() { ["action"] = action, ["content"] = JsonNode.Parse(c.GetRawText()) } : new() { ["action"] = action };

    // The probe's requests (claude 2.1.296, fake stdio server), trimmed.
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Elicit: " + what);
        var form = JsonDocument.Parse("""{"subtype":"elicitation","mcp_server_name":"probe","message":"What is your name?","mode":"form","requested_schema":{"type":"object","properties":{"name":{"type":"string","title":"Name","minLength":1},"age":{"type":"integer","minimum":0},"color":{"type":"string","enum":["red","blue"],"enumNames":["Red","Blue"]},"ok":{"type":"boolean","default":true},"tags":{"type":"array","items":{"anyOf":[{"const":"a","title":"A"},{"const":"b","title":"B"}]}},"when":{"type":"string","format":"date-time"}},"required":["name"]}}""").RootElement.Clone();
        var fs = Parse(form);
        Ok(fs is [{ Key: "name", Label: "Name", Kind: "string", Required: true }, { Kind: "integer", Min: 0, Required: false }, { Kind: "enum", Options: [("red", "Red"), ("blue", "Blue")] },
                  { Kind: "boolean", Default: "true" }, { Kind: "multi", Options: [("a", "A"), ("b", "B")] }, { InputType: "datetime-local" }], "parse schema");
        var (c, bad) = Content(fs, new Dictionary<string, string> { ["name"] = " Ada ", ["age"] = "36", ["color"] = "blue", ["ok"] = "true" },
                               new Dictionary<string, HashSet<string>> { ["tags"] = ["b", "a"] });
        Ok(bad is null && c!.ToJsonString() == """{"name":"Ada","age":36,"color":"blue","ok":true,"tags":["a","b"]}""", "content typed, blanks left out");
        Ok(Content(fs, new Dictionary<string, string> { ["age"] = "3" }, new Dictionary<string, HashSet<string>>()).Invalid == "Name", "required missing");
        Ok(Content(fs, new Dictionary<string, string> { ["name"] = "x", ["age"] = "-1" }, new Dictionary<string, HashSet<string>>()).Invalid == "age"
           && Content(fs, new Dictionary<string, string> { ["name"] = "x", ["age"] = "1.5" }, new Dictionary<string, HashSet<string>>()).Invalid == "age", "number out of range or not integer");
        Ok(Content(fs, new Dictionary<string, string> { ["name"] = "x", ["when"] = "2026-10-11T10:30" }, new Dictionary<string, HashSet<string>>()).Content!["when"]!.GetValue<string>().StartsWith("2026-10-11T10:30:00"), "date-time as RFC 3339");
        var url = JsonDocument.Parse("""{"subtype":"elicitation","mcp_server_name":"probe","message":"Open the page to link your account","mode":"url","url":"https://example.com/link?x=1","elicitation_id":"el-42"}""").RootElement.Clone();
        Ok(IsUrl(url) && !IsUrl(form) && Url(url) == "https://example.com/link?x=1" && Parse(url).Length == 0, "url mode");
        Ok(Url(JsonDocument.Parse("""{"url":"javascript:alert(1)"}""").RootElement) is null, "non-http url dropped");
        var p = new PendingPermission("r", Tool, url, null, null, null);
        Ok(Matches(p, "probe", "el-42") && !Matches(p, "probe", "el-1") && !Matches(p with { Tool = "Bash" }, "probe", "el-42"), "complete matches its request");
        Ok(Reply("decline", null).ToJsonString() == """{"action":"decline"}""" && Reply("accept", JsonDocument.Parse("""{"name":"Ada"}""").RootElement).ToJsonString() == """{"action":"accept","content":{"name":"Ada"}}""", "reply");
    }
}
