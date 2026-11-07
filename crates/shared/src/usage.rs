//! Port of `Core/Usage.cs` (`get_usage` control response; quota meter) — spec 02 §11. Owner: shared-pure-a.
//! Shape is experimental: every part optional.

use crate::{fmt, i18n, json};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `behaviors.week` group kinds, in display order.
pub const KINDS: [&str; 5] = ["behaviors", "agents", "skills", "plugins", "mcp_servers"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageRow {
    pub name: String,
    /// 0..1.
    pub pct: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageGroup {
    pub kind: String,
    pub rows: Vec<UsageRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub models: Vec<UsageRow>,
    pub extra_on: Option<bool>,
    pub spent: Option<Decimal>,
    pub spend_limit: Option<Decimal>,
    pub currency: Option<String>,
    pub surfaces: Vec<UsageRow>,
    pub week: Vec<UsageGroup>,
}

/// Rows of an array (elements without a `name` string skipped); `pct` 0..100 in the answer, 0..1 here.
fn rows(a: Option<&Value>, name: &str, pct: &str) -> Vec<UsageRow> {
    match a {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|x| Some(UsageRow { name: json::str(x, name)?.to_string(), pct: json::double(x, pct) / 100.0 }))
            .collect(),
        _ => Vec::new(),
    }
}

/// Money as `amount_minor` × 10^-exponent; None unless amount_minor is a number.
fn amount(m: Option<&Value>) -> Option<Decimal> {
    let m = m?;
    let minor = json::decimal(m, "amount_minor")?;
    let exp = json::long(m, "exponent");
    let pow = Decimal::from(10u64.checked_pow(u32::try_from(exp.unsigned_abs()).ok()?)?);
    if exp >= 0 {
        minor.checked_div(pow)
    } else {
        minor.checked_mul(pow)
    }
}

impl Usage {
    pub fn parse(response: &Value) -> Usage {
        let rl = json::prop(response, "rate_limits");
        let at = |k: &str| rl.and_then(|r| json::prop(r, k));
        let spend = at("spend");
        let used = spend.and_then(|s| json::prop(s, "used"));
        let week = json::prop(response, "behaviors").and_then(|b| json::prop(b, "week"));
        Usage {
            models: rows(at("model_scoped"), "display_name", "utilization"),
            extra_on: at("extra_usage").and_then(|e| json::prop(e, "is_enabled")).and_then(Value::as_bool),
            // Credits off and none spent: nothing to show (the plan has them disabled).
            spent: amount(used).filter(|s| *s > Decimal::ZERO || spend.is_some_and(|s| json::bool(s, "enabled"))),
            spend_limit: amount(spend.and_then(|s| json::prop(s, "limit"))),
            currency: used.and_then(|u| json::str(u, "currency")).map(str::to_string),
            surfaces: rows(at("seven_day_breakdown").and_then(|b| json::prop(b, "rows")), "display_name", "percent"),
            week: week.map_or_else(Vec::new, |w| {
                KINDS
                    .iter()
                    .map(|k| UsageGroup {
                        kind: k.to_string(),
                        rows: rows(json::prop(w, k), if *k == "behaviors" { "key" } else { "name" }, "pct"),
                    })
                    .filter(|g| !g.rows.is_empty())
                    .collect()
            }),
        }
    }

    /// USD (or no currency) → `Fmt.Cost(v, 2)`; else `"{v:F2} {currency}"` with the UI decimal separator.
    pub fn money(&self, v: Decimal) -> String {
        match self.currency.as_deref() {
            None | Some("USD") => fmt::cost(v, 2),
            Some(c) => {
                let s = format!("{:.2}", v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero));
                format!("{} {c}", s.replace('.', &i18n::lang().decimal_sep().to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set_thread_lang, Lang};
    use serde_json::json;

    #[test]
    fn check() {
        // claude 2.1.296 on a Max plan (trimmed from the probe).
        let u = Usage::parse(&json!({"session":{"total_cost_usd":0},"subscription_type":"max","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":5,"resets_at":"2026-10-11T05:19:59.900017+00:00"},"seven_day":{"utilization":91,"resets_at":"2026-10-12T17:59:59.900036+00:00"},"extra_usage":{"is_enabled":false,"monthly_limit":null,"used_credits":null,"user_disabled":true},"limits":[{"kind":"weekly_scoped","group":"weekly","percent":0,"scope":{"model":{"id":null,"display_name":"Fable"}}}],"spend":{"used":{"amount_minor":1250,"currency":"USD","exponent":2},"limit":{"amount_minor":5000,"currency":"USD","exponent":2},"percent":25,"enabled":true},"seven_day_breakdown":{"rows":[{"key":"claude_code","display_name":"Claude Code","percent":97},{"key":"cowork","display_name":"Cowork","percent":3}]},"model_scoped":[{"display_name":"Fable","utilization":40,"resets_at":"2026-10-12T18:00:00+00:00"}]},"behaviors":{"week":{"request_count":13279,"session_count":448,"behaviors":[{"key":"cache_miss","pct":12,"count":3}],"agents":[{"name":"general-purpose","pct":12}],"skills":[],"plugins":[{"name":"impeccable","pct":13}],"mcp_servers":[]}}}));
        assert_eq!(u.models, vec![UsageRow { name: "Fable".into(), pct: 0.4 }]);
        assert_eq!(u.extra_on, Some(false));
        assert_eq!(u.spent, Some(Decimal::new(125, 1)));
        assert_eq!(u.spend_limit, Some(Decimal::from(50)));
        assert_eq!(u.currency.as_deref(), Some("USD"));
        assert_eq!(u.surfaces.len(), 2);
        assert_eq!(u.surfaces[0], UsageRow { name: "Claude Code".into(), pct: 0.97 });
        assert_eq!(u.week.iter().map(|g| g.kind.as_str()).collect::<Vec<_>>(), vec!["behaviors", "agents", "plugins"]);
        assert_eq!(u.week[0].rows, vec![UsageRow { name: "cache_miss".into(), pct: 0.12 }]);

        let off = Usage::parse(&json!({"rate_limits":{"spend":{"used":{"amount_minor":0,"currency":"USD","exponent":2},"limit":null,"enabled":false}}}));
        assert_eq!(off.spent, None, "credits off, none spent: hidden");
        // API key sessions: rate_limits and behaviors are null.
        assert_eq!(Usage::parse(&json!({"rate_limits_available":false,"rate_limits":null,"behaviors":null})), Usage::default());
    }

    #[test]
    fn money_other_currency() {
        // USD goes through fmt::cost (another unit's port); only the non-USD branch is checked here.
        let u = Usage { currency: Some("EUR".into()), ..Usage::default() };
        set_thread_lang(Some(Lang::Fr));
        assert_eq!(u.money(Decimal::new(12345, 3)), "12,35 EUR");
        set_thread_lang(Some(Lang::En));
        assert_eq!(u.money(Decimal::from(3)), "3.00 EUR");
        set_thread_lang(None);
    }
}
