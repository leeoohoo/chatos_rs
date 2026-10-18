using System.ComponentModel;
using System.Text;
using ChatOS.Core.Domain;
using ChatOS.Desktop.DesignSystem;
using ChatOS.Desktop.Features.Pet;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage.Streams;
using Windows.System;

namespace ChatOS.Desktop.Features.Projects;

public sealed partial class ProjectFilesPage : UserControl
{
    private readonly PetWindowController _petWindowController;
    private readonly Dictionary<string, TreeViewNode> _treeNodes = new(StringComparer.Ordinal);
    private readonly HashSet<string> _expandedTreePaths = new(StringComparer.Ordinal);
    private bool _treeMode = true;
    private bool _rebuildingTree;
    private bool _previewNavigationPending;

    public ProjectFilesPage(
        ProjectFilesViewModel viewModel,
        LocalizationViewModel localization,
        PetWindowController petWindowController)
    {
        ViewModel = viewModel;
        Localization = localization;
        _petWindowController = petWindowController;
        InitializeComponent();
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        ViewModel.Entries.CollectionChanged += OnEntriesCollectionChanged;
        ViewModel.DirectoryChanged += OnDirectoryChanged;
        ApplyDirectoryViewMode();
    }

    public ProjectFilesViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }

    private async void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(ProjectFilesViewModel.SelectedFile) or
            nameof(ProjectFilesViewModel.HasImagePreview))
        {
            await UpdateImagePreviewAsync();
        }
        if (e.PropertyName is nameof(ProjectFilesViewModel.SelectedFile) or
            nameof(ProjectFilesViewModel.HasMarkdownPreview))
        {
            MarkdownRichTextRenderer.Render(
                MarkdownPreview,
                ViewModel.HasMarkdownPreview ? ViewModel.SelectedFile?.Content : string.Empty);
        }
        if (e.PropertyName is nameof(ProjectFilesViewModel.IsSearchActive))
        {
            ApplyDirectoryViewMode();
        }
        if (e.PropertyName is nameof(ProjectFilesViewModel.SelectedFile) or
            nameof(ProjectFilesViewModel.IsLoading) or
            nameof(ProjectFilesViewModel.IsEditing) or
            nameof(ProjectFilesViewModel.HasNavigationTarget))
        {
            QueuePreviewNavigation();
        }
    }

    private void OnCodePreviewLoaded(object sender, RoutedEventArgs e) => QueuePreviewNavigation();

    private void QueuePreviewNavigation()
    {
        if (_previewNavigationPending) return;
        _previewNavigationPending = DispatcherQueue.TryEnqueue(
            Microsoft.UI.Dispatching.DispatcherQueuePriority.Low,
            () =>
            {
                _previewNavigationPending = false;
                if (!IsLoaded || ViewModel.IsLoading ||
                    !ViewModel.HasCodeTextPreview || !ViewModel.HasNavigationTarget) return;

                // Wait for x:Bind and visibility updates before native selection scrolls into view.
                CodePreview.UpdateLayout();
                var selection = ProjectFileTextSelection.ForLocation(
                    CodePreview.Text, (int)ViewModel.NavigationLine, (int)ViewModel.NavigationColumn);
                CodePreview.Focus(FocusState.Programmatic);
                CodePreview.Select(selection.Start, selection.Length);
            });
    }

    private void OnEntriesCollectionChanged(object? sender, System.Collections.Specialized.NotifyCollectionChangedEventArgs e)
    {
        if (_treeMode)
        {
            _ = DispatcherQueue.TryEnqueue(RebuildDirectoryTree);
        }
    }

    private void ApplyDirectoryViewMode()
    {
        var showSearchResults = ViewModel.IsSearchActive;
        var listVisible = !_treeMode && !showSearchResults;
        var treeVisible = _treeMode && !showSearchResults;
        DirectoryList.Visibility = listVisible ? Visibility.Visible : Visibility.Collapsed;
        DirectoryList.IsHitTestVisible = listVisible;
        DirectoryTree.Visibility = treeVisible ? Visibility.Visible : Visibility.Collapsed;
        DirectoryTree.IsHitTestVisible = treeVisible;
        if (_treeMode && !showSearchResults)
        {
            RebuildDirectoryTree();
        }
    }

    private void RebuildDirectoryTree()
    {
        if (!_treeMode || ViewModel.IsSearchActive || DirectoryTree is null)
        {
            return;
        }

        _rebuildingTree = true;
        try
        {
            DirectoryTree.RootNodes.Clear();
            _treeNodes.Clear();
            foreach (var entry in ViewModel.Entries)
            {
                var node = CreateTreeNode(entry);
                DirectoryTree.RootNodes.Add(node);
                _treeNodes[entry.Path] = node;
            }
        }
        finally
        {
            _rebuildingTree = false;
        }
        _ = RestoreExpandedTreeNodesAsync();
    }

    private static TreeViewNode CreateTreeNode(ProjectFileEntry entry) => new()
    {
        Content = entry,
        HasUnrealizedChildren = entry.IsDirectory,
    };

    private async void OnDirectoryChanged(object? sender, string path)
    {
        if (!_treeMode || ViewModel.IsSearchActive)
        {
            return;
        }

        if (!_treeNodes.TryGetValue(path, out var node))
        {
            return;
        }

        node.Children.Clear();
        node.HasUnrealizedChildren = true;
        if (node.IsExpanded)
        {
            await LoadTreeChildrenAsync(node);
        }
    }

    private async Task UpdateImagePreviewAsync()
    {
        var file = ViewModel.SelectedFile;
        if (file is null || !ViewModel.HasImagePreview)
        {
            ImagePreview.Source = null;
            ImagePreviewError.Visibility = Visibility.Collapsed;
            return;
        }

        ImagePreview.Source = null;
        ImagePreviewError.Visibility = Visibility.Collapsed;
        try
        {
            var bytes = FileBytes(file);
            using var stream = new InMemoryRandomAccessStream();
            using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
            {
                writer.WriteBytes(bytes);
                await writer.StoreAsync();
                await writer.FlushAsync();
            }
            stream.Seek(0);
            var image = new BitmapImage();
            await image.SetSourceAsync(stream);
            if (ViewModel.SelectedFile?.Path == file.Path)
            {
                ImagePreview.Source = image;
                ImagePreviewError.Visibility = Visibility.Collapsed;
            }
        }
        catch
        {
            if (ViewModel.SelectedFile?.Path == file.Path)
            {
                ImagePreview.Source = null;
                ImagePreviewError.Visibility = Visibility.Visible;
            }
        }
    }

    private async void OnCopySelectedClick(object sender, RoutedEventArgs e)
    {
        var file = ViewModel.SelectedFile;
        if (file is null) return;
        try
        {
            var package = new DataPackage();
            if (ViewModel.HasImagePreview && file.IsBinary)
            {
                var stream = new InMemoryRandomAccessStream();
                using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
                {
                    writer.WriteBytes(FileBytes(file));
                    await writer.StoreAsync();
                    await writer.FlushAsync();
                }
                stream.Seek(0);
                package.SetBitmap(RandomAccessStreamReference.CreateFromStream(stream));
            }
            else
            {
                package.SetText(file.Content);
            }
            Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(package);
            Windows.ApplicationModel.DataTransfer.Clipboard.Flush();
            ViewModel.EntryActionMessage = Localization.Text("已复制。", "Copied.");
        }
        catch (Exception exception)
        {
            ViewModel.ErrorMessage = exception.Message;
        }
    }

    private async void OnRevealSelectedClick(object sender, RoutedEventArgs e) =>
        await ViewModel.OpenSelectedExternallyAsync(ProjectFileExternalOpenMode.Reveal);

    private async void OnOpenSelectedClick(object sender, RoutedEventArgs e) =>
        await ViewModel.OpenSelectedExternallyAsync(ProjectFileExternalOpenMode.Default);

    private async void OnOpenInPetClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedFile is { } file)
        {
            await _petWindowController.OpenFileAsync(
                file.Path,
                ViewModel.HasNavigationTarget ? (int)ViewModel.NavigationLine : null);
        }
    }

    private static byte[] FileBytes(ProjectFileContent file) => file.IsBinary
        ? Convert.FromBase64String(file.Content)
        : Encoding.UTF8.GetBytes(file.Content);

    private async void OnDirectoryEntryClick(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { DataContext: ProjectFileEntry entry })
        {
            return;
        }

        if (await ConfirmCanLeaveAsync())
        {
            await ViewModel.OpenEntryCommand.ExecuteAsync(entry);
        }
    }

    private async void OnFileSearchItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectFileEntry entry && await ConfirmCanLeaveAsync())
        {
            await ViewModel.OpenEntryCommand.ExecuteAsync(entry);
        }
    }

    private async void OnContentSearchItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectFileContentMatch match && await ConfirmCanLeaveAsync())
        {
            await ViewModel.OpenContentMatchCommand.ExecuteAsync(match);
        }
    }

    private async void OnNavigationLocationClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectCodeNavigationLocation location && await ConfirmCanLeaveAsync())
        {
            await ViewModel.OpenNavigationLocationCommand.ExecuteAsync(location);
        }
    }

    private async void OnRefreshClick(object sender, RoutedEventArgs e)
    {
        if (await ConfirmCanLeaveAsync())
        {
            await ViewModel.RefreshCommand.ExecuteAsync(null);
        }
    }

    private async void OnGoToParentClick(object sender, RoutedEventArgs e)
    {
        if (await ConfirmCanLeaveAsync())
        {
            await ViewModel.GoToParentCommand.ExecuteAsync(null);
        }
    }

    private async void OnBreadcrumbClick(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { Tag: string path } ||
            !await ConfirmCanLeaveAsync())
        {
            return;
        }

        await ViewModel.NavigateToPathAsync(path);
    }

    private void OnToggleDirectoryViewClick(object sender, RoutedEventArgs e)
    {
        _treeMode = !_treeMode;
        ApplyDirectoryViewMode();
    }

    private async void OnTreeExpanding(TreeView sender, TreeViewExpandingEventArgs args)
    {
        if (args.Node.Content is not ProjectFileEntry entry || !entry.IsDirectory)
        {
            return;
        }

        // A node starts with unrealized children so the disclosure arrow is
        // available without recursively listing the whole project up front.
        if (!args.Node.HasUnrealizedChildren || args.Node.Children.Count > 0)
        {
            return;
        }

        _expandedTreePaths.Add(entry.Path);
        await LoadTreeChildrenAsync(args.Node);
    }

    private void OnTreeCollapsed(TreeView sender, TreeViewCollapsedEventArgs args)
    {
        if (_rebuildingTree || args.Node.Content is not ProjectFileEntry entry)
        {
            return;
        }

        _expandedTreePaths.RemoveWhere(path =>
            string.Equals(path, entry.Path, StringComparison.Ordinal) ||
            path.StartsWith(entry.Path + "/", StringComparison.Ordinal) ||
            path.StartsWith(entry.Path + "\\", StringComparison.Ordinal));
    }

    private async Task RestoreExpandedTreeNodesAsync()
    {
        foreach (var root in DirectoryTree.RootNodes.ToArray())
        {
            await RestoreExpandedTreeNodeAsync(root);
        }
    }

    private async Task RestoreExpandedTreeNodeAsync(TreeViewNode node)
    {
        if (node.Content is not ProjectFileEntry entry ||
            !_expandedTreePaths.Contains(entry.Path))
        {
            return;
        }

        await LoadTreeChildrenAsync(node);
        DirectoryTree.Expand(node);
        foreach (var child in node.Children.ToArray())
        {
            await RestoreExpandedTreeNodeAsync(child);
        }
    }

    private async Task LoadTreeChildrenAsync(TreeViewNode node)
    {
        if (node.Content is not ProjectFileEntry entry || !entry.IsDirectory)
        {
            return;
        }

        node.HasUnrealizedChildren = false;
        var listing = await ViewModel.LoadTreeDirectoryAsync(entry.Path);
        if (listing is null)
        {
            node.HasUnrealizedChildren = true;
            return;
        }

        node.Children.Clear();
        foreach (var child in listing.Entries
                     .OrderByDescending(value => value.IsDirectory)
                     .ThenBy(value => value.Name, StringComparer.CurrentCultureIgnoreCase))
        {
            var childNode = CreateTreeNode(child);
            node.Children.Add(childNode);
            _treeNodes[child.Path] = childNode;
        }
    }

    private async void OnTreeEntryClick(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { Tag: ProjectFileEntry entry } ||
            !_treeNodes.TryGetValue(entry.Path, out var node))
        {
            return;
        }

        if (entry.IsDirectory)
        {
            if (node.IsExpanded)
            {
                DirectoryTree.Collapse(node);
            }
            else
            {
                DirectoryTree.Expand(node);
            }
            return;
        }

        if (await ConfirmCanLeaveAsync())
        {
            await ViewModel.OpenEntryCommand.ExecuteAsync(entry);
        }
    }

    private void OnSearchKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Enter)
        {
            ViewModel.SearchCommand.Execute(null);
            e.Handled = true;
        }
    }

    private async void OnCreateFileClick(object sender, RoutedEventArgs e) =>
        await ShowCreateDialogAsync(isDirectory: false);

    private async void OnCreateDirectoryClick(object sender, RoutedEventArgs e) =>
        await ShowCreateDialogAsync(isDirectory: true);

    private async void OnCreateTreeFileClick(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: string parentPath })
        {
            await ShowCreateDialogAsync(isDirectory: false, parentPath);
        }
    }

    private async void OnCreateTreeDirectoryClick(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: string parentPath })
        {
            await ShowCreateDialogAsync(isDirectory: true, parentPath);
        }
    }

    private async void OnRenameEntryClick(object sender, RoutedEventArgs e)
    {
        var entry = EntryFromSender(sender);
        if (entry is null || !await ConfirmCanLeaveAsync())
        {
            return;
        }

        var nameBox = new TextBox
        {
            Text = entry.Name,
            PlaceholderText = Localization.Text("输入新名称", "Enter a new name"),
            SelectionStart = 0,
            SelectionLength = entry.Name.Length,
        };
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = entry.IsDirectory
                ? Localization.Text("重命名文件夹", "Rename folder")
                : Localization.Text("重命名文件", "Rename file"),
            Content = nameBox,
            PrimaryButtonText = Localization.Rename,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Primary,
            IsPrimaryButtonEnabled = IsValidEntryName(entry.Name),
        };
        nameBox.TextChanged += (_, _) => dialog.IsPrimaryButtonEnabled = IsValidEntryName(nameBox.Text);
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.RenameEntryCommand.ExecuteAsync(
                new ProjectFileRenameRequest(entry, nameBox.Text));
        }
    }

    private async void OnDeleteEntryClick(object sender, RoutedEventArgs e)
    {
        var entry = EntryFromSender(sender);
        if (entry is null || !await ConfirmCanLeaveAsync())
        {
            return;
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = entry.IsDirectory
                ? Localization.Text($"删除文件夹“{entry.Name}”？", $"Delete folder “{entry.Name}”?")
                : Localization.Text($"删除文件“{entry.Name}”？", $"Delete file “{entry.Name}”?"),
            Content = entry.IsDirectory
                ? Localization.Text("文件夹及其中的全部内容会被删除，此操作无法在客户端撤销。", "The folder and all its contents will be deleted. This cannot be undone in the client.")
                : Localization.Text("文件会被删除，此操作无法在客户端撤销。", "The file will be deleted. This cannot be undone in the client."),
            PrimaryButtonText = Localization.Delete,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.DeleteEntryCommand.ExecuteAsync(entry);
        }
    }

    private ProjectFileEntry? EntryFromSender(object sender)
    {
        if (sender is FrameworkElement { DataContext: ProjectFileEntry entry })
        {
            return entry;
        }

        if (sender is not FrameworkElement { Tag: string path }) return null;

        var listedEntry = ViewModel.Entries.FirstOrDefault(value =>
            string.Equals(value.Path, path, StringComparison.Ordinal));
        if (listedEntry is not null) return listedEntry;

        // Expanded children are loaded into native nodes, outside the current listing.
        return _treeNodes.TryGetValue(path, out var node) ? node.Content as ProjectFileEntry : null;
    }

    private async Task ShowCreateDialogAsync(bool isDirectory, string? parentPath = null)
    {
        if (!await ConfirmCanLeaveAsync())
        {
            return;
        }

        var nameBox = new TextBox
        {
            PlaceholderText = isDirectory
                ? Localization.Text("输入文件夹名称", "Enter a folder name")
                : Localization.Text("输入文件名称", "Enter a file name"),
        };
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = isDirectory ? Localization.NewFolder : Localization.NewFile,
            Content = nameBox,
            PrimaryButtonText = Localization.Create,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Primary,
            IsPrimaryButtonEnabled = false,
        };
        nameBox.TextChanged += (_, _) => dialog.IsPrimaryButtonEnabled = IsValidEntryName(nameBox.Text);
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.CreateEntryCommand.ExecuteAsync(
                new ProjectFileCreationRequest(nameBox.Text, isDirectory, parentPath));
        }
    }

    public async Task<bool> ConfirmCanLeaveAsync()
    {
        if (!ViewModel.IsDirty)
        {
            return true;
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.UnsavedFileChanges,
            Content = Localization.UnsavedFileChangesNotice,
            PrimaryButtonText = Localization.SaveAndContinue,
            SecondaryButtonText = Localization.DiscardChanges,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Primary,
        };
        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
        {
            await ViewModel.SaveCommand.ExecuteAsync(null);
            return !ViewModel.IsDirty;
        }

        if (result == ContentDialogResult.Secondary)
        {
            ViewModel.CancelEditingCommand.Execute(null);
            return true;
        }

        return false;
    }

    private static bool IsValidEntryName(string? value)
    {
        var name = value?.Trim();
        return !string.IsNullOrEmpty(name) &&
               !name.Contains('/') &&
               !name.Contains('\\');
    }
}
