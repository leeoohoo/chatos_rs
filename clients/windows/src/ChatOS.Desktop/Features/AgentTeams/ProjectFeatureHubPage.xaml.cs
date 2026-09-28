using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.AgentTeams;

public sealed partial class ProjectFeatureHubPage : Page
{
    private WorkspaceResourceKind _feature = WorkspaceResourceKind.AgentTeams;

    public ProjectFeatureHubPage(MainWindowViewModel shell)
    {
        Shell = shell;
        InitializeComponent();
        Shell.Projects.CollectionChanged += (_, _) => Bindings.Update();
    }

    public event EventHandler<ProjectFeatureRequestedEventArgs>? FeatureRequested;

    public MainWindowViewModel Shell { get; }

    public bool HasProjects => Shell.Projects.Count > 0;

    public string FeatureTitle => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "需求调研"
        : "Agent";

    public string FeatureDescription => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "在项目内确认需求、收集 Human 答案，并形成可执行的方案与计划。"
        : "进入项目团队，管理 Agent、私聊、任务、共享资产、成员提案与运行队列。";

    public string FeatureGlyph => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "\uE9D5"
        : "\uE716";

    public void Configure(WorkspaceResourceKind feature)
    {
        if (feature is not WorkspaceResourceKind.AgentTeams and not WorkspaceResourceKind.RequirementSurveys)
            throw new ArgumentOutOfRangeException(nameof(feature));
        _feature = feature;
        Bindings.Update();
    }

    private void OnProjectClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not ShellResourceViewModel project) return;
        FeatureRequested?.Invoke(this, new ProjectFeatureRequestedEventArgs(
            project,
            _feature == WorkspaceResourceKind.RequirementSurveys
                ? "requirement-surveys"
                : "agent-team"));
    }
}

public sealed record ProjectFeatureRequestedEventArgs(
    ShellResourceViewModel Project,
    string Tab) : EventArgs;
