using ChatOS.Core.Domain;
using ChatOS.Desktop.Features.Chat;
using ChatOS.Desktop.Features.Projects;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.AppShell;

public sealed partial class WorkspaceHostPage : UserControl
{
    private readonly ConversationPage _conversationPage;
    private readonly ProjectFilesPage _projectFilesPage;
    private readonly ProjectGitPage _projectGitPage;
    private readonly ProjectPlanPage _projectPlanPage;
    private readonly ProjectRunPage _projectRunPage;

    public WorkspaceHostPage(
        ConversationPage conversationPage,
        ProjectFilesPage projectFilesPage,
        ProjectGitPage projectGitPage,
        ProjectPlanPage projectPlanPage,
        ProjectRunPage projectRunPage,
        LocalizationViewModel localization)
    {
        _conversationPage = conversationPage;
        _projectFilesPage = projectFilesPage;
        _projectGitPage = projectGitPage;
        _projectPlanPage = projectPlanPage;
        _projectRunPage = projectRunPage;
        Localization = localization;
        InitializeComponent();
        WorkspaceNavigation.SelectedItem = ChatItem;
        WorkspacePageContent.Content = _conversationPage;
    }

    public LocalizationViewModel Localization { get; }

    public void Configure(ShellResourceViewModel? resource)
    {
        var isProject = resource?.Kind == WorkspaceResourceKind.Project;
        FilesItem.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;
        GitItem.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;
        PlanItem.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;
        RunItem.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;
        if (!isProject || WorkspaceNavigation.SelectedItem is not NavigationViewItem selected ||
            selected.Tag?.ToString() is not "chat" and not "files" and not "git" and not "plan" and not "run")
        {
            WorkspaceNavigation.SelectedItem = ChatItem;
            WorkspacePageContent.Content = _conversationPage;
        }
    }

    private void OnSelectionChanged(
        NavigationView sender,
        NavigationViewSelectionChangedEventArgs args)
    {
        if (args.SelectedItemContainer?.Tag?.ToString() == "files")
        {
            WorkspacePageContent.Content = _projectFilesPage;
            return;
        }

        if (args.SelectedItemContainer?.Tag?.ToString() == "git")
        {
            WorkspacePageContent.Content = _projectGitPage;
            return;
        }

        if (args.SelectedItemContainer?.Tag?.ToString() == "plan")
        {
            WorkspacePageContent.Content = _projectPlanPage;
            return;
        }

        if (args.SelectedItemContainer?.Tag?.ToString() == "run")
        {
            WorkspacePageContent.Content = _projectRunPage;
            return;
        }

        WorkspacePageContent.Content = _conversationPage;
    }
}
