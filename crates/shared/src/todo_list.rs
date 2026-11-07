//! Port of `Core/TodoList.cs` — spec 02 §10. Owner: shared-pure-a.
//! The agent's task list, rebuilt from its own top-level tool calls (live session and transcript replay agree).
//! UI: a `Memo` over the items signal.
//!
//! Two tool families (CLI 2.1.296): TodoWrite `{todos:[{content,status,activeForm}]}` sends the whole list every time;
//! TaskCreate `{subject,description,activeForm?}` answers `tool_use_result {task:{id}}`, TaskUpdate
//! `{taskId,status?,subject?,activeForm?}` edits that id (status "deleted" removes it). TaskList / TaskGet only read.

use crate::model::{Item, ToolItem, ToolState};
use crate::{i18n, json};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ALLOWED_TOOLS: [&str; 5] = ["TodoWrite", "TaskCreate", "TaskGet", "TaskUpdate", "TaskList"];
/// ONE argv token (the flag is variadic and would swallow a positional argument).
pub const ALLOWED_TOOLS_ARG: &str = "--allowedTools=TodoWrite,TaskCreate,TaskGet,TaskUpdate,TaskList";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TodoEntry {
    /// TodoWrite: index as string; TaskCreate: task id, or "#<tool_use_id>" until the result names it.
    pub key: String,
    pub content: String,
    pub status: TodoStatus,
    pub active_form: Option<String>,
}

impl TodoEntry {
    /// active_form ?? content when InProgress, else content (the terminal's spinner text).
    pub fn label(&self) -> &str {
        match (&self.status, &self.active_form) {
            (TodoStatus::InProgress, Some(a)) => a,
            _ => &self.content,
        }
    }
}

/// Fold the items (ResetItem clears; top-level ToolItems not Error/Denied/NotRun apply).
pub fn from<I: std::borrow::Borrow<Item>>(items: &[I]) -> Vec<TodoEntry> {
    let mut list = Vec::new();
    for i in items {
        match i.borrow() {
            // /clear: the CLI starts a new, empty list whose ids restart at 1.
            Item::Reset => list.clear(),
            Item::Tool(t)
                if t.parent_tool_use_id.is_none()
                    && !matches!(t.state, ToolState::Error | ToolState::Denied | ToolState::NotRun) =>
            {
                apply(&mut list, t)
            }
            _ => {}
        }
    }
    list
}

fn apply(list: &mut Vec<TodoEntry>, t: &ToolItem) {
    let input = &t.input;
    match t.name.as_str() {
        "TodoWrite" => {
            if let Some(Value::Array(todos)) = json::prop(input, "todos") {
                *list = todos
                    .iter()
                    .enumerate()
                    .map(|(n, x)| TodoEntry {
                        key: n.to_string(),
                        content: json::string(x, "content"),
                        status: json::str(x, "status").and_then(status).unwrap_or(TodoStatus::Pending),
                        active_form: json::str(x, "activeForm").map(str::to_string),
                    })
                    .collect();
            }
        }
        "TaskCreate" => {
            // Until the result names the id, the tool_use id stands in: nothing can update it before then.
            let id = t.structured.as_ref().and_then(|s| json::prop(s, "task")).and_then(|task| json::str(task, "id"));
            // An id the list already holds means the CLI started over without a reset we saw: keys stay unique.
            if let Some(id) = id {
                if list.iter().any(|x| x.key == id) {
                    list.clear();
                }
            }
            list.push(TodoEntry {
                key: id.map_or_else(|| format!("#{}", t.id), str::to_string),
                content: json::string(input, "subject"),
                status: TodoStatus::Pending,
                active_form: json::str(input, "activeForm").map(str::to_string),
            });
        }
        "TaskUpdate" => {
            let Some(at) = json::str(input, "taskId").and_then(|id| list.iter().position(|x| x.key == id)) else {
                return;
            };
            let s = json::str(input, "status");
            if s == Some("deleted") {
                list.remove(at);
                return;
            }
            let e = &mut list[at];
            if let Some(c) = json::str(input, "subject") {
                e.content = c.to_string();
            }
            if let Some(st) = s.and_then(status) {
                e.status = st;
            }
            if let Some(a) = json::str(input, "activeForm") {
                e.active_form = Some(a.to_string());
            }
        }
        _ => {}
    }
}

