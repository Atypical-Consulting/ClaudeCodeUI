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
        self.kind == "background"
    }

    /// state ∈ {working, busy, running}.
    pub fn working(&self) -> bool {
        matches!(self.state.as_str(), "working" | "busy" | "running")
    }

    /// name, else (background ? id : basename(cwd)) — never a PID.
    pub fn label(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if self.background() {
            self.id.clone()
        } else {
            let cwd = self.cwd.trim_end_matches(['/', '\\']);
            cwd.rsplit(['/', '\\']).next().unwrap_or_default().to_string()
        }
    }

    /// state ∈ {blocked, waiting} && !stale.
    pub fn needs_you(&self) -> bool {
        self.blocked() && !self.stale
    }

    fn blocked(&self) -> bool {
        matches!(self.state.as_str(), "blocked" | "waiting")
    }
}

/// `claude agents --json --all` output (not an array → []).
pub fn parse(json: &Value) -> Vec<Agent> {
    use crate::json::{long, str};
    let Some(rows) = json.as_array() else { return Vec::new() };
    rows.iter()
        .map(|a| Agent {
            id: str(a, "id").map_or_else(|| long(a, "pid").to_string(), str::to_string),
            kind: str(a, "kind").unwrap_or_default().to_string(),
            session_id: str(a, "sessionId").map(str::to_string),
            name: str(a, "name").unwrap_or_default().to_string(),
            cwd: str(a, "cwd").unwrap_or_default().to_string(),
            state: str(a, "state").or_else(|| str(a, "status")).unwrap_or_default().to_string(),
            started_at: Some(long(a, "startedAt"))
                .filter(|ms| (1..253_402_300_800_000).contains(ms))
                .and_then(chrono::DateTime::from_timestamp_millis),
            waiting_for: str(a, "waitingFor").map(str::to_string),
            ..Agent::default()
        })
        .collect()
}

/// Fold `jobs/<id>/state.json` and the last timeline line into a background row.
pub fn job(agent: Agent, state: Option<&Value>, last: Option<&Value>) -> Agent {
    use crate::json::str;
    let needs = state.and_then(|s| str(s, "needs")).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    let flags: Vec<&str> = state
        .and_then(|s| s.get("respawnFlags"))
        .and_then(Value::as_array)
        .map(|f| f.iter().map(|x| x.as_str().unwrap_or_default()).collect())
        .unwrap_or_default();
    let mode = flags.iter().position(|f| *f == "--permission-mode").and_then(|i| flags.get(i + 1)).map(|m| m.to_string());
    let last_at = last.and_then(|l| str(l, "at")).and_then(crate::json::parse_time);
    Agent { needs, mode, last_at, ..agent }
}

/// Stable order: needs_you 0, live working 1, blocked/waiting 2, rest 3.
pub fn order(mut rows: Vec<Agent>) -> Vec<Agent> {
    rows.sort_by_key(|a| {
        if a.needs_you() {
            0
        } else if !a.stale && a.working() {
            1
        } else if a.blocked() {
            2
        } else {
            3
        }
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Vec<Agent> {
        let v: Value = serde_json::from_str(r#"[{"id":"632fca02","cwd":"/r/.claude/worktrees/x","kind":"background","startedAt":1785952770170,"sessionId":"632fca02-153e-4f54-a711-8199f18e140c","name":"github issues planning","state":"blocked"},{"pid":69764,"cwd":"/r","kind":"interactive","startedAt":1791561876803,"sessionId":"3b30179d-689e-475a-813c-34f01c89eeb4","name":"ui","status":"waiting","waitingFor":"input needed"}]"#).unwrap();
        parse(&v)
    }

    #[test]
    fn parse_agents() {
        let list = sample();
        assert_eq!(list.len(), 2);
        assert!(list[0].id == "632fca02" && list[0].background() && list[0].state == "blocked" && list[0].name == "github issues planning");
        assert!(list[1].id == "69764" && !list[1].background() && list[1].state == "waiting");
        assert_eq!(list[1].waiting_for.as_deref(), Some("input needed"));
        assert_eq!(list[0].started_at, chrono::DateTime::from_timestamp_millis(1785952770170));
        assert!(parse(&json!({})).is_empty());
    }

    #[test]
    fn job_record() {
        let list = sample();
        let state = json!({"state":"blocked","needs":"delete the verify/* branches too?","tempo":"blocked","respawnFlags":["--reply-on-resume","--permission-mode","bypassPermissions","--model","opus"]});
        let j = job(list[0].clone(), Some(&state), Some(&json!({"at":"2026-08-12T16:38:48.935Z","state":"blocked"})));
        assert_eq!(j.needs.as_deref(), Some("delete the verify/* branches too?"));
        assert_eq!(j.mode.as_deref(), Some("bypassPermissions"));
        assert_eq!(j.last_at, crate::json::parse_time("2026-08-12T16:38:48.935Z"));
        let j = job(list[0].clone(), Some(&json!({"needs":null})), None);
        assert!(j.needs.is_none() && j.last_at.is_none() && j.mode.is_none());
    }

    #[test]
    fn ordering_and_labels() {
        let list = sample();
        let idle = Agent { state: "idle".into(), id: "a".into(), ..list[0].clone() };
        let working = Agent { state: "working".into(), id: "b".into(), ..list[0].clone() };
        let ids = |v: Vec<Agent>| v.into_iter().map(|a| a.id).collect::<Vec<_>>();
        assert_eq!(ids(order(vec![idle.clone(), working, list[1].clone()])), ["69764", "b", "a"]);
        let stale = Agent { stale: true, ..list[0].clone() };
        assert!(!stale.needs_you());
        assert_eq!(ids(order(vec![idle, stale, list[1].clone()])), ["69764", "632fca02", "a"]);
        assert_eq!(Agent { name: String::new(), cwd: "/r/app".into(), ..list[1].clone() }.label(), "app");
        assert_eq!(Agent { name: String::new(), ..list[0].clone() }.label(), "632fca02");
    }
}
