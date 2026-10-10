using System.Text.Json;

namespace ClaudeCodeUI;

public enum TodoStatus { Pending, InProgress, Completed }
public sealed record TodoEntry(string Key, string Content, TodoStatus Status, string? ActiveForm)
{
    public string Label => Status == TodoStatus.InProgress ? ActiveForm ?? Content : Content;   // the terminal's spinner text
}

// The agent's task list, rebuilt from its own top-level tool calls, so a live session and a transcript replay agree.
// Two tool families, verified on CLI 2.1.296 (--probe-cli todo-tools and raw captures):
//   TodoWrite {todos:[{content,status,activeForm}]} sends the whole list every time;
//   TaskCreate {subject,description,activeForm?} answers tool_use_result {task:{id}}, TaskUpdate {taskId,status?,subject?,activeForm?}
//   edits that id (status "deleted" removes it). TaskList / TaskGet only read.
// A call that errored or was denied changed nothing; a running one is shown as sent.
public static class TodoList
{
    // Newer models only get these tools when an --allowedTools rule names one of them (todoToolsOptIn); without
    // CLAUDE_CODE_ENABLE_TASKS=false the CLI exposes the Task* family, with it TodoWrite. Passed by LiveSession.Args.
    public const string AllowedTools = "TodoWrite,TaskCreate,TaskGet,TaskUpdate,TaskList";
    // One token: --allowedTools is variadic and would swallow a positional argument placed after a separate value.
    public const string AllowedToolsArg = "--allowedTools=" + AllowedTools;

    public static List<TodoEntry> From(IEnumerable<Item> items)
    {
        var list = new List<TodoEntry>();
        foreach (var i in items)
            if (i is ResetItem) list.Clear();   // /clear: the CLI starts a new, empty list whose ids restart at 1
            else if (i is ToolItem { ParentToolUseId: null, State: not (ToolState.Error or ToolState.Denied) } t) Apply(list, t);
        return list;
    }

    static void Apply(List<TodoEntry> list, ToolItem t)
    {
        var input = t.Input;
        switch (t.Name)
        {
            case "TodoWrite" when Events.Prop(input, "todos") is { ValueKind: JsonValueKind.Array } todos:
                list.Clear();
                list.AddRange(todos.EnumerateArray().Select((x, n) =>
                    new TodoEntry(n.ToString(), Events.Str(x, "content") ?? "", Status(Events.Str(x, "status")) ?? TodoStatus.Pending, Events.Str(x, "activeForm"))));
                break;

            case "TaskCreate":
                // Until the result names the id, the tool_use id stands in: nothing can update it before then.
                var id = Events.Prop(t.Structured ?? default, "task") is { } task ? Events.Str(task, "id") : null;
                // An id the list already holds means the CLI started over without a reset we saw: keys stay unique for @key.
                if (id is not null && list.Exists(x => x.Key == id)) list.Clear();
                list.Add(new(id ?? "#" + t.Id, Events.Str(input, "subject") ?? "", TodoStatus.Pending, Events.Str(input, "activeForm")));
                break;

            case "TaskUpdate" when list.FindIndex(x => x.Key == Events.Str(input, "taskId")) is >= 0 and var at:
                var status = Events.Str(input, "status");
                if (status == "deleted") { list.RemoveAt(at); break; }
                var e = list[at];
                list[at] = e with
                {
                    Content = Events.Str(input, "subject") ?? e.Content,
                    Status = Status(status) ?? e.Status,
                    ActiveForm = Events.Str(input, "activeForm") ?? e.ActiveForm,
                };
                break;
        }
    }

    // Null for anything else: the tools' schemas only accept these three (plus "deleted" for TaskUpdate).
    static TodoStatus? Status(string? s) => s switch
    {
        "pending" => TodoStatus.Pending,
        "in_progress" => TodoStatus.InProgress,
        "completed" => TodoStatus.Completed,
        _ => null,
    };

    public static string Progress(IReadOnlyCollection<TodoEntry> l) => Strings.Get("Todo.Progress", l.Count(e => e.Status == TodoStatus.Completed), l.Count);

    public static string StatusLabel(TodoStatus s) => Strings.Get("Todo." + s);

    // One-line target of a todo call in the tool log: "2/3 done" for a TodoWrite, the subject of a TaskCreate,
    // "#1 → completed" for a TaskUpdate. Null for any other tool, so an MCP tool with look-alike fields keeps its own target.
    internal static string? Target(string name, JsonElement input)
    {
        switch (name)
        {
            case "TodoWrite" when Events.Prop(input, "todos") is { ValueKind: JsonValueKind.Array } todos:
                return Strings.Get("Todo.Progress", todos.EnumerateArray().Count(x => Events.Str(x, "status") == "completed"), todos.GetArrayLength());
            case "TaskUpdate" when Events.Str(input, "taskId") is { } id:
                var s = Events.Str(input, "status");
                var to = s == "deleted" ? Strings.Get("Todo.Deleted") : Status(s) is { } st ? StatusLabel(st) : s;
                return to is null ? "#" + id : $"#{id} → {to}";
            case "TaskCreate":
                return Events.Str(input, "subject");
            default:
                return null;
        }
    }

