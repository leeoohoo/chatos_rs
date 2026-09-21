using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Windows.Graphics;
using Windows.System;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed partial class QuickSearchWindow : Window
{
    private MainWindow? _mainWindow;
    private readonly QuickSearchActionRouter _router;

    public QuickSearchWindow(QuickSearchViewModel viewModel, QuickSearchActionRouter router)
    {
        ViewModel = viewModel;
        _router = router;
        InitializeComponent();
        AppWindow.Title = "ChatOS Quick Search";
        AppWindow.Resize(new SizeInt32(720, 520));
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsAlwaysOnTop = true;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
            presenter.SetBorderAndTitleBar(false, false);
        }
        AppWindow.Closing += (_, args) =>
        {
            args.Cancel = true;
            AppWindow.Hide();
        };
    }

    public QuickSearchViewModel ViewModel { get; }

    public void Show(MainWindow mainWindow)
    {
        _mainWindow = mainWindow;
        ViewModel.Reset();
        CenterOnActiveDisplay(mainWindow);
        Activate();
        SearchBox.Focus(FocusState.Programmatic);
        SearchBox.SelectAll();
    }

    private void CenterOnActiveDisplay(MainWindow owner)
    {
        var display = DisplayArea.GetFromWindowId(owner.AppWindow.Id, DisplayAreaFallback.Primary);
        var area = display.WorkArea;
        AppWindow.Move(new PointInt32(
            area.X + Math.Max(0, (area.Width - AppWindow.Size.Width) / 2),
            area.Y + Math.Max(0, area.Height / 5)));
    }

    private async Task ExecuteSelectedAsync()
    {
        var selected = ViewModel.SelectedResult;
        if (selected is null || _mainWindow is null) return;
        AppWindow.Hide();
        await ViewModel.RecordUseAsync(selected);
        await _router.ExecuteAsync(selected, _mainWindow);
    }

    private async void OnResultDoubleTapped(object sender, DoubleTappedRoutedEventArgs e) =>
        await ExecuteSelectedAsync();

    private async void OnKeyDown(object sender, KeyRoutedEventArgs e)
    {
        switch (e.Key)
        {
            case VirtualKey.Down:
                ViewModel.MoveSelection(1);
                e.Handled = true;
                break;
            case VirtualKey.Up:
                ViewModel.MoveSelection(-1);
                e.Handled = true;
                break;
            case VirtualKey.Enter:
                e.Handled = true;
                await ExecuteSelectedAsync();
                break;
            case VirtualKey.Escape:
                AppWindow.Hide();
                _mainWindow?.Activate();
                e.Handled = true;
                break;
        }
    }
}
