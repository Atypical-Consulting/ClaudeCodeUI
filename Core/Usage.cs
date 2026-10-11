using System.Text.Json;

namespace ClaudeCodeUI;

// get_usage past the 5 h / 7 d windows (claude 2.1.296, "experimental, the shape may change": every part is optional).
// Models: per-model weekly windows (rate_limits.model_scoped). ExtraOn: extra_usage.is_enabled. Spent / SpendLimit: usage
// credits (rate_limits.spend, money as amount_minor + exponent). Surfaces: the week by product (seven_day_breakdown).
// Week: what the week's usage went to, /usage's breakdown: only in an answer asked without skip_behaviors, a scan of every
// local transcript of the week, so Boot skips it and the quota meter asks for it on demand.
public sealed record Usage(IReadOnlyList<UsageRow> Models, bool? ExtraOn, decimal? Spent, decimal? SpendLimit, string? Currency,
                           IReadOnlyList<UsageRow> Surfaces, IReadOnlyList<UsageGroup> Week)
{
    public static readonly string[] Kinds = ["behaviors", "agents", "skills", "plugins", "mcp_servers"];

    public static Usage Parse(JsonElement r)
    {
        var rl = Events.Prop(r, "rate_limits");
        var spend = rl is { } a ? Events.Prop(a, "spend") : null;
        var week = Events.Prop(r, "behaviors") is { } b ? Events.Prop(b, "week") : null;
        return new(Rows(rl is { } m ? Events.Prop(m, "model_scoped") : null, "display_name", "utilization"),
            rl is { } x && Events.Prop(x, "extra_usage") is { } ex && Events.Prop(ex, "is_enabled") is { ValueKind: JsonValueKind.True or JsonValueKind.False } on ? on.GetBoolean() : null,
            // Credits off and none spent: nothing to show (the plan has them disabled).
            spend is { } s1 && Amount(Events.Prop(s1, "used")) is { } spent && (spent > 0 || Events.Bool(s1, "enabled")) ? spent : null, spend is { } s2 ? Amount(Events.Prop(s2, "limit")) : null,
            spend is { } s3 && Events.Prop(s3, "used") is { } u ? Events.Str(u, "currency") : null,
            Rows(rl is { } y && Events.Prop(y, "seven_day_breakdown") is { } bd ? Events.Prop(bd, "rows") : null, "display_name", "percent"),
            week is { } w ? [.. Kinds.Select(k => new UsageGroup(k, Rows(Events.Prop(w, k), k == "behaviors" ? "key" : "name", "pct"))).Where(g => g.Rows.Count > 0)] : []);
    }

    // 0..100 in the answer, 0..1 here like the windows.
    static List<UsageRow> Rows(JsonElement? a, string name, string pct) => a is { ValueKind: JsonValueKind.Array } arr
        ? [.. arr.EnumerateArray().Where(x => Events.Str(x, name) is not null).Select(x => new UsageRow(Events.Str(x, name)!,
            Events.Prop(x, pct) is { ValueKind: JsonValueKind.Number } n ? n.GetDouble() / 100 : 0))]
        : [];

    static decimal? Amount(JsonElement? m) => m is { } x && Events.Prop(x, "amount_minor") is { ValueKind: JsonValueKind.Number } n
        ? n.GetDecimal() / (decimal)Math.Pow(10, Events.Long(x, "exponent")) : null;

    public string Money(decimal v) => Currency is null or "USD" ? Fmt.Cost(v, 2) : $"{v:F2} {Currency}";

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Usage: " + what);
        // claude 2.1.296 on a Max plan (trimmed from the probe).
        var u = Parse(JsonDocument.Parse("""{"session":{"total_cost_usd":0},"subscription_type":"max","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":5,"resets_at":"2026-10-11T05:19:59.900017+00:00"},"seven_day":{"utilization":91,"resets_at":"2026-10-12T17:59:59.900036+00:00"},"extra_usage":{"is_enabled":false,"monthly_limit":null,"used_credits":null,"user_disabled":true},"limits":[{"kind":"weekly_scoped","group":"weekly","percent":0,"scope":{"model":{"id":null,"display_name":"Fable"}}}],"spend":{"used":{"amount_minor":1250,"currency":"USD","exponent":2},"limit":{"amount_minor":5000,"currency":"USD","exponent":2},"percent":25,"enabled":true},"seven_day_breakdown":{"rows":[{"key":"claude_code","display_name":"Claude Code","percent":97},{"key":"cowork","display_name":"Cowork","percent":3}]},"model_scoped":[{"display_name":"Fable","utilization":40,"resets_at":"2026-10-12T18:00:00+00:00"}]},"behaviors":{"week":{"request_count":13279,"session_count":448,"behaviors":[{"key":"cache_miss","pct":12,"count":3}],"agents":[{"name":"general-purpose","pct":12}],"skills":[],"plugins":[{"name":"impeccable","pct":13}],"mcp_servers":[]}}}""").RootElement);
        Ok(u.Models is [{ Name: "Fable", Pct: 0.4 }] && u.ExtraOn == false && u.Spent == 12.5m && u.SpendLimit == 50m && u.Money(u.Spent!.Value) == "$12.50", "windows, extra usage, credits");
        Ok(u.Surfaces is [{ Name: "Claude Code", Pct: 0.97 }, _] && u.Week is [{ Kind: "behaviors", Rows: [{ Name: "cache_miss", Pct: 0.12 }] }, { Kind: "agents" }, { Kind: "plugins" }], "breakdown");
        Ok(Parse(JsonDocument.Parse("""{"rate_limits":{"spend":{"used":{"amount_minor":0,"currency":"USD","exponent":2},"limit":null,"enabled":false}}}""").RootElement).Spent is null, "credits off, none spent: hidden");
        // API key sessions: rate_limits and behaviors are null.
        Ok(Parse(JsonDocument.Parse("""{"rate_limits_available":false,"rate_limits":null,"behaviors":null}""").RootElement) is { Models: [], ExtraOn: null, Spent: null, Week: [] }, "no plan");
    }
}

public record UsageRow(string Name, double Pct);
public record UsageGroup(string Kind, IReadOnlyList<UsageRow> Rows);
