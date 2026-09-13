using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.Pet;

public sealed partial class PetOverlayViewModel : ObservableObject, IDisposable
{
    private readonly ILocalAgentPetActivityService _activityService;
    private readonly IAskUserPromptService _askUser;
    private readonly ILocalAgentToolApprovalService _toolApproval;
    private readonly ILocalAgentRunControlService _runControl;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _stateGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;

    public PetOverlayViewModel(
        ILocalAgentPetActivityService activityService,
        IAskUserPromptService askUser,
        ILocalAgentToolApprovalService toolApproval,
        ILocalAgentRunControlService runControl,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher)
    {
        _activityService = activityService;
        _askUser = askUser;
        _toolApproval = toolApproval;
        _runControl = runControl;
        _localization = localization;
        _dispatcher = dispatcher;
        _activityService.Changed += OnActivitiesChanged;
        _localization.PropertyChanged += OnLocalizationChanged;
    }

    public ObservableCollection<PetActivityItemViewModel> Activities { get; } = [];

    public LocalizationViewModel Localization => _localization;

    public bool HasActivities => Activities.Count > 0;

    public bool HasSelectedActivity => SelectedActivity is not null;

    public bool HasActivePrompt => ActivePrompt is not null;

    public bool HasActiveToolApproval => ActiveToolApproval is not null;

    public bool HasActiveRunControl => ActiveRunControl is not null;

    public bool CanCancelSelected => SelectedActivity?.CanCancel == true;

    public bool CanIgnoreSelected => SelectedActivity is not null;

    public string InboxTitle => _localization.Text("宠物消息", "Pet inbox");

    public string EmptyMessage => _localization.Text("暂时没有需要关注的消息", "Nothing needs your attention right now");

    public string RefreshLabel => _localization.Text("刷新", "Refresh");

    public string IgnoreLabel => _localization.Text("忽略", "Ignore");

    public string HandledLabel => _localization.Text("已处理", "Handled");

    public string CancelTaskLabel => _localization.Text("取消任务", "Cancel task");

    public string CloseLabel => _localization.Text("收起", "Close");

    public string LocalApprovalRequiredLabel => _localization.LocalApprovalRequired;

    public string DenyLabel => _localization.Deny;

    public string AllowOnceLabel => _localization.AllowOnce;

