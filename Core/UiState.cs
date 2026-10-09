namespace ClaudeCodeUI;

// Per-circuit UI state. Setters raise Changed; MainLayout relays the keys it does not handle through Key.
public sealed class UiState
{
    bool inspectorOpen = true, paletteOpen;
    string? selectedToolId;

    public bool InspectorOpen { get => inspectorOpen; set { inspectorOpen = value; Changed?.Invoke(); } }
    public bool PaletteOpen { get => paletteOpen; set { paletteOpen = value; Changed?.Invoke(); } }
    public string? SelectedToolId { get => selectedToolId; set { selectedToolId = value; Changed?.Invoke(); } }   // a ToolItem or an Agent row

    public event Action? Changed;
    public event Action<string>? Key;   // "Escape", "Enter", "Shift+Enter", "Delete"

    public void RaiseKey(string key) => Key?.Invoke(key);
}
