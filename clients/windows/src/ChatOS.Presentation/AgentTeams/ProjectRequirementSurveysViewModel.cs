using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.AgentTeams;

public sealed partial class ProjectRequirementSurveysViewModel : ObservableObject, IDisposable
{
    private readonly IAgentTeamService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _refreshGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;
    private string? _ownerUserId;
    private string? _projectId;
    private long _generation;
    private int _localMutationCount;

    public ProjectRequirementSurveysViewModel(
        IAgentTeamService service,
        IUiDispatcher dispatcher)
    {
        _service = service;
        _dispatcher = dispatcher;
        _service.Changed += OnServiceChanged;
    }

    public ObservableCollection<ProjectRequirementSurveyItemViewModel> Surveys { get; } = [];

    public bool HasSurveys => Surveys.Count > 0;
    public bool HasSelection => SelectedSurvey is not null;
    public bool CanSubmitSelected => SelectedSurvey?.CanSubmit == true && !IsBusy;
    public bool IsOpen => _sessionCancellation is not null;
    public bool CanRefresh => IsOpen && !IsBusy;
    public int PendingCount => Surveys.Count(value => value.IsPending);
    public int AwaitingResolutionCount => Surveys.Count(value => value.IsAwaitingResolution);
    public int ResolvedCount => Surveys.Count(value => value.HasResolution);

