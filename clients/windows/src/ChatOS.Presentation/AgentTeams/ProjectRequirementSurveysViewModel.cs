using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.AgentTeams;

public sealed partial class ProjectRequirementSurveysViewModel : ObservableObject, IDisposable
{
    private readonly IProjectRequirementSurveyService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _refreshGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;
    private CancellationTokenSource? _detailCancellation;
    private string? _ownerUserId;
    private string? _projectId;
    private long _generation;
    private int _localMutationCount;

    public ProjectRequirementSurveysViewModel(
        IProjectRequirementSurveyService service,
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

    partial void OnSelectedSurveyChanged(ProjectRequirementSurveyItemViewModel? value)
    {
        _detailCancellation?.Cancel();
        _detailCancellation?.Dispose();
        _detailCancellation = value is null || _sessionCancellation is null
            ? null
            : CancellationTokenSource.CreateLinkedTokenSource(_sessionCancellation.Token);
        if (value is not null && _detailCancellation is not null)
            _ = LoadDetailsAsync(value, _detailCancellation.Token);
    }

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
            var surveys = await _service.ListAsync(
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
                updated = await _service.SubmitAsync(
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
                StatusMessage = "答案已提交，本地任务已恢复执行。";
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

    public async Task<bool> ConfirmSelectedPlanAsync()
    {
        var item = SelectedSurvey;
        var plan = item?.Details?.Plan;
        if (item is null || plan is null || !item.CanConfirmPlan) return false;
        var context = RequireContext(CancellationToken.None);
        try
        {
            item.SetPlanBusy(true, null);
            Interlocked.Increment(ref _localMutationCount);
            try
            {
                await _service.ConfirmPlanAsync(
                    context.Owner, context.Project, item.Id, plan.Revision, context.Token)
                    .ConfigureAwait(false);
            }
            finally
            {
                Interlocked.Decrement(ref _localMutationCount);
            }
            EnsureCurrent(context.Generation, context.Token);
            await LoadDetailsAsync(item, context.Token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (ReferenceEquals(SelectedSurvey, item))
                    StatusMessage = "方案已确认，本次不会自动执行任何任务。";
            }, context.Token).ConfigureAwait(false);
            return true;
        }
        catch (OperationCanceledException) when (context.Token.IsCancellationRequested)
        {
            return false;
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => item.SetPlanBusy(false,
                $"确认未完成：{exception.Message} 请刷新后重新查看当前版本。"),
                CancellationToken.None).ConfigureAwait(false);
            return false;
        }
        finally
        {
            context.Dispose();
        }
    }

    public async Task<bool> AdoptAndConfirmSelectedPlanAsync()
    {
        var item = SelectedSurvey;
        var markdown = item?.LegacyPlanMarkdown;
        if (item is null || !item.CanAdoptLegacyPlan || string.IsNullOrWhiteSpace(markdown))
            return false;
        var context = RequireContext(CancellationToken.None);
        try
        {
            item.SetPlanBusy(true, null);
            Interlocked.Increment(ref _localMutationCount);
            AgentRequirementPlan saved;
            try
            {
                saved = await _service.SaveSourceReplyAsPlanAsync(
                    context.Owner, context.Project, item.Id, markdown, context.Token)
                    .ConfigureAwait(false);
                await _service.ConfirmPlanAsync(
                    context.Owner, context.Project, item.Id, saved.Revision, context.Token)
                    .ConfigureAwait(false);
            }
            finally
            {
                Interlocked.Decrement(ref _localMutationCount);
            }
            EnsureCurrent(context.Generation, context.Token);
            await LoadDetailsAsync(item, context.Token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (ReferenceEquals(SelectedSurvey, item))
                    StatusMessage = "方案已保存并确认，本次不会自动执行任何任务。";
            }, context.Token).ConfigureAwait(false);
            return true;
        }
        catch (OperationCanceledException) when (context.Token.IsCancellationRequested)
        {
            return false;
        }
        catch (Exception exception)
        {
            // Saving and confirming are separate optimistic operations. If save
            // succeeded but confirm conflicted, refresh so the pending v1 remains
            // visible instead of encouraging a duplicate save.
            await LoadDetailsAsync(item, CancellationToken.None).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => item.SetPlanBusy(false,
                $"保存或确认未完成：{exception.Message} 请刷新后查看当前方案。"),
                CancellationToken.None).ConfigureAwait(false);
            return false;
        }
        finally
        {
            context.Dispose();
        }
    }

    private async Task LoadDetailsAsync(
        ProjectRequirementSurveyItemViewModel item,
        CancellationToken cancellationToken)
    {
        if (_ownerUserId is null || _projectId is null) return;
        try
        {
            await _dispatcher.InvokeAsync(() => item.SetPlanBusy(true, null), cancellationToken)
                .ConfigureAwait(false);
            var details = await _service.GetDetailsAsync(
                _ownerUserId, _projectId, item.Id, cancellationToken).ConfigureAwait(false);
            cancellationToken.ThrowIfCancellationRequested();
            await _dispatcher.InvokeAsync(() =>
            {
                if (ReferenceEquals(SelectedSurvey, item)) item.SetDetails(details);
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (ReferenceEquals(SelectedSurvey, item))
                    item.SetPlanBusy(false, $"暂时无法读取方案：{exception.Message}");
            }, CancellationToken.None).ConfigureAwait(false);
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

    private async void OnServiceChanged(
        object? sender,
        ProjectRequirementSurveyChangedEventArgs args)
    {
        if (Volatile.Read(ref _localMutationCount) > 0 ||
            !string.Equals(args.OwnerUserId, _ownerUserId, StringComparison.Ordinal) ||
            !string.Equals(args.ProjectId, _projectId, StringComparison.Ordinal))
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
        _detailCancellation?.Cancel();
        _detailCancellation?.Dispose();
        _detailCancellation = null;
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

public sealed class ProjectRequirementSurveyItemViewModel : ObservableObject
{
    private AgentRequirementSurveyDetails? _details;
    private bool _isPlanBusy;
    private string? _planError;

    public ProjectRequirementSurveyItemViewModel(AgentRequirementSurvey survey)
    {
        Survey = survey;
        var answers = survey.Submission?.Answers.ToDictionary(
            value => value.QuestionId, StringComparer.Ordinal) ?? [];
        Questions = survey.Draft.Questions.Select(question =>
        {
            var submitted = answers.GetValueOrDefault(question.Id);
            var selected = submitted?.SelectedOptionIds ?? [];
            var selectedSet = selected.ToHashSet(StringComparer.Ordinal);
            var answer = question.Kind switch
            {
                AgentRequirementQuestionKind.Text => submitted?.TextValue ?? string.Empty,
                AgentRequirementQuestionKind.Boolean => submitted?.BooleanValue switch
                {
                    true => "是",
                    false => "否",
                    null => string.Empty,
                },
                _ => string.Join("、", question.Options
                    .Where(option => selectedSet.Contains(option.Id))
                    .Select(option => option.Label)),
            };
            return new ProjectRequirementSurveyQuestionViewModel(
                question.Prompt,
                question.Kind switch
                {
                    AgentRequirementQuestionKind.Text => "文本",
                    AgentRequirementQuestionKind.SingleChoice => "单选",
                    AgentRequirementQuestionKind.MultipleChoice => "多选",
                    AgentRequirementQuestionKind.Boolean => "是/否",
                    _ => string.Empty,
                },
                question.IsRequired,
                string.IsNullOrWhiteSpace(answer) ? "尚未填写" : answer);
        }).ToArray();
    }

    public AgentRequirementSurvey Survey { get; }
    public string Id => Survey.Id;
    public string Title => Survey.Draft.Title;
    public string Purpose => Survey.Draft.Purpose;
    public string StatusText => IsPending ? "待填写" : "已提交";
    public string QuestionCountText => $"{Survey.Draft.Questions.Count} 个问题";
    public string CreatedAtText => DateTimeOffset.FromUnixTimeMilliseconds(Survey.CreatedAtUnixMs)
        .ToLocalTime().ToString("g");
    public bool IsPending => Survey.Status == AgentRequirementSurveyStatus.Pending;
    public bool IsSubmitted => !IsPending;
    public bool HasResolution => Survey.ResolvedAtUnixMs is not null;
    public bool HasDetailedResolution => Survey.Resolution is not null;
    public bool IsAwaitingResolution => false;
    public bool CanSubmit => IsPending;
    public string PermissionText => IsPending
        ? "Human 可填写并提交；Agent 不可代替 Human 作答。"
        : "Human 答案已锁定并转为只读；对应本地任务已恢复执行。";
    public string SubmissionNotes => string.IsNullOrWhiteSpace(Survey.Submission?.Notes)
        ? "无补充备注" : Survey.Submission.Notes;
    public string ResolutionSummary => Survey.Resolution?.Summary ?? "答案已提交，本地任务已恢复执行。";
    public string SolutionMarkdown => Survey.Resolution?.SolutionMarkdown ?? string.Empty;
    public string RisksAndOpenQuestions => string.IsNullOrWhiteSpace(
        Survey.Resolution?.RisksAndOpenQuestions) ? "无" : Survey.Resolution!.RisksAndOpenQuestions;
    public string RelatedMaterials => string.IsNullOrWhiteSpace(Survey.Resolution?.RelatedMaterials)
        ? "无" : Survey.Resolution!.RelatedMaterials;
    public IReadOnlyList<ProjectRequirementSurveyQuestionViewModel> Questions { get; }
    public IReadOnlyList<AgentRequirementExecutionStep> ExecutionSteps =>
        Survey.Resolution?.ExecutionSteps ?? [];

    public AgentRequirementSurveyDetails? Details => _details;
    public AgentRequirementPlan? Plan => _details?.Plan;
    public bool IsPlanBusy => _isPlanBusy;
    public bool HasPlanError => !string.IsNullOrWhiteSpace(_planError);
    public string PlanError => _planError ?? string.Empty;
    public bool HasPlan => Plan is not null;
    public bool IsPlanConfirmed => Plan?.ConfirmedAtUnixMs is not null;
    public bool IsPlanPendingConfirmation => HasPlan && !IsPlanConfirmed;
    public string PlanVersionText => Plan is null ? string.Empty : $"方案版本 {Plan.Revision}";
    public string PlanMarkdown => Plan?.Markdown ?? string.Empty;
    public string LegacyPlanMarkdown => !HasPlan &&
        _details?.SurveyFollowUp.State == AgentRequirementPlanFollowUpState.Succeeded
            ? _details.SurveyFollowUp.Markdown ?? string.Empty
            : string.Empty;
    public bool HasLegacyPlanCandidate => !string.IsNullOrWhiteSpace(LegacyPlanMarkdown);
    public bool CanAdoptLegacyPlan => HasLegacyPlanCandidate && !IsPlanBusy;
    public bool PlanGenerationSucceeded => _details?.PlanFollowUp?.State is
        AgentRequirementPlanFollowUpState.Succeeded or
        AgentRequirementPlanFollowUpState.SucceededWithoutReply;
    public bool CanConfirmPlan => IsPlanPendingConfirmation && PlanGenerationSucceeded && !IsPlanBusy;
    public string PlanStatusText => IsPlanConfirmed
        ? "方案已确认"
        : IsPlanPendingConfirmation
            ? "等待 Human 确认"
            : FollowUpText(_details?.SurveyFollowUp.State);
    public string PlanFollowUpText => FollowUpText(
        _details?.PlanFollowUp?.State ?? _details?.SurveyFollowUp.State);
    public string PlanBoundaryText => IsPlanConfirmed
        ? "方案已确认，本次不会自动执行任何任务。需要评审、生成文档或开始实施时，请返回原对话安排下一步。"
        : "确认仅认可当前方案，不会自动开始任务。后续创建的任务将固定引用此版本；修改方案后需要重新确认。";

    public void SetDetails(AgentRequirementSurveyDetails details)
    {
        _details = details;
        _isPlanBusy = false;
        _planError = null;
        NotifyPlanChanged();
    }

    public void SetPlanBusy(bool value, string? error)
    {
        _isPlanBusy = value;
        _planError = error;
        NotifyPlanChanged();
    }

    private void NotifyPlanChanged()
    {
        foreach (var property in new[]
        {
            nameof(Details), nameof(Plan), nameof(IsPlanBusy), nameof(HasPlanError),
            nameof(PlanError), nameof(HasPlan), nameof(IsPlanConfirmed),
            nameof(IsPlanPendingConfirmation), nameof(PlanVersionText), nameof(PlanMarkdown),
            nameof(LegacyPlanMarkdown), nameof(HasLegacyPlanCandidate),
            nameof(CanAdoptLegacyPlan),
            nameof(PlanGenerationSucceeded), nameof(CanConfirmPlan), nameof(PlanStatusText),
            nameof(PlanFollowUpText), nameof(PlanBoundaryText),
        }) OnPropertyChanged(property);
    }

    private static string FollowUpText(AgentRequirementPlanFollowUpState? state) => state switch
    {
        AgentRequirementPlanFollowUpState.AwaitingAnswers => "请先填写并提交调研。",
        AgentRequirementPlanFollowUpState.Processing => "正在根据你的回答整理需求与后续方案…",
        AgentRequirementPlanFollowUpState.Succeeded => "方案生成已完成。",
        AgentRequirementPlanFollowUpState.SucceededWithoutReply => "后续处理已结束，暂无方案回复。",
        AgentRequirementPlanFollowUpState.WaitingForInput => "还需要补充信息，请查看待填写调研或原对话。",
        AgentRequirementPlanFollowUpState.NeedsReview => "后续处理已暂停，请在原对话中查看需要处理的事项。",
        AgentRequirementPlanFollowUpState.Failed => "后续处理未完成，请在原对话中查看原因并重试。",
        AgentRequirementPlanFollowUpState.Cancelled => "后续处理已取消，你提交的回答仍已保留。",
        _ => "正在读取方案状态…",
    };
}

public sealed record ProjectRequirementSurveyQuestionViewModel(
    string Prompt,
    string KindText,
    bool IsRequired,
    string AnswerText)
{
    public string RequirementText => IsRequired ? "必填" : "选填";
}
