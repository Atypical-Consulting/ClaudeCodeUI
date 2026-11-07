//! Port of `Core/Usage.cs` (`get_usage` control response; quota meter) — spec 02 §11. Owner: shared-pure-a.
//! Shape is experimental: every part optional.

use rust_decimal::Decimal;
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

impl Usage {
    pub fn parse(response: &Value) -> Usage {
        todo!("port: Usage.Parse")
    }

    /// USD (or no currency) → `Fmt.Cost(v, 2)`; else `"{v:F2} {currency}"` with the UI decimal separator.
    pub fn money(&self, v: Decimal) -> String {
        todo!("port: Usage.Money")
    }
}
