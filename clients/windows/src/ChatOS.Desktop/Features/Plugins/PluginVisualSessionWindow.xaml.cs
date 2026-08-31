using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics;
using Windows.Storage.Streams;
using ChatOS.Presentation.Settings;

namespace ChatOS.Desktop.Features.Plugins;

public sealed partial class PluginVisualSessionWindow : Window
{
    private const int GwlExStyle = -20;
    private const long WsExToolWindow = 0x00000080L;
    private const int SwShownoactivate = 4;
    private readonly PluginArtifactsWindow _artifactsWindow;
    private readonly IntPtr _hwnd;
    private bool _isVisible;
    private bool _positioned;
    private bool _syncingSelection;
    private string? _renderedFrameIdentity;

    public PluginVisualSessionWindow(
        PluginVisualSessionsViewModel viewModel,
        PluginArtifactsWindow artifactsWindow,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        _artifactsWindow = artifactsWindow;
        Localization = localization;
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TitleBar);
        AppWindow.Title = "ChatOS Visual Session";
        AppWindow.Resize(new SizeInt32(540, 480));
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsAlwaysOnTop = true;
            presenter.IsResizable = true;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
        }

        _hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        var style = GetWindowLongPtr(_hwnd, GwlExStyle).ToInt64();
        SetWindowLongPtr(_hwnd, GwlExStyle, new IntPtr(style | WsExToolWindow));
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        ViewModel.Sessions.CollectionChanged += (_, _) => UpdateVisualState();
        AppWindow.Closing += OnWindowClosing;
        UpdateVisualState();
    }

    public PluginVisualSessionsViewModel ViewModel { get; }

    public LocalizationViewModel Localization { get; }

    public void Hide()
    {
        if (!_isVisible) return;
        AppWindow.Hide();
        _isVisible = false;
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e) =>
        _ = DispatcherQueue.TryEnqueue(UpdateVisualState);

    private void UpdateVisualState()
    {
        if (SessionPicker is null) return;
        if (!ViewModel.HasSessions)
        {
            Hide();
            return;
        }

        ShowWithoutActivation();
        _syncingSelection = true;
        SessionPicker.SelectedItem = ViewModel.SelectedSession;
        _syncingSelection = false;
        var selected = ViewModel.SelectedSession;
        SessionTitleText.Text = selected?.Title ?? string.Empty;
        SessionDetailText.Text = selected is null
            ? string.Empty
            : string.IsNullOrWhiteSpace(selected.TargetApplication)
                ? selected.PluginDisplayName
                : $"{selected.PluginDisplayName} · {selected.TargetApplication}";
        CapturedAtText.Text = selected?.CapturedAt.ToLocalTime().ToString("HH:mm:ss") ?? string.Empty;
        ErrorText.Text = ViewModel.ErrorMessage ?? string.Empty;
        _ = RenderFrameAsync(selected);
    }

    private void ShowWithoutActivation()
    {
        if (!_positioned)
        {
            var display = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary);
            var work = display.WorkArea;
            AppWindow.Move(new PointInt32(
                work.X + work.Width - AppWindow.Size.Width - 24,
                work.Y + 24));
            _positioned = true;
        }

        if (_isVisible) return;
        ShowWindow(_hwnd, SwShownoactivate);
        _isVisible = true;
    }

    private async Task RenderFrameAsync(ChatOS.Connector.Plugins.PluginVisualSession? session)
    {
        var identity = session is null
            ? null
            : $"{session.AdapterSessionId}\n{session.Id}\n{session.FrameSequence}";
        if (identity == _renderedFrameIdentity) return;
        _renderedFrameIdentity = identity;
        if (session?.FrameData is not { Length: > 0 } bytes)
        {
            FrameImage.Source = null;
            FramePlaceholder.Visibility = Visibility.Visible;
            return;
        }

        try
        {
            using var stream = new InMemoryRandomAccessStream();
            using (var writer = new DataWriter(stream))
            {
                writer.WriteBytes(bytes);
                await writer.StoreAsync();
                await writer.FlushAsync();
                writer.DetachStream();
            }

            stream.Seek(0);
            var image = new BitmapImage();
            await image.SetSourceAsync(stream);
            if (_renderedFrameIdentity != identity) return;
            FrameImage.Source = image;
            FramePlaceholder.Visibility = Visibility.Collapsed;
        }
        catch (Exception exception)
        {
            if (_renderedFrameIdentity != identity) return;
            FrameImage.Source = null;
            FramePlaceholder.Visibility = Visibility.Visible;
            ErrorText.Text = Localization.Text(
                $"画面解码失败：{exception.Message}",
                $"Unable to decode the frame: {exception.Message}");
        }
    }

    private async void OnSessionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncingSelection || SessionPicker.SelectedItem is not ChatOS.Connector.Plugins.PluginVisualSession session)
        {
            return;
        }

        await ViewModel.SelectAsync(session.AdapterSessionId);
    }

    private async void OnDismissClicked(object sender, RoutedEventArgs e) =>
        await ViewModel.DismissSelectedAsync();

    private async void OnArtifactsClicked(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedSession is { } selected)
        {
            await _artifactsWindow.ShowAsync(selected.AdapterSessionId);
        }
    }

    private async void OnWindowClosing(AppWindow sender, AppWindowClosingEventArgs args)
    {
        args.Cancel = true;
        await ViewModel.DismissSelectedAsync();
    }

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    private static extern IntPtr GetWindowLongPtr64(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")]
    private static extern IntPtr GetWindowLongPtr32(IntPtr hWnd, int nIndex);

    private static IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex) =>
        IntPtr.Size == 8 ? GetWindowLongPtr64(hWnd, nIndex) : GetWindowLongPtr32(hWnd, nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    private static extern IntPtr SetWindowLongPtr64(IntPtr hWnd, int nIndex, IntPtr newLong);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongW")]
    private static extern IntPtr SetWindowLongPtr32(IntPtr hWnd, int nIndex, IntPtr newLong);

    private static IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr newLong) =>
        IntPtr.Size == 8
            ? SetWindowLongPtr64(hWnd, nIndex, newLong)
            : SetWindowLongPtr32(hWnd, nIndex, newLong);
}
