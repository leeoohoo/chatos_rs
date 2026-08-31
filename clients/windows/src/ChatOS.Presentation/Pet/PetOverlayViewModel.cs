using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.Pet;

public sealed partial class PetOverlayViewModel : ObservableObject, IDisposable
{
    private readonly PetActivityCoordinator _coordinator;
    private readonly IRealtimeClient _realtime;
    private readonly IConversationCommandService _conversationCommands;
    private readonly IMessageTaskGraphService _taskGraph;
    private readonly IAskUserPromptService _askUser;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly PetStateReducer _reducer = new();
    private readonly SemaphoreSlim _stateGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;
    private Task? _realtimeTask;
    private long _generation;

    public PetOverlayViewModel(
        PetActivityCoordinator coordinator,
        IRealtimeClient realtime,
        IConversationCommandService conversationCommands,
        IMessageTaskGraphService taskGraph,
        IAskUserPromptService askUser,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher)
    {
        _coordinator = coordinator;
        _realtime = realtime;
        _conversationCommands = conversationCommands;
        _taskGraph = taskGraph;
        _askUser = askUser;
        _localization = localization;
        _dispatcher = dispatcher;
        _localization.PropertyChanged += OnLocalizationChanged;
    }

    public ObservableCollection<PetActivityItemViewModel> Activities { get; } = [];

    public LocalizationViewModel Localization => _localization;

    public bool HasActivities => Activities.Count > 0;

    public bool HasSelectedActivity => SelectedActivity is not null;

    public bool HasActivePrompt => ActivePrompt is not null;

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
        Stop();
        var session = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _sessionCancellation = session;
        var generation = Interlocked.Increment(ref _generation);
        await RefreshAsync(session.Token).ConfigureAwait(false);
        if (generation != _generation || session.IsCancellationRequested)
        {
            return;
        }

