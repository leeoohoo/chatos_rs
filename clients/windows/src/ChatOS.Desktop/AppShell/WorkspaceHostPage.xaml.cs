using ChatOS.Core.Domain;
using ChatOS.Desktop.Features.AgentTeams;
using ChatOS.Desktop.Features.Chat;
using ChatOS.Desktop.Features.Projects;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace ChatOS.Desktop.AppShell;

public sealed partial class WorkspaceHostPage : UserControl
{
    private readonly ConversationPage _conversationPage;
    private readonly ProjectFilesPage _projectFilesPage;
    private readonly ProjectGitPage _projectGitPage;
    private readonly ProjectRunPage _projectRunPage;
    private readonly AgentTeamPage _agentTeamPage;
    private readonly ProjectRequirementSurveysPage _requirementSurveysPage;
    private string? _projectId;
    private string _selectedWorkspaceTab = "chat";
    private string _selectedDirectoryTab = "files";

    // Keep the indicator naming explicit: project re-entry updates the same
    // visual selectors that the macOS sidebar exposes, rather than relying on
    // a stale selector-bar state.
    private Button DirectoryIndicator => DirectoryItem;
    private Button FilesIndicator => FilesItem;

    public WorkspaceHostPage(
        MainWindowViewModel viewModel,
        ConversationPage conversationPage,
        ProjectFilesPage projectFilesPage,
        ProjectGitPage projectGitPage,
        ProjectRunPage projectRunPage,
        AgentTeamPage agentTeamPage,
        ProjectRequirementSurveysPage requirementSurveysPage,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        _conversationPage = conversationPage;
        _projectFilesPage = projectFilesPage;
        _projectGitPage = projectGitPage;
        _projectRunPage = projectRunPage;
        _agentTeamPage = agentTeamPage;
        _requirementSurveysPage = requirementSurveysPage;
        _agentTeamPage.RequirementSurveysRequested += (_, _) =>
            SelectWorkspaceTab("requirement-surveys");
        Localization = localization;
        InitializeComponent();

        _projectRunPage.RemoveProjectRequested += (_, _) =>
            ProjectRemoveRequested?.Invoke(this, EventArgs.Empty);

        SetDirectorySelection("files");
        DirectoryPageContent.Content = _projectFilesPage;
        SetWorkspaceSelection("chat");
        ShowWorkspacePage(_conversationPage);
    }

    public LocalizationViewModel Localization { get; }

    public MainWindowViewModel ViewModel { get; }

    public event EventHandler<string>? ProjectTabRequested;

    public event EventHandler? ProjectRemoveRequested;

    public Task<bool> ConfirmCanChangeResourceAsync() =>
        _projectFilesPage.ConfirmCanLeaveAsync();

    public void OpenProjectFeature(string tab)
    {
        SelectWorkspaceTab(tab is "agent-team" or "requirement-surveys" ? tab : "chat");
    }

    public void Configure(ShellResourceViewModel? resource)
    {
        var isProject = resource?.Kind == WorkspaceResourceKind.Project;
        WorkspaceNavigationRow.Height = isProject ? new GridLength(44) : new GridLength(0);
        WorkspaceNavigationHost.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;
        DirectoryIndicator.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;
        FilesIndicator.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;
        RunItem.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;

        if (isProject)
        {
            _projectId = resource!.Id;
            SetDirectorySelection("files");
            DirectoryPageContent.Content = _projectFilesPage;
            SetWorkspaceSelection("directory");
            ShowProjectDirectory();
            return;
        }

        if (!isProject) _projectId = null;
        if (!isProject || _selectedWorkspaceTab is not "directory" and not "chat" and not "run" and
                not "agent-team" and not "requirement-surveys")
        {
            SetWorkspaceSelection("chat");
            ShowWorkspacePage(_conversationPage);
        }
    }

    private async void OnWorkspaceNavigationClicked(object sender, RoutedEventArgs args)
    {
        if (sender is not FrameworkElement { Tag: string tab }) return;
        if (_selectedWorkspaceTab == "directory" && tab != "directory" &&
            !await _projectFilesPage.ConfirmCanLeaveAsync())
        {
            return;
        }
        SelectWorkspaceTab(tab);
    }

    private void OnRetryProjectConversationClicked(object sender, RoutedEventArgs args)
    {
        if (_projectId is not null)
        {
            ProjectTabRequested?.Invoke(this, "chat");
        }
    }

    private void SelectWorkspaceTab(string tab)
    {
        SetWorkspaceSelection(tab);

        if (tab == "directory")
        {
            ShowProjectDirectory();
            NotifyCurrentDirectoryTab();
            return;
        }

        if (_projectId is not null) ProjectTabRequested?.Invoke(this, tab);
        ShowWorkspacePage(tab switch
        {
            "run" => _projectRunPage,
            "agent-team" => _agentTeamPage,
            "requirement-surveys" => _requirementSurveysPage,
            _ => _conversationPage,
        });
    }

    private async void OnDirectoryNavigationClicked(object sender, RoutedEventArgs args)
    {
        if (sender is not FrameworkElement { Tag: string tab }) return;
        if (_selectedDirectoryTab == tab || !await _projectFilesPage.ConfirmCanLeaveAsync())
        {
            return;
        }
        SetDirectorySelection(tab);
        DirectoryPageContent.Content = tab == "git" ? _projectGitPage : _projectFilesPage;
        if (_projectId is not null && _selectedWorkspaceTab == "directory")
            ProjectTabRequested?.Invoke(this, tab);
    }

    private void NotifyCurrentDirectoryTab()
    {
        if (_projectId is null) return;
        ProjectTabRequested?.Invoke(this, _selectedDirectoryTab);
    }

    private void SetWorkspaceSelection(string tab)
    {
        _selectedWorkspaceTab = tab;
        ApplySegmentSelection(
            (DirectoryItem, tab == "directory"),
            (ChatItem, tab == "chat"),
            (RunItem, tab == "run"),
            (AgentTeamItem, tab == "agent-team"),
            (RequirementSurveysItem, tab == "requirement-surveys"));
    }

    private void SetDirectorySelection(string tab)
    {
        _selectedDirectoryTab = tab;
        ApplySegmentSelection((FilesItem, tab == "files"), (GitItem, tab == "git"));
    }

    private static void ApplySegmentSelection(params (Button Button, bool Selected)[] buttons)
    {
        var accent = (Brush)Application.Current.Resources["ChatOSAccentBrush"];
        var subtle = (Brush)Application.Current.Resources["ChatOSSurfaceSubtleBrush"];
        foreach (var (button, selected) in buttons)
        {
            AutomationProperties.SetItemStatus(button, selected ? "selected" : "not-selected");
            button.Background = selected ? accent : subtle;
            button.Foreground = selected
                ? new SolidColorBrush(Microsoft.UI.Colors.White)
                : (Brush)Application.Current.Resources["ChatOSPrimaryTextBrush"];
            button.FontWeight = selected ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal;
        }
    }

    private void ShowProjectDirectory()
    {
        ProjectDirectoryView.Visibility = Visibility.Visible;
        WorkspacePageContent.Visibility = Visibility.Collapsed;
    }

    private void ShowWorkspacePage(object page)
    {
        WorkspacePageContent.Content = page;
        WorkspacePageContent.Visibility = Visibility.Visible;
        ProjectDirectoryView.Visibility = Visibility.Collapsed;
    }
}
