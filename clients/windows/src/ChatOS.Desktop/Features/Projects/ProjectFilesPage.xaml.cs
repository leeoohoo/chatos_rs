using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace ChatOS.Desktop.Features.Projects;

public sealed partial class ProjectFilesPage : UserControl
{
    public ProjectFilesPage(ProjectFilesViewModel viewModel, LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Localization = localization;
        InitializeComponent();
    }

    public ProjectFilesViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }

    private void OnDirectoryItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectFileEntry entry)
        {
            ViewModel.OpenEntryCommand.Execute(entry);
        }
    }

    private void OnFileSearchItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectFileEntry entry)
        {
            ViewModel.OpenEntryCommand.Execute(entry);
        }
    }

    private void OnContentSearchItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectFileContentMatch match)
        {
            ViewModel.OpenContentMatchCommand.Execute(match);
        }
    }

    private void OnNavigationLocationClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectCodeNavigationLocation location)
        {
            ViewModel.OpenNavigationLocationCommand.Execute(location);
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

    private async void OnRenameEntryClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: ProjectFileEntry entry })
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
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.RenameEntryCommand.ExecuteAsync(
                new ProjectFileRenameRequest(entry, nameBox.Text));
        }
    }

    private async void OnDeleteEntryClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: ProjectFileEntry entry })
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

    private async Task ShowCreateDialogAsync(bool isDirectory)
    {
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
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.CreateEntryCommand.ExecuteAsync(
                new ProjectFileCreationRequest(nameBox.Text, isDirectory));
        }
    }
}
