using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.AgentTeams;

public sealed partial class ProjectFeatureHubPage : Page
{
    private WorkspaceResourceKind _feature = WorkspaceResourceKind.AgentTeams;
    private readonly IAgentTeamService _teams;
    private int _summaryGeneration;

    public ProjectFeatureHubPage(MainWindowViewModel shell, IAgentTeamService teams)
    {
        Shell = shell;
        _teams = teams;
        InitializeComponent();
        Shell.Projects.CollectionChanged += (_, _) =>
        {
            Bindings.Update();
            if (_feature == WorkspaceResourceKind.RequirementSurveys) _ = LoadSurveySummaryAsync();
        };
        RefreshFeatureVisibility();
    }

    public event EventHandler<ProjectFeatureRequestedEventArgs>? FeatureRequested;

    public MainWindowViewModel Shell { get; }

    public ObservableCollection<ProjectSurveySummary> SurveyProjects { get; } = [];

    public bool HasProjects => Shell.Projects.Count > 0;

    public string FeatureTitle => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "需求调研"
        : "Agent";

    public string FeatureDescription => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "跨项目查看需求确认进度，再进入项目收集 Human 答案并形成可执行方案。"
        : "进入项目团队，管理 Agent、私聊、任务、共享资产、成员提案与运行队列。";

    public string FeatureHelpText => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "汇总当前账户所有本机项目的需求调研。选择项目后进入完整问卷、答案、方案与执行计划。"
        : "Agent 团队保存在对应的本机项目中。选择项目后将直接进入完整工作区。";

    public string FeatureListTitle => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "项目调研进度"
        : "选择项目";

    public string FeatureListDescription => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "优先处理待填写和等待方案的项目。"
        : "继续最近的工作，或者先从侧栏创建一个本机项目。";

    public bool IsLoadingSurveySummary { get; private set; }
    public string SurveySummaryError { get; private set; } = string.Empty;
    public int PendingSurveyCount => SurveyProjects.Sum(project => project.PendingCount);
    public int AwaitingSurveyCount => SurveyProjects.Sum(project => project.AwaitingCount);
    public int ResolvedSurveyCount => SurveyProjects.Sum(project => project.ResolvedCount);

    public string FeatureGlyph => _feature == WorkspaceResourceKind.RequirementSurveys
        ? "\uE9D5"
        : "\uE716";

    public void Configure(WorkspaceResourceKind feature)
    {
        if (feature is not WorkspaceResourceKind.AgentTeams and not WorkspaceResourceKind.RequirementSurveys)
            throw new ArgumentOutOfRangeException(nameof(feature));
        _feature = feature;
        Bindings.Update();
        RefreshFeatureVisibility();
        if (_feature == WorkspaceResourceKind.RequirementSurveys) _ = LoadSurveySummaryAsync();
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

    private void OnSurveyProjectClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not ProjectSurveySummary summary) return;
        FeatureRequested?.Invoke(this, new ProjectFeatureRequestedEventArgs(
            summary.Project,
            "requirement-surveys"));
    }

    private async void OnRefreshSurveySummaryClick(object sender, RoutedEventArgs e) =>
        await LoadSurveySummaryAsync();

    private async Task LoadSurveySummaryAsync()
    {
        var generation = Interlocked.Increment(ref _summaryGeneration);
        var owner = Shell.CurrentOwnerUserId;
        IsLoadingSurveySummary = true;
        SurveySummaryError = string.Empty;
        Bindings.Update();
        try
        {
            if (string.IsNullOrWhiteSpace(owner)) throw new InvalidOperationException("请先登录后查看需求调研汇总。");
            var summaries = new List<ProjectSurveySummary>();
            foreach (var project in Shell.Projects.ToArray())
            {
                var surveys = await _teams.ListProjectRequirementSurveysAsync(owner, project.Id);
                summaries.Add(ProjectSurveySummary.Create(project, surveys));
            }
            if (generation != _summaryGeneration || _feature != WorkspaceResourceKind.RequirementSurveys) return;
            SurveyProjects.Clear();
            foreach (var summary in summaries
                .OrderByDescending(value => value.PendingCount)
                .ThenByDescending(value => value.AwaitingCount)
                .ThenBy(value => value.Project.Title, StringComparer.CurrentCultureIgnoreCase))
            {
                SurveyProjects.Add(summary);
            }
        }
        catch (Exception exception)
        {
            if (generation == _summaryGeneration) SurveySummaryError = exception.Message;
        }
        finally
        {
            if (generation == _summaryGeneration)
            {
                IsLoadingSurveySummary = false;
                Bindings.Update();
                RefreshFeatureVisibility();
            }
        }
    }

    private void RefreshFeatureVisibility()
    {
        var surveys = _feature == WorkspaceResourceKind.RequirementSurveys;
        SurveySummaryPanel.Visibility = surveys ? Visibility.Visible : Visibility.Collapsed;
        SurveyProjectList.Visibility = surveys && HasProjects ? Visibility.Visible : Visibility.Collapsed;
        AgentProjectList.Visibility = !surveys && HasProjects ? Visibility.Visible : Visibility.Collapsed;
        NoProjectsPanel.Visibility = HasProjects ? Visibility.Collapsed : Visibility.Visible;
    }
}

public sealed class ProjectSurveySummary
{
    public ProjectSurveySummary(
        ShellResourceViewModel project,
        int pendingCount,
        int awaitingCount,
        int resolvedCount)
    {
        Project = project;
        PendingCount = pendingCount;
        AwaitingCount = awaitingCount;
        ResolvedCount = resolvedCount;
    }

    // WinUI's generated XAML metadata requires public setters even though these values
    // are only assigned by the constructor in application code.
    public ShellResourceViewModel Project { get; set; }
    public int PendingCount { get; set; }
    public int AwaitingCount { get; set; }
    public int ResolvedCount { get; set; }

    public string StatusLabel => PendingCount > 0
        ? $"有 {PendingCount} 张问卷等待 Human 填写"
        : AwaitingCount > 0
            ? $"有 {AwaitingCount} 张问卷等待形成方案"
            : ResolvedCount > 0 ? "当前调研均已形成方案" : "暂无需求调研";

    public static ProjectSurveySummary Create(
        ShellResourceViewModel project,
        IReadOnlyList<AgentRequirementSurvey> surveys) => new(
            project,
            surveys.Count(survey => survey.Status == AgentRequirementSurveyStatus.Pending),
            surveys.Count(survey => survey.Status != AgentRequirementSurveyStatus.Pending && survey.Resolution is null),
            surveys.Count(survey => survey.Resolution is not null));
}

public sealed class ProjectFeatureRequestedEventArgs(
    ShellResourceViewModel project,
    string tab) : EventArgs
{
    public ShellResourceViewModel Project { get; } = project;

    public string Tab { get; } = tab;
}
