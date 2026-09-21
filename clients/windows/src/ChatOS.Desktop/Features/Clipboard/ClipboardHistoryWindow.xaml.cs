using System.ComponentModel;
using ChatOS.Core.Domain;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;

namespace ChatOS.Desktop.Features.Clipboard;

public sealed partial class ClipboardHistoryWindow : Window
{
    private readonly WindowsClipboardHistoryMonitor _monitor;

    public ClipboardHistoryWindow(
        ClipboardHistoryViewModel viewModel,
        WindowsClipboardHistoryMonitor monitor)
    {
        ViewModel = viewModel;
        _monitor = monitor;
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TitleBar);
        AppWindow.Title = "ChatOS Clipboard History";
        AppWindow.Resize(new SizeInt32(760, 620));
        if (AppWindow.Presenter is OverlappedPresenter presenter) presenter.IsMaximizable = false;
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        ViewModel.Entries.CollectionChanged += (_, _) => UpdateVisualState();
        _monitor.HistoryChanged += OnHistoryChanged;
        AppWindow.Closing += (_, args) =>
        {
            args.Cancel = true;
            AppWindow.Hide();
        };
        UpdateVisualState();
    }

    public ClipboardHistoryViewModel ViewModel { get; }

    public async Task ShowAsync()
    {
        Activate();
        await ViewModel.LoadAsync();
        UpdateVisualState();
    }

    private void OnHistoryChanged(object? sender, EventArgs e) =>
        _ = DispatcherQueue.TryEnqueue(async () => await ViewModel.LoadAsync());

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e) =>
        _ = DispatcherQueue.TryEnqueue(UpdateVisualState);

    private void UpdateVisualState()
    {
        if (EmptyState is null) return;
        EmptyState.Visibility = !ViewModel.IsLoading && !ViewModel.HasEntries
            ? Visibility.Visible
            : Visibility.Collapsed;
        ErrorBar.IsOpen = !string.IsNullOrWhiteSpace(ViewModel.ErrorMessage);
        ErrorBar.Message = ViewModel.ErrorMessage ?? string.Empty;
    }

    private async void OnRefreshClicked(object sender, RoutedEventArgs e) => await ViewModel.LoadAsync();

    private async void OnRestoreClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ClipboardHistoryEntry entry })
            await ViewModel.RestoreAsync(entry);
    }

    private async void OnPinClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ClipboardHistoryEntry entry })
            await ViewModel.TogglePinnedAsync(entry);
    }

    private async void OnDeleteClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ClipboardHistoryEntry entry })
            await ViewModel.DeleteAsync(entry);
    }
}