/// "pending" | "in_progress" | "completed", else None.
pub fn status(s: &str) -> Option<TodoStatus> {
    match s {
        "pending" => Some(TodoStatus::Pending),
        "in_progress" => Some(TodoStatus::InProgress),
        "completed" => Some(TodoStatus::Completed),
        _ => None,
    }
}

/// i18n `Todo.Progress` (completed, total): en "{0}/{1} done", fr "{0}/{1} faites".
pub fn progress(list: &[TodoEntry]) -> String {
    let done = list.iter().filter(|e| e.status == TodoStatus::Completed).count();
    i18n::tf("Todo.Progress", &[&done, &list.len()])
}

/// i18n `Todo.Pending|Todo.InProgress|Todo.Completed`.
pub fn status_label(s: TodoStatus) -> String {
    i18n::t(&format!("Todo.{s:?}"))
}

/// Tool-log one-liner for TodoWrite / TaskUpdate / TaskCreate, None for other tools.
pub fn target(name: &str, input: &Value) -> Option<String> {
    match name {
        "TodoWrite" => {
            let Some(Value::Array(todos)) = json::prop(input, "todos") else { return None };
            let done = todos.iter().filter(|x| json::str(x, "status") == Some("completed")).count();
            Some(i18n::tf("Todo.Progress", &[&done, &todos.len()]))
        }
        "TaskUpdate" => {
            let id = json::str(input, "taskId")?;
            let to = match json::str(input, "status") {
                Some("deleted") => Some(i18n::t("Todo.Deleted")),
                Some(s) => Some(status(s).map_or_else(|| s.to_string(), status_label)),
                None => None,
            };
            Some(match to {
                Some(to) => format!("#{id} → {to}"),
                None => format!("#{id}"),
            })
        }
        "TaskCreate" => json::str(input, "subject").map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set_thread_lang, Lang};
    use serde_json::json;

    /// A tool call as the reducer stores it.
    fn call(id: &str, name: &str, input: Value) -> Item {
        Item::Tool(ToolItem {
            id: id.into(),
            name: name.into(),
            input,
            parent_tool_use_id: None,
            state: ToolState::Running,
            result_text: None,
            structured: None,
            started_at: chrono::DateTime::UNIX_EPOCH,
            ended_at: None,
            task_id: None,
            task_status: None,
            progress: None,
            tokens: 0,
            sub_tool_uses: 0,
            background: false,
            model: None,
        })
    }

    fn tool_mut<'a>(items: &'a mut [Item], id: &str) -> &'a mut ToolItem {
        items
            .iter_mut()
            .find_map(|i| match i {
                Item::Tool(t) if t.id == id => Some(t),
                _ => None,
            })
            .expect("tool")
    }

    /// `Task #id created successfully` result.
    fn created(items: &mut [Item], tid: &str, id: &str) {
        let t = tool_mut(items, tid);
        t.state = ToolState::Done;
        t.structured = Some(json!({"task":{"id":id,"subject":"x"}}));
    }

    fn keys(l: &[TodoEntry]) -> Vec<(&str, TodoStatus)> {
        l.iter().map(|e| (e.key.as_str(), e.status)).collect()
    }

    #[test]
    fn task_family() {
        set_thread_lang(Some(Lang::Fr));
        let mut s = vec![call("c1", "TaskCreate", json!({"subject":"alpha","description":"Task alpha"}))];
        assert_eq!(from(&s), vec![TodoEntry { key: "#c1".into(), content: "alpha".into(), status: TodoStatus::Pending, active_form: None }]);
        created(&mut s, "c1", "1");
        s.push(call("c2", "TaskCreate", json!({"subject":"beta","description":"Task beta","activeForm":"Doing beta"})));
        s.push(call("c3", "TaskCreate", json!({"subject":"gamma","description":"Task gamma"})));
        created(&mut s, "c2", "2");
        created(&mut s, "c3", "3");
        s.push(call("u1", "TaskUpdate", json!({"taskId":"1","status":"completed"})));
        s.push(call("u2", "TaskUpdate", json!({"taskId":"2","status":"in_progress"})));
        s.push(call("u3", "TaskUpdate", json!({"taskId":"9","status":"completed"}))); // unknown id: ignored
        s.push(call("l", "TaskList", json!({})));
        let l = from(&s);
        assert_eq!(keys(&l), vec![("1", TodoStatus::Completed), ("2", TodoStatus::InProgress), ("3", TodoStatus::Pending)]);
        assert_eq!((l[1].label(), l[2].label()), ("Doing beta", "gamma"));

        s.push(call("u4", "TaskUpdate", json!({"taskId":"3","status":"deleted"})));
        s.push(call("u5", "TaskUpdate", json!({"taskId":"1","status":"pending"})));
        tool_mut(&mut s, "u5").state = ToolState::Error; // an errored update changed nothing
        let mut sub = call("sub", "TaskCreate", json!({"subject":"sub"}));
        if let Item::Tool(t) = &mut sub {
            t.parent_tool_use_id = Some("agent".into()); // a subagent's own call
        }
        s.push(sub);
        let l = from(&s);
        assert_eq!(keys(&l), vec![("1", TodoStatus::Completed), ("2", TodoStatus::InProgress)]);
        assert_eq!(progress(&l), "1/2 faites");
        set_thread_lang(None);
    }

    #[test]
    fn todo_write() {
        let mut w = vec![
            call("w1", "TodoWrite", json!({"todos":[{"content":"alpha","status":"pending","activeForm":"Doing alpha"},{"content":"beta","status":"pending","activeForm":"beta"}]})),
            call("w2", "TodoWrite", json!({"todos":[{"content":"alpha","status":"completed","activeForm":"Doing alpha"},{"content":"beta","status":"in_progress","activeForm":"Doing beta"},{"content":"gamma","status":"pending","activeForm":"gamma"}]})),
        ];
        let l = from(&w);
        assert_eq!(keys(&l), vec![("0", TodoStatus::Completed), ("1", TodoStatus::InProgress), ("2", TodoStatus::Pending)]);
        assert_eq!((l[0].label(), l[1].label(), l[2].content.as_str()), ("alpha", "Doing beta", "gamma"));
        w.push(call("w3", "TodoWrite", json!({"todos":[]})));
        tool_mut(&mut w, "w3").state = ToolState::Denied;
        assert_eq!(from(&w).len(), 3, "denied TodoWrite ignored");
    }

    #[test]
    fn reset_and_reused_ids() {
        let mut r = vec![call("a", "TaskCreate", json!({"subject":"alpha"}))];
        created(&mut r, "a", "1");
        r.push(Item::Reset);
        assert!(from(&r).is_empty(), "reset empties the list");
        r.push(call("b", "TaskCreate", json!({"subject":"beta"})));
        created(&mut r, "b", "1");
        r.push(call("bu", "TaskUpdate", json!({"taskId":"1","status":"in_progress"})));
        let l = from(&r);
        assert_eq!((keys(&l), l[0].content.as_str()), (vec![("1", TodoStatus::InProgress)], "beta"));
        r.push(call("c", "TaskCreate", json!({"subject":"gamma"})));
        created(&mut r, "c", "1"); // an id reused with no reset seen: never two entries with one key
        let l = from(&r);
        assert_eq!((l.len(), l[0].content.as_str()), (1, "gamma"));
    }

    #[test]
    fn targets() {
        set_thread_lang(Some(Lang::Fr));
        assert_eq!(target("TodoWrite", &json!({"todos":[{"status":"completed"},{"status":"pending"}]})).as_deref(), Some("1/2 faites"));
        assert_eq!(target("TaskUpdate", &json!({"taskId":"4","status":"in_progress"})).as_deref(), Some("#4 → en cours"));
        assert_eq!(target("TaskUpdate", &json!({"taskId":"4"})).as_deref(), Some("#4"));
        assert_eq!(target("TaskUpdate", &json!({"taskId":"4","status":"done"})).as_deref(), Some("#4 → done"));
        assert_eq!(target("TaskUpdate", &json!({"taskId":"4","status":"deleted"})).as_deref(), Some("#4 → supprimée"));
        assert_eq!(target("TaskCreate", &json!({"subject":"alpha","description":"Task alpha"})).as_deref(), Some("alpha"));
        assert_eq!(target("Read", &json!({"file_path":"a"})), None);
        assert_eq!(target("mcp__x__job", &json!({"taskId":"42","status":"done"})), None);
        assert_eq!(target("mcp__x__t", &json!({"todos":[]})), None);
        assert_eq!(status_label(TodoStatus::Completed), "faite");
        set_thread_lang(None);
        assert_eq!(status("deleted"), None);
    }
}