    [ObservableProperty]
    private string _projectName = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelection))]
    [NotifyPropertyChangedFor(nameof(CanSubmitSelected))]
    private ProjectRequirementSurveyItemViewModel? _selectedSurvey;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSubmitSelected))]
    [NotifyPropertyChangedFor(nameof(CanRefresh))]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string _statusMessage = string.Empty;

    public async Task OpenAsync(
        string ownerUserId,
        WorkspaceProject project,
        CancellationToken cancellationToken = default)
    {
        CancelSession();
        _ownerUserId = ownerUserId;
        _projectId = project.Id;
        ProjectName = project.Name;
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() =>
        {
            Surveys.Clear();
            SelectedSurvey = null;
            NotifySurveyStateChanged();
            OnPropertyChanged(nameof(IsOpen));
            OnPropertyChanged(nameof(CanRefresh));
        }, cancellationToken).ConfigureAwait(false);
        await RefreshAsync(_sessionCancellation.Token).ConfigureAwait(false);
    }

    public async Task RefreshAsync(CancellationToken cancellationToken = default)
    {
        var context = RequireContext(cancellationToken);
        await _refreshGate.WaitAsync(context.Token).ConfigureAwait(false);
        try
        {
            await SetBusyAsync(true, null, context.Token).ConfigureAwait(false);
            var surveys = await _service.ListProjectRequirementSurveysAsync(
                context.Owner, context.Project, context.Token).ConfigureAwait(false);
            EnsureCurrent(context.Generation, context.Token);
            await _dispatcher.InvokeAsync(() => ReplaceSurveys(surveys), context.Token)
                .ConfigureAwait(false);
            await SetBusyAsync(false, null, context.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (context.Token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await SetBusyAsync(false, exception.Message, CancellationToken.None).ConfigureAwait(false);
        }
        finally
        {
            _refreshGate.Release();
            context.Dispose();
        }
    }

    public async Task<bool> SubmitAsync(
        AgentRequirementSurvey survey,
        AgentRequirementSubmission submission)
    {
        var context = RequireContext(CancellationToken.None);
        try
        {
            if (!string.Equals(survey.ProjectId, context.Project, StringComparison.Ordinal) ||
                survey.Status != AgentRequirementSurveyStatus.Pending)
                throw new InvalidOperationException("这张调研已不能提交，请刷新后重试。");
            AgentRequirementSurvey.ValidateSubmission(submission, survey.Draft.Questions);
            await SetBusyAsync(true, null, context.Token).ConfigureAwait(false);
            Interlocked.Increment(ref _localMutationCount);
            AgentRequirementSurvey updated;
            try
            {
                updated = await _service.SubmitProjectRequirementSurveyAsync(
                    context.Owner, context.Project, survey.Id, submission, context.Token)
                    .ConfigureAwait(false);
            }
            finally
            {
                Interlocked.Decrement(ref _localMutationCount);
            }
            EnsureCurrent(context.Generation, context.Token);
            await _dispatcher.InvokeAsync(() =>
            {
                ReplaceSurvey(updated);
                StatusMessage = "答案已提交，正在等待有权限的项目经理或 Agent 形成方案。";
            }, context.Token).ConfigureAwait(false);
            await SetBusyAsync(false, null, context.Token, preserveStatus: true).ConfigureAwait(false);
            return true;
        }
        catch (OperationCanceledException) when (context.Token.IsCancellationRequested)
        {
            return false;
        }
        catch (Exception exception)
        {
            await SetBusyAsync(false, exception.Message, CancellationToken.None).ConfigureAwait(false);
            return false;
        }
        finally
        {
            context.Dispose();
        }
    }

    private void ReplaceSurveys(IEnumerable<AgentRequirementSurvey> surveys)
    {
        var selectedId = SelectedSurvey?.Id;
        var ordered = surveys
            .OrderBy(value => value.Status == AgentRequirementSurveyStatus.Pending ? 0 :
                value.Resolution is null ? 1 : 2)
            .ThenByDescending(value => value.CreatedAtUnixMs)
            .ThenBy(value => value.Id, StringComparer.Ordinal)
            .Select(value => new ProjectRequirementSurveyItemViewModel(value))
            .ToArray();
        Surveys.Clear();
        foreach (var survey in ordered) Surveys.Add(survey);
        SelectedSurvey = Surveys.FirstOrDefault(value => value.Id == selectedId)
            ?? Surveys.FirstOrDefault();
        NotifySurveyStateChanged();
    }

    private void ReplaceSurvey(AgentRequirementSurvey survey)
    {
        var values = Surveys.Select(value => value.Survey.Id == survey.Id ? survey : value.Survey);
        ReplaceSurveys(values);
    }

    private void NotifySurveyStateChanged()
    {
        OnPropertyChanged(nameof(HasSurveys));
        OnPropertyChanged(nameof(PendingCount));
        OnPropertyChanged(nameof(AwaitingResolutionCount));
        OnPropertyChanged(nameof(ResolvedCount));
        OnPropertyChanged(nameof(CanSubmitSelected));
    }

    private async void OnServiceChanged(object? sender, AgentTeamChangedEventArgs args)
    {
        if (Volatile.Read(ref _localMutationCount) > 0 ||
            !string.Equals(args.OwnerUserId, _ownerUserId, StringComparison.Ordinal) ||
            args.ProjectId is not null && !string.Equals(args.ProjectId, _projectId, StringComparison.Ordinal))
            return;
        try
        {
            await RefreshAsync(_sessionCancellation?.Token ?? CancellationToken.None).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
        }
    }

    private Task SetBusyAsync(
        bool busy,
        string? error,
        CancellationToken cancellationToken,
        bool preserveStatus = false) =>
        _dispatcher.InvokeAsync(() =>
        {
            IsBusy = busy;
            ErrorMessage = error;
            if (!preserveStatus) StatusMessage = busy || error is not null ? string.Empty : "已刷新";
        }, cancellationToken);

    private SessionContext RequireContext(CancellationToken cancellationToken)
    {
        if (_ownerUserId is null || _projectId is null || _sessionCancellation is null)
            throw new InvalidOperationException("需求调研中心尚未打开。");
        return new SessionContext(_ownerUserId, _projectId, _generation,
            CancellationTokenSource.CreateLinkedTokenSource(
                cancellationToken, _sessionCancellation.Token));
    }

    private void EnsureCurrent(long generation, CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (generation != _generation) throw new OperationCanceledException(cancellationToken);
    }

    private void CancelSession()
    {
        Interlocked.Increment(ref _generation);
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
        OnPropertyChanged(nameof(IsOpen));
        OnPropertyChanged(nameof(CanRefresh));
    }

    public void Dispose()
    {
        _service.Changed -= OnServiceChanged;
        CancelSession();
        _refreshGate.Dispose();
    }

    private sealed record SessionContext(
        string Owner,
        string Project,
        long Generation,
        CancellationTokenSource Linked) : IDisposable
    {
        public CancellationToken Token => Linked.Token;
        public void Dispose() => Linked.Dispose();
    }
}

public sealed class ProjectRequirementSurveyItemViewModel
{
    public ProjectRequirementSurveyItemViewModel(AgentRequirementSurvey survey)
    {
        Survey = survey;
        var answers = survey.Submission?.Answers.ToDictionary(
            value => value.QuestionId, StringComparer.Ordinal) ?? [];
        Questions = survey.Draft.Questions.Select(question =>
        {
            var selected = answers.GetValueOrDefault(question.Id)?.SelectedOptionIds ?? [];
            var selectedSet = selected.ToHashSet(StringComparer.Ordinal);
            var answer = string.Join("、", question.Options
                .Where(option => selectedSet.Contains(option.Id))
                .Select(option => option.Label));
            return new ProjectRequirementSurveyQuestionViewModel(
                question.Prompt,
                question.Kind == AgentRequirementQuestionKind.SingleChoice ? "单选" : "多选",
                question.IsRequired,
                string.IsNullOrWhiteSpace(answer) ? "尚未填写" : answer);
        }).ToArray();
    }

    public AgentRequirementSurvey Survey { get; }
    public string Id => Survey.Id;
    public string Title => Survey.Draft.Title;
    public string Purpose => Survey.Draft.Purpose;
    public string StatusText => IsPending ? "待填写" : HasResolution ? "已形成方案" : "等待形成方案";
    public string QuestionCountText => $"{Survey.Draft.Questions.Count} 个问题";
    public string CreatedAtText => DateTimeOffset.FromUnixTimeMilliseconds(Survey.CreatedAtUnixMs)
        .ToLocalTime().ToString("g");
    public bool IsPending => Survey.Status == AgentRequirementSurveyStatus.Pending;
    public bool IsSubmitted => !IsPending;
    public bool HasResolution => Survey.Resolution is not null;
    public bool IsAwaitingResolution => IsSubmitted && !HasResolution;
    public bool CanSubmit => IsPending;
    public string PermissionText => IsPending
        ? "Human 可填写并提交；Agent 不可代替 Human 作答。"
        : HasResolution
            ? "调研已解决并转为只读；解决方案由获授调研权限的项目经理或 Agent 提交。"
            : "Human 答案已锁定并转为只读；仅获授调研权限的项目经理或 Agent 可以形成解决方案。";
    public string SubmissionNotes => string.IsNullOrWhiteSpace(Survey.Submission?.Notes)
        ? "无补充备注" : Survey.Submission.Notes;
    public string ResolutionSummary => Survey.Resolution?.Summary ?? "答案已提交，方案尚未生成。";
    public string SolutionMarkdown => Survey.Resolution?.SolutionMarkdown ?? string.Empty;
    public string RisksAndOpenQuestions => string.IsNullOrWhiteSpace(
        Survey.Resolution?.RisksAndOpenQuestions) ? "无" : Survey.Resolution!.RisksAndOpenQuestions;
    public string RelatedMaterials => string.IsNullOrWhiteSpace(Survey.Resolution?.RelatedMaterials)
        ? "无" : Survey.Resolution!.RelatedMaterials;
    public IReadOnlyList<ProjectRequirementSurveyQuestionViewModel> Questions { get; }
    public IReadOnlyList<AgentRequirementExecutionStep> ExecutionSteps =>
        Survey.Resolution?.ExecutionSteps ?? [];
}

public sealed record ProjectRequirementSurveyQuestionViewModel(
    string Prompt,
    string KindText,
    bool IsRequired,
    string AnswerText)
{
    public string RequirementText => IsRequired ? "必填" : "选填";
}
