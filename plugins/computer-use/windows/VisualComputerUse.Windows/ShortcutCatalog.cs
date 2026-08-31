namespace VisualComputerUse.Windows;

internal static class ShortcutCatalog
{
    private static readonly ShortcutDefinition[] Common =
    [
        new("copy", "Copy", ["control", "c"], "Copy the current selection."),
        new("paste", "Paste", ["control", "v"], "Paste clipboard content."),
        new("cut", "Cut", ["control", "x"], "Cut the current selection."),
        new("undo", "Undo", ["control", "z"], "Undo the last action."),
        new("redo", "Redo", ["control", "y"], "Redo the last action."),
        new("find", "Find", ["control", "f"], "Open the application's find interface."),
        new("save", "Save", ["control", "s"], "Save the current document."),
        new("select_all", "Select all", ["control", "a"], "Select all content in the focused control."),
        new("close_window", "Close window", ["alt", "f4"], "Close the active window."),
        new("switch_window", "Switch window", ["alt", "tab"], "Switch to another application window.")
    ];

    internal static ShortcutListDto List(ActiveApplicationDto application, string? query)
    {
        IEnumerable<ShortcutDefinition> result = Common;
        if (!string.IsNullOrWhiteSpace(query))
        {
            result = result.Where(shortcut =>
                shortcut.Id.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                shortcut.Title.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                (shortcut.Description?.Contains(query, StringComparison.OrdinalIgnoreCase) ?? false) ||
                shortcut.Keys.Any(key => key.Contains(query, StringComparison.OrdinalIgnoreCase)));
        }
        return new ShortcutListDto(application, result.ToArray(), "Built-in Windows shortcut catalog");
    }
}