        _realtimeTask = ObserveRealtimeAsync(generation, session.Token);
    }

    public void Stop()
    {
        Interlocked.Increment(ref _generation);
        var cancellation = Interlocked.Exchange(ref _sessionCancellation, null);
        cancellation?.Cancel();
        cancellation?.Dispose();
        _realtimeTask = null;
        _reducer.Clear();
        _ = _dispatcher.InvokeAsync(() =>
        {
            Activities.Clear();
            SelectedActivity = null;
            ActivePrompt = null;
            IsExpanded = false;
            IsDetailOpen = false;
            AnimationState = PetAnimationState.Idle;
            ActiveWorkCount = 0;
            AttentionCount = 0;
            ErrorMessage = null;
            ActionMessage = null;
            OnPropertyChanged(nameof(HasActivities));
        });
    }

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
        ErrorMessage = null;
    }

    public async Task RefreshAsync(CancellationToken cancellationToken = default)
    {
        await RunBusyAsync(async token =>
        {
            await _stateGate.WaitAsync(token).ConfigureAwait(false);
            try
            {
                await _coordinator.ReconcileAsync(_reducer, cancellationToken: token)
                    .ConfigureAwait(false);
                await PublishAsync(token).ConfigureAwait(false);
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
        ErrorMessage = null;
        if (item.Activity.Source != PetActivitySource.AskUserPrompt ||
            item.Activity.Route.ConversationId is not { Length: > 0 } conversationId)
        {
            return;
        }

        await RunBusyAsync(async token =>
        {
            var prompts = await _askUser.FetchPromptsAsync(conversationId, cancellationToken: token)
                .ConfigureAwait(false);
            var prompt = prompts.FirstOrDefault(value =>
                string.Equals(value.Id, item.Activity.Route.PromptId, StringComparison.Ordinal) ||
                string.Equals(value.TurnId, item.Activity.Route.TurnId, StringComparison.Ordinal));
            if (prompt is null || !prompt.IsPending)
            {
                throw new InvalidOperationException(_localization.Text(
                    "这个提问已经处理或失效，正在刷新消息。",
                    "This prompt was already handled or expired. Refreshing the inbox."));
            }

            var promptViewModel = new AskUserPromptViewModel(
                prompt,
                _askUser,
                () => CompletePromptAsync(item.Activity),
                _localization);
            await _dispatcher.InvokeAsync(() => ActivePrompt = promptViewModel, token)
                .ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
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
            var activity = selected.Activity;
            if (activity.Route.MessageId is { Length: > 0 } messageId &&
                activity.Route.TaskId is { Length: > 0 } taskId &&
                activity.Route.ConversationId is { Length: > 0 } conversationId)
            {
                await _taskGraph.CancelTaskAsync(
                    messageId,
                    taskId,
                    new MessageTaskLookup(conversationId, activity.Route.TurnId, activity.Route.MessageId),
                    _localization.Text("用户从桌面宠物取消任务", "Cancelled from the desktop pet"),
                    token).ConfigureAwait(false);
            }
            else if (activity.Route.ConversationId is { Length: > 0 } conversation)
            {
                await _conversationCommands.StopTurnAsync(
                    conversation,
                    activity.Route.TurnId,
                    token).ConfigureAwait(false);
            }
            else
            {
                throw new InvalidOperationException(_localization.Text(
                    "这个运行中事件缺少可取消的任务标识。",
                    "This running event does not include a cancellable task identity."));
            }

            await _dispatcher.InvokeAsync(() => ActionMessage = _localization.Text(
                "已发送取消请求，状态会在服务端确认后更新。",
                "Cancellation requested. The status will update after server confirmation."), token)
                .ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    public void Dispose()
    {
        Stop();
        _localization.PropertyChanged -= OnLocalizationChanged;
        _stateGate.Dispose();
    }

    private async Task ObserveRealtimeAsync(long generation, CancellationToken cancellationToken)
    {
        try
        {
            await foreach (var activityEvent in _realtime.StreamPetActivitiesAsync(cancellationToken)
                .ConfigureAwait(false))
            {
                if (generation != _generation)
                {
                    return;
                }

                if (activityEvent is PetActivityEvent.Reconcile)
                {
                    await RefreshAsync(cancellationToken).ConfigureAwait(false);
                    continue;
                }

                await _stateGate.WaitAsync(cancellationToken).ConfigureAwait(false);
                try
                {
                    if (await _coordinator.ApplyRealtimeAsync(
                            _reducer,
                            activityEvent,
                            cancellationToken: cancellationToken).ConfigureAwait(false))
                    {
                        await PublishAsync(cancellationToken).ConfigureAwait(false);
                    }
                }
                finally
                {
                    _stateGate.Release();
                }
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
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
                await _coordinator.ApplyDispositionAsync(
                    _reducer,
                    activity,
                    disposition,
                    cancellationToken: token).ConfigureAwait(false);
                await PublishAsync(token).ConfigureAwait(false);
            }
            finally
            {
                _stateGate.Release();
            }

            await _dispatcher.InvokeAsync(CloseDetail, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task CompletePromptAsync(PetActivity activity)
    {
        await ApplyDispositionAsync(activity, PetActivityDisposition.Handled, CancellationToken.None)
            .ConfigureAwait(false);
    }

    private Task PublishAsync(CancellationToken cancellationToken)
    {
        var visible = _reducer.VisibleActivities();
        var presentation = _reducer.Presentation();
        var selectedId = SelectedActivity?.Activity.Id;
        return _dispatcher.InvokeAsync(() =>
        {
            Activities.Clear();
            foreach (var activity in visible)
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
            }

            AnimationState = presentation.AnimationState;
            ActiveWorkCount = presentation.ActiveWorkCount;
            AttentionCount = presentation.AttentionCount;
            OnPropertyChanged(nameof(HasActivities));
        }, cancellationToken);
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
        PetActivityKind.Failed or PetActivityKind.Blocked or PetActivityKind.Cancelled;

    public bool CanCancel => (Activity.Kind is PetActivityKind.Working or PetActivityKind.Reviewing) &&
        (!string.IsNullOrWhiteSpace(Activity.Route.ConversationId) ||
         (!string.IsNullOrWhiteSpace(Activity.Route.MessageId) &&
          !string.IsNullOrWhiteSpace(Activity.Route.TaskId)));

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
            PetActivityKind.Reviewing => localization.Text("检查中", "Reviewing"),
            PetActivityKind.WaitingForApproval => localization.Text("等待审批", "Waiting for approval"),
            PetActivityKind.WaitingForUser => localization.Text("等待输入", "Waiting for input"),
            PetActivityKind.Succeeded => localization.Text("已完成", "Completed"),
            PetActivityKind.Failed => localization.Text("失败", "Failed"),
            PetActivityKind.Blocked => localization.Text("已阻塞", "Blocked"),
            PetActivityKind.Cancelled => localization.Text("已取消", "Cancelled"),
            _ => Activity.Kind.ToString(),
        };
        SourceLabel = Activity.Source switch
        {
            PetActivitySource.LocalApproval => localization.Text("本机审批", "Local approval"),
            PetActivitySource.AskUserPrompt => "Ask User",
            PetActivitySource.Chat => localization.Text("聊天", "Chat"),
            PetActivitySource.TaskBoard => localization.Text("任务", "Task"),
            PetActivitySource.TaskRunner => localization.Text("任务执行", "Task run"),
            PetActivitySource.ProjectExecution => localization.Text("项目执行", "Project run"),
            _ => Activity.Source.ToString(),
        };
        TimeLabel = Activity.UpdatedAt.ToLocalTime().ToString("MM-dd HH:mm");
    }
}
