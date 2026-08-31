using ChatOS.Core.Domain;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.Projects;

public sealed partial class ProjectGitPage : UserControl
{
    public ProjectGitPage(ProjectGitViewModel viewModel, LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Localization = localization;
        InitializeComponent();
    }

    public ProjectGitViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }

    private async void OnWorkingDiffClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectGitChange change })
        {
            await ViewModel.OpenDiffCommand.ExecuteAsync(new ProjectGitDiffRequest(change, false));
        }
    }

    private async void OnStagedDiffClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectGitChange change })
        {
            await ViewModel.OpenDiffCommand.ExecuteAsync(new ProjectGitDiffRequest(change, true));
        }
    }

    private async void OnStageClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectGitChange change })
        {
            await ViewModel.StageChangeCommand.ExecuteAsync(change);
        }
    }

    private async void OnUnstageClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectGitChange change })
        {
            await ViewModel.UnstageChangeCommand.ExecuteAsync(change);
        }
    }

    private async void OnSwitchBranchClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectGitBranch branch })
        {
            await ViewModel.SwitchBranchCommand.ExecuteAsync(branch);
        }
    }

    private async void OnMergeBranchClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: ProjectGitBranch branch })
        {
            return;
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text($"合并分支“{branch.Name}”？", $"Merge branch “{branch.Name}”?"),
            Content = Localization.Text("Git 会保留未解决的冲突状态，不会自动丢弃本地修改。发生冲突后请在修改列表中处理。", "Git will keep unresolved conflict state and will not discard local changes. Resolve conflicts in the changes list."),
            PrimaryButtonText = Localization.Merge,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.MergeBranchCommand.ExecuteAsync(branch);
        }
    }

    private async void OnCreateBranchClick(object sender, RoutedEventArgs e)
    {
        var name = new TextBox { PlaceholderText = Localization.Text("例如 feature/windows-client", "For example, feature/windows-client") };
        var switchToBranch = new CheckBox
        {
            Content = Localization.Text("创建后立即切换", "Switch immediately after creating"),
            IsChecked = true,
        };
        var content = new StackPanel { Spacing = 10 };
        content.Children.Add(name);
        content.Children.Add(switchToBranch);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text("新建分支", "New branch"),
            Content = content,
            PrimaryButtonText = Localization.Create,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.CreateBranchCommand.ExecuteAsync(new ProjectGitBranchDraft(
                name.Text,
                switchToBranch.IsChecked == true));
        }
    }

    private async void OnAddRemoteClick(object sender, RoutedEventArgs e) =>
        await ShowRemoteDialogAsync(null);

    private async void OnEditRemoteClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ProjectGitRemote remote })
        {
            await ShowRemoteDialogAsync(remote);
        }
    }

    private async void OnRemoveRemoteClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: ProjectGitRemote remote })
        {
            return;
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text($"移除远程仓库“{remote.Name}”？", $"Remove remote “{remote.Name}”?"),
            Content = Localization.Text("只会删除本地 Git 远程配置，不会删除服务器上的仓库。", "Only the local Git remote configuration will be removed; the server repository will not be deleted."),
            PrimaryButtonText = Localization.Text("移除", "Remove"),
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.RemoveRemoteCommand.ExecuteAsync(remote);
        }
    }

    private async Task ShowRemoteDialogAsync(ProjectGitRemote? remote)
    {
        var name = new TextBox
        {
            Header = Localization.Name,
            Text = remote?.Name ?? "origin",
            PlaceholderText = "origin",
        };
        var url = new TextBox
        {
            Header = Localization.Text("仓库地址", "Repository URL"),
            Text = remote?.Url ?? string.Empty,
            PlaceholderText = "https://example.com/team/project.git",
        };
        var content = new StackPanel { Spacing = 10 };
        content.Children.Add(name);
        content.Children.Add(url);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = remote is null
                ? Localization.Text("添加远程仓库", "Add remote")
                : Localization.EditRemote,
            Content = content,
            PrimaryButtonText = Localization.Save,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ViewModel.SaveRemoteCommand.ExecuteAsync(new ProjectGitRemoteDraft(
                remote?.Name,
                name.Text,
                url.Text));
        }
    }
}
