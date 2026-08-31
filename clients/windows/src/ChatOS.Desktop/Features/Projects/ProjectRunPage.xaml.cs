using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Core.State;
using ChatOS.Presentation.Settings;
using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.Projects;

public sealed partial class ProjectRunPage : UserControl
{
    private readonly PetFavoriteProjectsManager _favorites;
    private bool _syncingFavorite;

    public ProjectRunPage(
        ProjectRunViewModel viewModel,
        PetFavoriteProjectsManager favorites,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        _favorites = favorites;
        Localization = localization;
        InitializeComponent();
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        _favorites.Changed += OnFavoritesChanged;
        Loaded += (_, _) => SyncFavoriteSwitch();
    }

    public ProjectRunViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(ProjectRunViewModel.ProjectId)) SyncFavoriteSwitch();
    }

    private void OnFavoritesChanged(object? sender, EventArgs e) =>
        _ = DispatcherQueue.TryEnqueue(SyncFavoriteSwitch);

    private void SyncFavoriteSwitch()
    {
        if (FavoriteProjectSwitch is null) return;
        _syncingFavorite = true;
        FavoriteProjectSwitch.IsEnabled = !string.IsNullOrWhiteSpace(ViewModel.ProjectId);
        FavoriteProjectSwitch.IsOn = _favorites.IsFavorite(ViewModel.ProjectId);
        _syncingFavorite = false;
    }

    private async void OnFavoriteProjectToggled(object sender, RoutedEventArgs e)
    {
        if (_syncingFavorite || ViewModel.ProjectId is not { Length: > 0 } projectId) return;
        try
        {
            await _favorites.SetFavoriteAsync(projectId, FavoriteProjectSwitch.IsOn);
        }
        catch (Exception exception)
        {
            ViewModel.ErrorMessage = exception.Message;
            SyncFavoriteSwitch();
        }
    }

    private void OnTargetItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectRunTarget target)
        {
            ViewModel.SelectTargetCommand.Execute(target);
        }
    }

    private void OnStopInstanceClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectRunInstance instance })
        {
            ViewModel.StopCommand.Execute(instance);
        }
    }

    private void OnDeleteInstanceClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectRunInstance instance })
        {
            ViewModel.DeleteInstanceCommand.Execute(instance);
        }
    }

    private void OnRemoveEnvironmentVariableClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectRunEnvironmentVariableViewModel variable })
        {
            ViewModel.RemoveEnvironmentVariableCommand.Execute(variable);
        }
    }
}
