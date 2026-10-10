using System.Text.Json;

namespace ClaudeCodeUI;

// The thread's top-level blocks (Components/Session/Thread.razor). Subagent items (ParentToolUseId set) live under their
// Workflow row. Consecutive tools form a Run shown as a Ledger, consecutive Agent calls a Run shown as a Workflow,
// consecutive hooks one Hooks row. A Workflow call is a block of its own (WorkflowCard).
public static class ThreadBlocks
{
    public sealed record Run(List<ToolItem> Tools, bool Agents);
    public sealed record Hooks(List<HookItem> Items);

    public static List<object> Of(IEnumerable<Item> items)
    {
        var blocks = new List<object>();
        foreach (var item in items)
        {
            switch (item)
            {
                case ToolItem { ParentToolUseId: null, Name: WorkflowRuns.Tool } w:
                    blocks.Add(w);
                    break;
                case ToolItem { ParentToolUseId: null } t:
                    var agent = ToolKinds.IsAgent(t.Name);
                    if (OpenRun() is { } r && r.Agents == agent) r.Tools.Add(t);
                    else blocks.Add(new Run([t], agent));
                    break;
                case HookItem h:
                    if (blocks.LastOrDefault() is Hooks hs) hs.Items.Add(h);
                    else blocks.Add(new Hooks([h]));
                    break;
                case UserItem or ApiErrorItem or TextItem { ParentToolUseId: null }:
                    blocks.Add(item);
                    break;
            }
        }
        return blocks;

        // Hooks fire between a tool_use and its result (PreToolUse, PostToolUse, SubagentStart) or between two tool calls
        // (PostToolBatch): a hook row follows the run without closing it, so the next tool still joins the same Ledger or
        // Workflow instead of opening one per tool. Only a prompt, text or an API error ends a run.
        Run? OpenRun() => blocks switch
        {
            [.., Run r] => r,
            [.., Run r, Hooks] => r,
            _ => null,
        };
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "ThreadBlocks: " + what);
        var none = JsonDocument.Parse("{}").RootElement.Clone();
        ToolItem T(string id, string name) => new(id, name, none, null);
        HookItem H(string name, string outcome, string output) => new(new HookEvt(name, outcome, outcome == "error" ? 2 : 0, output));

        // claude 2.1.296 order: tool_use Bash, PreToolUse:Bash printing, tool_result, PostToolBatch, tool_use Bash.
        Ok(Of([T("b1", "Bash"), H("PreToolUse:Bash", "success", "ccui-probe-pre"), H("PostToolBatch", "success", "x"), T("b2", "Bash")])
               is [Run { Agents: false, Tools: [{ Id: "b1" }, { Id: "b2" }] }, Hooks { Items.Count: 2 }], "tool hooks keep one ledger");
        // tool_use Agent, a SubagentStart that failed, tool_use Agent: still one Workflow.
        Ok(Of([T("a1", "Agent"), H("SubagentStart:general-purpose", "error", "boom"), T("a2", "Agent"), T("b", "Bash")])
               is [Run { Agents: true, Tools.Count: 2 }, Hooks, Run { Agents: false }], "agent fan-out keeps one workflow");
        Ok(Of([T("b1", "Bash"), new TextItem("t", null), T("b2", "Bash")]) is [Run, TextItem, Run], "text ends a run");
        Ok(Of([T("b1", "Bash"), T("w", WorkflowRuns.Tool), T("b2", "Bash")]) is [Run, ToolItem { Id: "w" }, Run], "a workflow is its own block");
    }
}