    public string AllowForSessionLabel => _localization.AllowForSession;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelectedActivity))]
    [NotifyPropertyChangedFor(nameof(CanCancelSelected))]
    [NotifyPropertyChangedFor(nameof(CanIgnoreSelected))]
    private PetActivityItemViewModel? _selectedActivity;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasActivePrompt))]
    private AskUserPromptViewModel? _activePrompt;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasActiveToolApproval))]
    private LocalAgentToolApprovalViewModel? _activeToolApproval;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasActiveRunControl))]
    private LocalAgentRunControlViewModel? _activeRunControl;

    [ObservableProperty]
    private PetAnimationState _animationState = PetAnimationState.Idle;

    [ObservableProperty]
    private int _activeWorkCount;

    [ObservableProperty]
    private int _attentionCount;

    [ObservableProperty]
    private bool _isExpanded;

    [ObservableProperty]
    private bool _isDetailOpen;

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    public async Task StartAsync(CancellationToken cancellationToken = default)
    {
        StopSession();
        await ResetPresentationAsync(CancellationToken.None).ConfigureAwait(false);
        var session = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _sessionCancellation = session;
        await RefreshAsync(session.Token).ConfigureAwait(false);
    }

    public void Stop()
    {
        StopSession();
        _ = ResetPresentationAsync(CancellationToken.None);
    }

    private void StopSession()
    {
        var cancellation = Interlocked.Exchange(ref _sessionCancellation, null);
        cancellation?.Cancel();
        cancellation?.Dispose();
    }

    private Task ResetPresentationAsync(CancellationToken cancellationToken) =>
        _dispatcher.InvokeAsync(() =>
        {
            Activities.Clear();
            SelectedActivity = null;
            ActivePrompt = null;
            ActiveToolApproval = null;
            ActiveRunControl = null;
            IsExpanded = false;
            IsDetailOpen = false;
            AnimationState = PetAnimationState.Idle;
            ActiveWorkCount = 0;
            AttentionCount = 0;
            ErrorMessage = null;
            ActionMessage = null;
            OnPropertyChanged(nameof(HasActivities));
        }, cancellationToken);

    public void ToggleExpanded()
    {
        IsExpanded = !IsExpanded;
        if (!IsExpanded)
        {
            CloseDetail();
        }
    }

    public void CloseDetail()
    {
        IsDetailOpen = false;
        SelectedActivity = null;
        ActivePrompt = null;
        ActiveToolApproval = null;
        ActiveRunControl = null;
        ErrorMessage = null;
    }

    public async Task RefreshAsync(CancellationToken cancellationToken = default)
    {
        await RunBusyAsync(async token =>
        {
            await _stateGate.WaitAsync(token).ConfigureAwait(false);
            try
            {
                var activities = await _activityService.FetchAsync(token).ConfigureAwait(false);
                await PublishAsync(activities, token).ConfigureAwait(false);
            }
            finally
            {
                _stateGate.Release();
            }
        }, cancellationToken).ConfigureAwait(false);
    }

    public async Task SelectAsync(
        PetActivityItemViewModel item,
        CancellationToken cancellationToken = default)
    {
        SelectedActivity = item;
        IsDetailOpen = true;
        ActivePrompt = null;
        ActiveToolApproval = null;
        ActiveRunControl = null;
        ErrorMessage = null;
        if (item.Activity.Route.ConversationId is not { Length: > 0 } conversationId)
        {
            return;
        }

        if (item.Activity.Source == PetActivitySource.AskUserPrompt)
        {
            await RunBusyAsync(async token =>
            {
                var prompts = await _askUser.FetchPromptsAsync(
                    conversationId, cancellationToken: token).ConfigureAwait(false);
                var matches = prompts.Where(value =>
                    string.Equals(value.Id, item.Activity.Route.PromptId, StringComparison.Ordinal))
                    .ToArray();
                if (matches.Length != 1 || !matches[0].IsPending)
                    throw new InvalidOperationException(_localization.Text(
                        "这个提问已经处理或失效，正在刷新消息。",
                        "This prompt was already handled or expired. Refreshing the inbox."));

                var promptViewModel = new AskUserPromptViewModel(
                    matches[0], _askUser, CompleteAuthoritativeActionAsync, _localization);
                await _dispatcher.InvokeAsync(() =>
                {
                    if (SelectedActivity?.Id == item.Id) ActivePrompt = promptViewModel;
                }, token).ConfigureAwait(false);
            }, cancellationToken).ConfigureAwait(false);
            return;
        }

        if (item.Activity.Source == PetActivitySource.LocalAgentToolApproval
            && item.Activity.Route.InvocationId is { Length: > 0 } invocationId)
        {
            await RunBusyAsync(async token =>
            {
                var approvals = await _toolApproval.FetchPendingAsync(conversationId, token)
                    .ConfigureAwait(false);
                var matches = approvals.Where(value =>
                    value.InvocationId == invocationId
                    && value.RunId == item.Activity.Route.RunId).ToArray();
                if (matches.Length != 1)
                    throw new InvalidOperationException(_localization.Text(
                        "这个工具授权已经处理或失效，正在刷新消息。",
                        "This tool approval was already handled or expired. Refreshing the inbox."));
                var approvalViewModel = new LocalAgentToolApprovalViewModel(
                    matches[0], _toolApproval, CompleteAuthoritativeActionAsync, _localization);
                await _dispatcher.InvokeAsync(() =>
                {
                    if (SelectedActivity?.Id == item.Id)
                        ActiveToolApproval = approvalViewModel;
                }, token).ConfigureAwait(false);
            }, cancellationToken).ConfigureAwait(false);
            return;
        }

        if (item.Activity.Route.RunId is { Length: > 0 } runId && !item.IsTerminal)
        {
            await RunBusyAsync(async token =>
            {
                var controls = await _runControl.FetchRunControlsAsync(conversationId, token)
                    .ConfigureAwait(false);
                var matches = controls.Where(value => value.RunId == runId).ToArray();
                if (matches.Length != 1)
                    throw new InvalidOperationException(_localization.Text(
                        "这个运行状态已经变化，正在刷新消息。",
                        "This run state changed. Refreshing the inbox."));
                var controlViewModel = new LocalAgentRunControlViewModel(
                    matches[0], _runControl, CompleteAuthoritativeActionAsync, _localization);
                await _dispatcher.InvokeAsync(() =>
                {
                    if (SelectedActivity?.Id == item.Id) ActiveRunControl = controlViewModel;
                }, token).ConfigureAwait(false);
            }, cancellationToken).ConfigureAwait(false);
        }
    }

    public Task IgnoreAsync(
        PetActivityItemViewModel item,
        CancellationToken cancellationToken = default) =>
        ApplyDispositionAsync(item.Activity, PetActivityDisposition.Ignored, cancellationToken);

    public Task MarkHandledAsync(
        PetActivityItemViewModel item,
        CancellationToken cancellationToken = default) =>
        ApplyDispositionAsync(item.Activity, PetActivityDisposition.Handled, cancellationToken);

    public async Task CancelSelectedAsync(CancellationToken cancellationToken = default)
    {
        if (SelectedActivity is not { CanCancel: true } selected)
        {
            return;
        }

        await RunBusyAsync(async token =>
        {
            var route = selected.Activity.Route;
            if (route.ConversationId is not { Length: > 0 } conversation
                || route.RunId is not { Length: > 0 } runId)
                throw new InvalidOperationException(_localization.Text(
                    "这个活动缺少精确的会话或 Run 标识，无法取消。",
                    "This activity is missing its exact conversation or Run identity."));
            await _runControl.CancelRunAsync(runId, conversation, token).ConfigureAwait(false);

            await _dispatcher.InvokeAsync(() => ActionMessage = _localization.Text(
                "已发送取消请求，本地运行状态确认后会自动更新。",
                "Cancellation requested. The local run status will update after confirmation."), token)
                .ConfigureAwait(false);
            await RefreshAsync(token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    public void Dispose()
    {
        Stop();
        _activityService.Changed -= OnActivitiesChanged;
        _localization.PropertyChanged -= OnLocalizationChanged;
    }

    private async Task ApplyDispositionAsync(
        PetActivity activity,
        PetActivityDisposition disposition,
        CancellationToken cancellationToken)
    {
        await RunBusyAsync(async token =>
        {
            await _stateGate.WaitAsync(token).ConfigureAwait(false);
            try
            {
                await _activityService.SuppressAsync(activity, disposition, token)
                    .ConfigureAwait(false);
                var activities = await _activityService.FetchAsync(token).ConfigureAwait(false);
                await PublishAsync(activities, token).ConfigureAwait(false);
            }
            finally
            {
                _stateGate.Release();
            }

            await _dispatcher.InvokeAsync(CloseDetail, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task CompleteAuthoritativeActionAsync()
    {
        await RefreshAsync(CancellationToken.None).ConfigureAwait(false);
    }

    private Task PublishAsync(
        IReadOnlyList<PetActivity> visible,
        CancellationToken cancellationToken)
    {
        var ordered = visible.OrderByDescending(activity => activity.PresentationPriority)
            .ThenByDescending(activity => activity.UpdatedAt)
            .ThenBy(activity => activity.Id, StringComparer.Ordinal)
            .ToArray();
        var primary = ordered.FirstOrDefault();
        var presentation = primary is null
            ? PetPresentation.Idle
            : new PetPresentation(
                primary.AnimationState,
                primary,
                ordered.Count(activity => activity.Kind is
                    PetActivityKind.Working),
                ordered.Count(activity => activity.RequiresAttention));
        var selectedId = SelectedActivity?.Activity.Id;
        return _dispatcher.InvokeAsync(() =>
        {
            Activities.Clear();
            foreach (var activity in ordered)
            {
                Activities.Add(new PetActivityItemViewModel(activity, _localization));
            }

            SelectedActivity = selectedId is null
                ? null
                : Activities.FirstOrDefault(value => value.Activity.Id == selectedId);
            if (SelectedActivity is null)
            {
                IsDetailOpen = false;
                ActivePrompt = null;
                ActiveToolApproval = null;
                ActiveRunControl = null;
            }

            AnimationState = presentation.AnimationState;
            ActiveWorkCount = presentation.ActiveWorkCount;
            AttentionCount = presentation.AttentionCount;
            OnPropertyChanged(nameof(HasActivities));
        }, cancellationToken);
    }

    private void OnActivitiesChanged(object? sender, EventArgs args)
    {
        var session = _sessionCancellation;
        if (session is null || session.IsCancellationRequested) return;
        _ = RefreshAsync(session.Token);
    }

    private async Task RunBusyAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        await _dispatcher.InvokeAsync(() =>
        {
            IsBusy = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);
        try
        {
            await operation(cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false);
        }
    }

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        foreach (var activity in Activities)
        {
            activity.ApplyLabels(_localization);
        }

        OnPropertyChanged(string.Empty);
    }
}

public sealed partial class PetActivityItemViewModel : ObservableObject
{
    public PetActivityItemViewModel(PetActivity activity, LocalizationViewModel localization)
    {
        Activity = activity;
        ApplyLabels(localization);
    }

    public PetActivity Activity { get; }

    public string Id => Activity.Id;

    public string Title => Activity.Title;

    public string Detail => Activity.Detail ?? string.Empty;

    public bool HasDetail => !string.IsNullOrWhiteSpace(Activity.Detail);

    public bool RequiresAttention => Activity.RequiresAttention;

    public bool IsTerminal => Activity.Kind is PetActivityKind.Succeeded or
        PetActivityKind.Failed or PetActivityKind.Cancelled;

    public bool CanCancel => (Activity.Kind is
        PetActivityKind.Working or PetActivityKind.NeedsReview or
        PetActivityKind.WaitingForApproval)
        && !string.IsNullOrWhiteSpace(Activity.Route.ConversationId)
        && !string.IsNullOrWhiteSpace(Activity.Route.RunId);

    [ObservableProperty]
    private string _statusLabel = string.Empty;

    [ObservableProperty]
    private string _sourceLabel = string.Empty;

    [ObservableProperty]
    private string _timeLabel = string.Empty;

    public void ApplyLabels(LocalizationViewModel localization)
    {
        StatusLabel = Activity.Kind switch
        {
            PetActivityKind.Working => localization.Text("执行中", "Running"),
            PetActivityKind.WaitingForApproval => localization.Text("等待审批", "Waiting for approval"),
            PetActivityKind.WaitingForUser => localization.Text("等待输入", "Waiting for input"),
            PetActivityKind.NeedsReview => localization.Text("需要人工复核", "Needs review"),
            PetActivityKind.Succeeded => localization.Text("已完成", "Completed"),
            PetActivityKind.Failed => localization.Text("失败", "Failed"),
            PetActivityKind.Cancelled => localization.Text("已取消", "Cancelled"),
            _ => Activity.Kind.ToString(),
        };
        SourceLabel = Activity.Source switch
        {
            PetActivitySource.LocalAgentToolApproval =>
                localization.Text("AI 工具授权", "AI tool approval"),
            PetActivitySource.AskUserPrompt => "Ask User",
            PetActivitySource.Chat => localization.Text("聊天", "Chat"),
            PetActivitySource.TaskRunner => localization.Text("任务执行", "Task run"),
            _ => Activity.Source.ToString(),
        };
        TimeLabel = Activity.UpdatedAt.ToLocalTime().ToString("MM-dd HH:mm");
    }
}
