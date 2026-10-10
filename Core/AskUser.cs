using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// The AskUserQuestion tool. It reaches us as a can_use_tool like any tool; answering it means allowing it with
// updatedInput = its input + "answers": {question text: answer}. A multi-select answer is the labels joined by ", ",
// a free-text answer is sent as is. Verified by --probe-cli ask-user-question (the model's next turn quotes it); an
// allow without "answers" makes the tool return "The user did not answer the questions."
public static class AskUser
{
    public const string Tool = "AskUserQuestion";

    public record Option(string Label, string Description);
    public record Question(string Text, string Header, Option[] Options, bool Multi);

    public static Question[] Parse(JsonElement input) =>
        Events.Prop(input, "questions") is { ValueKind: JsonValueKind.Array } qs
            ? [.. qs.EnumerateArray().Select(q => new Question(Events.Str(q, "question") ?? "", Events.Str(q, "header") ?? "",
                Events.Prop(q, "options") is { ValueKind: JsonValueKind.Array } os
                    ? [.. os.EnumerateArray().Select(o => new Option(Events.Str(o, "label") ?? "", Events.Str(o, "description") ?? ""))] : [],
                Events.Prop(q, "multiSelect") is { ValueKind: JsonValueKind.True }))]
            : [];

    // picked = option indexes (any order); other = the free-text answer, used when non-blank. "" = unanswered.
    public static string Answer(Question q, IEnumerable<int> picked, string? other)
    {
        var parts = picked.Where(i => i >= 0 && i < q.Options.Length).Distinct().Order().Select(i => q.Options[i].Label).ToList();
        if (!string.IsNullOrWhiteSpace(other)) parts.Add(other.Trim());
        return string.Join(", ", q.Multi ? parts : parts.Take(1));
    }

    public static JsonElement WithAnswers(JsonElement input, IReadOnlyDictionary<string, string> answers)
    {
        var o = JsonNode.Parse(input.GetRawText())!.AsObject();
        o["answers"] = new JsonObject(answers.Select(a => KeyValuePair.Create(a.Key, (JsonNode?)a.Value)));
        return JsonDocument.Parse(o.ToJsonString()).RootElement.Clone();
    }

    // (question, answer) pairs of a finished call, from its tool_use_result (stream) / toolUseResult (transcript).
    public static (string Question, string Answer)[] Answered(JsonElement? structured) =>
        structured is { } s && Events.Prop(s, "answers") is { ValueKind: JsonValueKind.Object } a
            ? [.. a.EnumerateObject().Where(p => p.Value.ValueKind == JsonValueKind.String).Select(p => (p.Name, p.Value.GetString()!))]
            : [];

    // Real can_use_tool input and tool_use_result from the probe captures (claude 2.1.296).
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "AskUser: " + what);
        var input = JsonDocument.Parse("""{"questions":[{"question":"Which toppings do you want?","header":"Toppings","options":[{"label":"Cheese","description":"Cheese topping"},{"label":"Ham","description":"Ham topping"},{"label":"Olives","description":"Olives topping"}],"multiSelect":true},{"question":"Which color do you prefer?","header":"Color","options":[{"label":"Red","description":"Choose red"},{"label":"Blue","description":"Choose blue"}],"multiSelect":false}]}""").RootElement.Clone();
        var qs = Parse(input);
        Ok(qs is [{ Header: "Toppings", Multi: true, Options: [{ Label: "Cheese", Description: "Cheese topping" }, _, _] }, { Text: "Which color do you prefer?", Multi: false, Options.Length: 2 }], "parse");
        Ok(Parse(JsonDocument.Parse("{}").RootElement).Length == 0, "parse without questions");

        Ok(Answer(qs[0], [2, 0], null) == "Cheese, Olives", "multi joins labels in option order");
        Ok(Answer(qs[0], [1], " Pineapple ") == "Ham, Pineapple", "multi + free text");
        Ok(Answer(qs[1], [1], null) == "Blue" && Answer(qs[1], [], "Chartreuse") == "Chartreuse", "single");
        Ok(Answer(qs[1], [], "  ") == "" && Answer(qs[0], [7], null) == "", "unanswered");

        var upd = WithAnswers(input, new Dictionary<string, string> { [qs[1].Text] = "Blue" });
        Ok(Events.Prop(upd, "questions") is { ValueKind: JsonValueKind.Array } q && q.GetArrayLength() == 2
           && upd.GetProperty("answers").GetProperty("Which color do you prefer?").GetString() == "Blue", "updatedInput = input + answers");

        var result = JsonDocument.Parse("""{"questions":[],"answers":{"Which color do you prefer?":"Chartreuse"}}""").RootElement.Clone();
        Ok(Answered(result) is [("Which color do you prefer?", "Chartreuse")], "answered");
        Ok(Answered(null).Length == 0 && Answered(JsonDocument.Parse("""{"answers":{}}""").RootElement).Length == 0, "not answered");
    }
}
