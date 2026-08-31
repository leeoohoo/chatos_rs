using System.ComponentModel;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;
using ChatOS.Presentation.Settings;

namespace ChatOS.Desktop.Features.Plugins;

public sealed partial class PluginArtifactsWindow : Window
{
    private string? _adapterSessionId;

    public PluginArtifactsWindow(
        PluginArtifactsViewModel viewModel,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Localization = localization;
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TitleBar);
        AppWindow.Title = "ChatOS Artifacts";
        AppWindow.Resize(new SizeInt32(720, 560));
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = true;
        }

        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        ViewModel.Artifacts.CollectionChanged += (_, _) => UpdateVisualState();
        AppWindow.Closing += (_, args) =>
        {
            args.Cancel = true;
            AppWindow.Hide();
            ViewModel.Stop();
        };
        UpdateVisualState();
    }

    public PluginArtifactsViewModel ViewModel { get; }

    public LocalizationViewModel Localization { get; }

    public async Task ShowAsync(
        string? adapterSessionId = null,
        CancellationToken cancellationToken = default)
    {
        _adapterSessionId = adapterSessionId;
        ScopeText.Text = adapterSessionId is null
            ? Localization.AllPluginArtifacts
            : Localization.CurrentVisualSessionArtifacts;
        Activate();
        await ViewModel.LoadAsync(adapterSessionId, cancellationToken);
        UpdateVisualState();
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e) =>
        _ = DispatcherQueue.TryEnqueue(UpdateVisualState);

    private void UpdateVisualState()
    {
        if (EmptyState is null) return;
        EmptyState.Visibility = !ViewModel.IsLoading && !ViewModel.HasArtifacts
            ? Visibility.Visible
            : Visibility.Collapsed;
        ArtifactList.Visibility = ViewModel.HasArtifacts ? Visibility.Visible : Visibility.Collapsed;
        StatusText.Text = ViewModel.ErrorMessage ?? ViewModel.ActionMessage ??
            (ViewModel.IsTransferring ? Localization.ProcessingFile : string.Empty);
        StatusText.Foreground = ViewModel.ErrorMessage is null
            ? (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSSecondaryTextBrush"]
            : (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSFailureBrush"];
    }

    private async void OnRefreshClicked(object sender, RoutedEventArgs e) =>
        await ViewModel.LoadAsync(_adapterSessionId);

    private async void OnOpenClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: PluginArtifactItemViewModel item })
        {
            await ViewModel.OpenAsync(item);
        }
    }

    private async void OnSaveAsClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: PluginArtifactItemViewModel item })
        {
            await ViewModel.SaveAsAsync(item);
        }
    }
}
