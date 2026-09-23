using System.Diagnostics;
using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using ChatOS.Desktop.Features.Clipboard;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class QuickSearchActionRouter(
    ClipboardHistoryWindow clipboardHistory,
    WindowsScreenRecordingCoordinator screenRecording)
{
    public async Task ExecuteAsync(QuickSearchResult result, MainWindow mainWindow)
    {
        switch (result.Action.Kind)
        {
            case QuickSearchActionKind.OpenProject:
            case QuickSearchActionKind.OpenContact:
                var resource = mainWindow.ViewModel.Projects.Concat(mainWindow.ViewModel.Contacts)
                    .FirstOrDefault(item => string.Equals(item.Id, result.Action.Value, StringComparison.Ordinal));
                if (resource is not null) mainWindow.ViewModel.SelectedResource = resource;
                mainWindow.Activate();
                break;
            case QuickSearchActionKind.OpenApplication:
            case QuickSearchActionKind.OpenFile:
                Start(result.Action.Value);
                break;
            case QuickSearchActionKind.RevealFile:
                Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{result.Action.Value}\"")
                {
                    UseShellExecute = true,
                });
                break;
            case QuickSearchActionKind.BuiltIn:
                await ExecuteBuiltInAsync(result.Action.BuiltInAction, mainWindow);
                break;
        }
    }

    private async Task ExecuteBuiltInAsync(QuickSearchBuiltInAction? action, MainWindow mainWindow)
    {
        switch (action)
        {
            case QuickSearchBuiltInAction.Screenshot:
                Start("ms-screenclip:");
                break;
            case QuickSearchBuiltInAction.ScreenRecording:
                await screenRecording.StartOrShowControlsAsync();
                break;
            case QuickSearchBuiltInAction.ClipboardHistory:
                await clipboardHistory.ShowAsync();
                break;
            case QuickSearchBuiltInAction.OpenSettings:
            case QuickSearchBuiltInAction.OpenRuntimePermissions:
                mainWindow.OpenSettings();
                mainWindow.Activate();
                break;
        }
    }

    private static void Start(string target) => Process.Start(new ProcessStartInfo(target)
    {
        UseShellExecute = true,
    });
}
