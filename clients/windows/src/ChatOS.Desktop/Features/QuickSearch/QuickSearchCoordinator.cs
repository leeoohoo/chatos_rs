namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class QuickSearchCoordinator(
    QuickSearchWindow window,
    WindowsGlobalHotKeyService hotKey) : IDisposable
{
    private MainWindow? _mainWindow;

    public void Initialize(MainWindow mainWindow)
    {
        if (_mainWindow is not null) return;
        _mainWindow = mainWindow;
        hotKey.Pressed += OnHotKeyPressed;
        hotKey.Register(mainWindow);
    }

    public void Show()
    {
        if (_mainWindow is not null) window.Show(_mainWindow);
    }

    private void OnHotKeyPressed(object? sender, EventArgs e) => Show();

    public void Dispose()
    {
        hotKey.Pressed -= OnHotKeyPressed;
        hotKey.Dispose();
    }
}
