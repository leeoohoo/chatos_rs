using System.ComponentModel;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;

namespace ChatOS.Desktop.Features.Projects;

public sealed partial class CreateProjectDialogContent : UserControl
{
    public CreateProjectDialogContent(CreateProjectViewModel viewModel)
    {
        ViewModel = viewModel;
        InitializeComponent();
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        WorkspacePicker.SelectedItem = ViewModel.SelectedWorkspace;
        Unloaded += (_, _) => ViewModel.PropertyChanged -= OnViewModelPropertyChanged;
        UpdateState();
    }

    public CreateProjectViewModel ViewModel { get; }

    public event EventHandler? StateChanged;

    public Task LoadAsync(CancellationToken cancellationToken = default) =>
        ViewModel.LoadInitialDirectoryAsync(cancellationToken);

    private async void OnWorkspaceSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        var selected = WorkspacePicker.SelectedItem as LocalConnectorWorkspaceStatus;
        if (Equals(selected, ViewModel.SelectedWorkspace)) return;
        await ViewModel.SelectWorkspaceAsync(selected);
    }

    private async void OnParentClicked(object sender, RoutedEventArgs e)
    {
        await ViewModel.GoToParentDirectoryAsync();
        // Keep the navigation affordances in sync even when the connector
        // returns the same root listing or completes without a property
        // change notification.
        UpdateState();
    }

    private async void OnOpenAbsolutePathClicked(object sender, RoutedEventArgs e) =>
        await OpenAbsolutePathAsync();

    private async void OnAddressPathKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != Windows.System.VirtualKey.Enter) return;
        e.Handled = true;
        await OpenAbsolutePathAsync();
    }

    private async Task OpenAbsolutePathAsync()
    {
        await ViewModel.OpenAbsolutePathAsync(AddressPathBox.Text);
        UpdateState();
        StateChanged?.Invoke(this, EventArgs.Empty);
    }

    private async void OnRefreshClicked(object sender, RoutedEventArgs e) =>
        await ViewModel.RefreshDirectoryAsync();

    private void OnToggleHiddenClicked(object sender, RoutedEventArgs e) =>
        ViewModel.ToggleHiddenDirectories();

    private void OnShowNewFolderClicked(object sender, RoutedEventArgs e)
    {
        NewFolderPanel.Visibility = Visibility.Visible;
        NewFolderNameBox.Text = string.Empty;
        NewFolderNameBox.Focus(FocusState.Programmatic);
    }

    private void OnCancelFolderClicked(object sender, RoutedEventArgs e) =>
        NewFolderPanel.Visibility = Visibility.Collapsed;

    private async void OnCreateFolderClicked(object sender, RoutedEventArgs e) =>
        await CreateFolderAsync();

    private async void OnNewFolderNameKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != Windows.System.VirtualKey.Enter) return;
        e.Handled = true;
        await CreateFolderAsync();
    }

    private async Task CreateFolderAsync()
    {
        var name = NewFolderNameBox.Text;
        await ViewModel.CreateDirectoryAsync(name);
        // UI Automation can invoke this button without moving focus.  Explicitly
        // synchronize the validation result here so an invalid name always has
        // visible and accessible feedback instead of only leaving the editor open.
        UpdateState();
        StateChanged?.Invoke(this, EventArgs.Empty);
        if (ViewModel.ErrorMessage is null)
        {
            NewFolderPanel.Visibility = Visibility.Collapsed;
        }
    }

    private async void OnFolderClicked(object sender, ItemClickEventArgs e) =>
        await ViewModel.OpenDirectoryAsync(e.ClickedItem as ProjectFileEntry);

    private void OnProjectNameChanged(object sender, TextChangedEventArgs e)
    {
        if (sender is TextBox textBox && textBox.Text != ViewModel.ProjectName)
        {
            ViewModel.UpdateProjectName(textBox.Text);
        }
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(CreateProjectViewModel.ProjectName) &&
            ProjectNameBox.Text != ViewModel.ProjectName)
        {
            ProjectNameBox.Text = ViewModel.ProjectName;
        }

        UpdateState();
        StateChanged?.Invoke(this, EventArgs.Empty);
    }

    private void UpdateState()
    {
        FormScroller.IsEnabled = !ViewModel.IsSaving;
        ErrorText.Text = ViewModel.ErrorMessage ?? string.Empty;
        NewFolderErrorText.Text = ViewModel.ErrorMessage ?? string.Empty;
        NewFolderErrorText.Visibility = NewFolderPanel.Visibility == Visibility.Visible &&
            !string.IsNullOrWhiteSpace(ViewModel.ErrorMessage)
                ? Visibility.Visible
                : Visibility.Collapsed;
        var hasWorkspace = ViewModel.SelectedWorkspace is not null;
        var hasCurrentDirectory = ViewModel.CurrentPath.Length > 0;
        NoWorkspaceNotice.Visibility = hasWorkspace ? Visibility.Collapsed : Visibility.Visible;
        // A null relative path denotes the selected workspace root.  Keep
        // the button disabled at that boundary even when a connector happens
        // to report a stale parent path for a drive root.
        ParentButton.IsEnabled = ViewModel.CanGoToParent;
        HiddenButton.IsEnabled = hasCurrentDirectory && !ViewModel.IsLoadingDirectory;
        NewFolderButton.IsEnabled = hasCurrentDirectory && ViewModel.IsDirectoryWritable && !ViewModel.IsLoadingDirectory;
        RefreshButton.IsEnabled = hasCurrentDirectory && !ViewModel.IsLoadingDirectory;
        FolderList.Visibility = ViewModel.IsLoadingDirectory || ViewModel.Entries.Count == 0
            ? Visibility.Collapsed
            : Visibility.Visible;
        LoadingPanel.Visibility = ViewModel.IsLoadingDirectory && ViewModel.Entries.Count == 0
            ? Visibility.Visible
            : Visibility.Collapsed;
        EmptyPanel.Visibility = !ViewModel.IsLoadingDirectory &&
            hasWorkspace &&
            ViewModel.HasUsableDirectoryListing &&
            ViewModel.Entries.Count == 0
            ? Visibility.Visible
            : Visibility.Collapsed;
        HiddenIcon.Glyph = ViewModel.ShowsHiddenDirectories ? "\uE890" : "\uED1A";
        ToolTipService.SetToolTip(
            HiddenButton,
            ViewModel.ShowsHiddenDirectories ? "隐藏隐藏与系统目录" : "显示隐藏与系统目录");
        ErrorText.Visibility = string.IsNullOrWhiteSpace(ViewModel.ErrorMessage)
            ? Visibility.Collapsed
            : Visibility.Visible;
    }
}
