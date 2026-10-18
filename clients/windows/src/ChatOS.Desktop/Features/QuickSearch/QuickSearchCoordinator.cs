using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class QuickSearchCoordinator(
    QuickSearchWindow window,
    QuickSearchActionRouter router,
    WindowsGlobalHotKeyService hotKey,
    ChatOS.Core.State.AppPreferencesManager preferences) : IDisposable
{
    private MainWindow? _mainWindow;

    public void Initialize(MainWindow mainWindow)
    {
        if (_mainWindow is not null) return;
        _mainWindow = mainWindow;
        hotKey.ActionPressed += OnHotKeyPressed;
        hotKey.Register(mainWindow, preferences);
    }

    public void Show()
    {
        if (_mainWindow is not null) window.Show(_mainWindow);
    }

    private async void OnHotKeyPressed(object? sender, WindowsGlobalUtilityAction action)
    {
        if (_mainWindow is null) return;
        if (action == WindowsGlobalUtilityAction.QuickSearch)
        {
            Show();
            return;
        }

        var builtIn = action switch
        {
            WindowsGlobalUtilityAction.Screenshot => QuickSearchBuiltInAction.Screenshot,
            WindowsGlobalUtilityAction.ScreenRecording => QuickSearchBuiltInAction.ScreenRecording,
            WindowsGlobalUtilityAction.ClipboardHistory => QuickSearchBuiltInAction.ClipboardHistory,
            _ => (QuickSearchBuiltInAction?)null,
        };
        try
        {
            await router.ExecuteBuiltInAsync(builtIn, _mainWindow);
        }
        catch (Exception exception)
        {
            ChatOS.Desktop.StartupDiagnostics.RecordUnhandled("global utility shortcut", exception);
        }
    }

    public void Dispose()
    {
        hotKey.ActionPressed -= OnHotKeyPressed;
        hotKey.Dispose();
    }
}
