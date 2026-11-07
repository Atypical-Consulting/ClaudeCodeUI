//! Port of the record and pure parts of `Core/BackgroundAgents.cs` (`claude --bg` sessions) — spec 03 §5.2-5.3.
//! Owner: core-store-sessions. Running `claude agents …` and the watcher are `ccui_core::background_agents`.

use crate::model::Time;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    /// id ?? pid as text.
    pub id: String,
    /// background | interactive.
    pub kind: String,
    pub session_id: Option<String>,
    pub name: String,
    pub cwd: String,
    /// state ?? status ?? "".
    pub state: String,
    /// startedAt (ms epoch, valid range only).
    pub started_at: Option<Time>,
    pub waiting_for: Option<String>,
    /// From jobs/<id>/state.json.
    pub needs: Option<String>,
    /// The mode `respawn` restarts it in.
    pub mode: Option<String>,
    /// Last timeline line `.at`.
    pub last_at: Option<Time>,
    /// The daemon is down: the row shows its last state.
    pub stale: bool,
}

impl Agent {
    pub fn background(&self) -> bool {
        todo!("port: Agent.Background")
    }

    /// state ∈ {working, busy, running}.
    pub fn working(&self) -> bool {
        todo!("port: Agent.Working")
    }

    /// name, else (background ? id : basename(cwd)) — never a PID.
    pub fn label(&self) -> String {
        todo!("port: Agent.Label")
    }

    /// state ∈ {blocked, waiting} && !stale.
    pub fn needs_you(&self) -> bool {
        todo!("port: Agent.NeedsYou")
    }
}

/// `claude agents --json --all` output (not an array → []).
pub fn parse(json: &Value) -> Vec<Agent> {
    todo!("port: BackgroundAgents.Parse")
}

/// Fold `jobs/<id>/state.json` and the last timeline line into a background row.
pub fn job(agent: Agent, state: Option<&Value>, last: Option<&Value>) -> Agent {
    todo!("port: BackgroundAgents.Job")
}

/// Stable order: needs_you 0, live working 1, blocked/waiting 2, rest 3.
pub fn order(rows: Vec<Agent>) -> Vec<Agent> {
    todo!("port: BackgroundAgents.Order")
}