    // Tool inputs and results trimmed from the 2.1.296 captures.
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "TodoList: " + what);
        static JsonElement J(string json) => JsonDocument.Parse(json).RootElement.Clone();
        static ToolResultEvt Created(string tid, string id) => new(tid, $"Task #{id} created successfully", false, J($$$"""{"task":{"id":"{{{id}}}","subject":"x"}}"""));

        var s = new LiveSession("t", "t", @"C:\w", "default");
        s.Apply(new ToolUseEvt("c1", "TaskCreate", J("""{"subject":"alpha","description":"Task alpha"}"""), null));
        Ok(From(s.Items) is [{ Key: "#c1", Content: "alpha", Status: TodoStatus.Pending }], "create before its result");
        s.Apply(Created("c1", "1"));
        s.Apply(new ToolUseEvt("c2", "TaskCreate", J("""{"subject":"beta","description":"Task beta","activeForm":"Doing beta"}"""), null));
        s.Apply(new ToolUseEvt("c3", "TaskCreate", J("""{"subject":"gamma","description":"Task gamma"}"""), null));
        s.Apply(Created("c2", "2"));
        s.Apply(Created("c3", "3"));
        s.Apply(new ToolUseEvt("u1", "TaskUpdate", J("""{"taskId":"1","status":"completed"}"""), null));
        s.Apply(new ToolUseEvt("u2", "TaskUpdate", J("""{"taskId":"2","status":"in_progress"}"""), null));
        s.Apply(new ToolUseEvt("u3", "TaskUpdate", J("""{"taskId":"9","status":"completed"}"""), null));    // unknown id: ignored
        s.Apply(new ToolUseEvt("l", "TaskList", J("{}"), null));
        var l = From(s.Items);
        Ok(l is [{ Key: "1", Status: TodoStatus.Completed }, { Key: "2", Status: TodoStatus.InProgress, Label: "Doing beta" }, { Key: "3", Status: TodoStatus.Pending, Label: "gamma" }],
            "create + update");
        s.Apply(new ToolUseEvt("u4", "TaskUpdate", J("""{"taskId":"3","status":"deleted"}"""), null));
        s.Apply(new ToolUseEvt("u5", "TaskUpdate", J("""{"taskId":"1","status":"pending"}"""), null));
        s.Apply(new ToolResultEvt("u5", "Task not found", true, null));    // an errored update changed nothing
        s.Apply(new ToolUseEvt("sub", "TaskCreate", J("""{"subject":"sub"}"""), "agent"));   // a subagent's own call
        Ok(From(s.Items) is [{ Key: "1", Status: TodoStatus.Completed }, { Key: "2" }], "deleted, errored and subagent calls");
        Ok(Progress(From(s.Items)) == "1/2 faites", "progress");

        var w = new LiveSession("w", "w", @"C:\w", "default");
        w.Apply(new ToolUseEvt("w1", "TodoWrite", J("""{"todos":[{"content":"alpha","status":"pending","activeForm":"Doing alpha"},{"content":"beta","status":"pending","activeForm":"beta"}]}"""), null));
        w.Apply(new ToolUseEvt("w2", "TodoWrite", J("""{"todos":[{"content":"alpha","status":"completed","activeForm":"Doing alpha"},{"content":"beta","status":"in_progress","activeForm":"Doing beta"},{"content":"gamma","status":"pending","activeForm":"gamma"}]}"""), null));
        Ok(From(w.Items) is [{ Status: TodoStatus.Completed, Label: "alpha" }, { Status: TodoStatus.InProgress, Label: "Doing beta" }, { Content: "gamma" }], "TodoWrite replaces the list");
        w.Apply(new ToolUseEvt("w3", "TodoWrite", J("""{"todos":[]}"""), null));
        w.Apply(new PermissionEvt("r", "TodoWrite", J("{}"), null, "w3", null));
        w.Resolve(w.Pending[0], Decision.Deny);
        Ok(From(w.Items).Count == 3, "denied TodoWrite ignored");

        // /clear (2.1.296: {"type":"conversation_reset","trigger":"clear"}, then a new session_id): ids restart at 1.
        var r = new LiveSession("r", "r", @"C:\w", "default");
        r.Apply(new ToolUseEvt("a", "TaskCreate", J("""{"subject":"alpha","description":"alpha"}"""), null));
        r.Apply(Created("a", "1"));
        r.Apply(new ResetEvt());
        Ok(From(r.Items).Count == 0, "reset empties the list");
        r.Apply(new ToolUseEvt("b", "TaskCreate", J("""{"subject":"beta","description":"beta"}"""), null));
        r.Apply(Created("b", "1"));
        r.Apply(new ToolUseEvt("bu", "TaskUpdate", J("""{"taskId":"1","status":"in_progress"}"""), null));
        Ok(From(r.Items) is [{ Key: "1", Content: "beta", Status: TodoStatus.InProgress }], "create, reset, create the same id, update it");
        r.Apply(new ToolUseEvt("c", "TaskCreate", J("""{"subject":"gamma","description":"gamma"}"""), null));
        r.Apply(Created("c", "1"));    // an id reused with no reset seen: never two entries with one key
        Ok(From(r.Items) is [{ Key: "1", Content: "gamma" }], "reused id without a reset");

        Ok(Target("TodoWrite", J("""{"todos":[{"status":"completed"},{"status":"pending"}]}""")) == "1/2 faites"
           && Target("TaskUpdate", J("""{"taskId":"4","status":"in_progress"}""")) == "#4 → en cours" && Target("TaskUpdate", J("""{"taskId":"4"}""")) == "#4"
           && Target("TaskUpdate", J("""{"taskId":"4","status":"done"}""")) == "#4 → done"
           && Target("TaskCreate", J("""{"subject":"alpha","description":"Task alpha"}""")) == "alpha" && Target("Read", J("""{"file_path":"a"}""")) is null, "targets");
        // Look-alike inputs on any other tool (an MCP tool here) keep their usual target.
        Ok(Target("mcp__x__job", J("""{"taskId":"42","status":"done"}""")) is null && Target("mcp__x__t", J("""{"todos":[]}""")) is null
           && ToolKinds.Target("mcp__x__t", J("""{"subject":"s","description":"d"}"""), "") == "d", "targets of other tools");
    }
}
