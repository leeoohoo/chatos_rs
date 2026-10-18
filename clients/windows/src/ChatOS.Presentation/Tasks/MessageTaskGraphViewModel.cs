using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Tasks;

public sealed partial class MessageTaskGraphViewModel : ObservableObject, IDisposable
{
    private const int EmptyGraphRetryLimit = 3;
    private static readonly TimeSpan EmptyGraphRetryInterval = TimeSpan.FromMilliseconds(600);
    private static readonly TimeSpan ActiveGraphPollingInterval = TimeSpan.FromSeconds(2);
    private readonly IMessageTaskGraphService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly TimeProvider _timeProvider;
    private readonly IRealtimeClient? _realtimeClient;
    private CancellationTokenSource? _sessionCancellation;
    private CancellationTokenSource? _loadCancellation;
    private CancellationTokenSource? _pollingCancellation;
    private bool _isRefreshing;
    private string? _pollingErrorMessage;
    private long _generation;
    private bool _disposed;

    public MessageTaskGraphViewModel(
        IMessageTaskGraphService service,
        IUiDispatcher dispatcher,
        TimeProvider? timeProvider = null,
        IRealtimeClient? realtimeClient = null)
    {
        _service = service;
        _dispatcher = dispatcher;
        _timeProvider = timeProvider ?? TimeProvider.System;
        _realtimeClient = realtimeClient;
        Nodes.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(IsEmpty));
            OnPropertyChanged(nameof(ShowsEmptyGraph));
            OnPropertyChanged(nameof(ShowsGraphLoadError));
        };
        RunEvents.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasRunEvents));
            OnPropertyChanged(nameof(EventsCountLabel));
        };
    }

    public ObservableCollection<MessageTaskGraphNodeItemViewModel> Nodes { get; } = [];

    public ObservableCollection<MessageTaskRunEvent> RunEvents { get; } = [];

    public bool IsEmpty => Nodes.Count == 0;

    public bool ShowsEmptyGraph => IsOpen && IsEmpty && !IsLoading && ErrorMessage is null;

    public bool ShowsGraphLoadError => IsOpen && IsEmpty && !IsLoading && ErrorMessage is not null;

    public bool HasRunEvents => RunEvents.Count > 0;

    public string EventsCountLabel => $"{RunEvents.Count} / {EventsTotal}";

    public bool HasTask => SelectedTask is not null;

    public bool HasFrozenModel => !string.IsNullOrWhiteSpace(SelectedTask?.DefaultModelConfigId);

    public string FrozenModelDisplay =>
        SelectedTask?.DefaultModelConfig?.DisplayName is { Length: > 0 } displayName
            ? displayName
            : SelectedTask?.DefaultModelConfigId ?? string.Empty;

    public string FrozenThinkingLevelDisplay => SelectedTask?.ThinkingLevel ?? string.Empty;

    public bool HasFrozenThinkingLevel =>
        !string.IsNullOrWhiteSpace(SelectedTask?.ThinkingLevel);

    public string FrozenModelRevisionDisplay => SelectedTask?.DefaultModelConfigRevision ?? string.Empty;

    public bool HasRun => RunDetail is not null;

    public MessageTaskRequirementPlan? RequirementPlan => SelectedTask?.RequirementPlan;

    public bool HasRequirementPlan => RequirementPlan is not null;

    public string RequirementPlanRevisionLabel => RequirementPlan is { } plan
        ? plan.Snapshot.Revision.ToString(System.Globalization.CultureInfo.InvariantCulture)
        : string.Empty;

    public MessageTaskAcceptanceReport? AcceptanceReport => RunDetail?.Run.AcceptanceReport;

    public bool HasAcceptanceReport => AcceptanceReport is not null;

    public bool ShowsMissingAcceptanceReport =>
        SelectedTask?.AcceptanceContract is not null &&
        RunDetail?.Run.Status?.Trim().ToLowerInvariant() == "succeeded" &&
        AcceptanceReport is null;

    public string AcceptanceCharacterCountLabel => AcceptanceReport is { } report &&
        SelectedTask?.AcceptanceContract is { } contract
        ? $"{report.FinalResponseChars} / {contract.EffectiveFinalResponseMaxChars}"
        : string.Empty;

    public IReadOnlyList<MessageTaskReference> Prerequisites
    {
        get
        {
            if (SelectedTask is not { } task) return [];
            var references = task.PrerequisiteTasks.ToDictionary(value => value.Id, StringComparer.Ordinal);
            var seen = new HashSet<string>(StringComparer.Ordinal);
            return task.PrerequisiteTaskIds.Concat(task.PrerequisiteTasks.Select(value => value.Id))
                .Where(seen.Add)
                .Select(id => references.GetValueOrDefault(id) ?? new MessageTaskReference(id, null, null))
                .ToArray();
        }
    }

    public bool HasPrerequisites => Prerequisites.Count > 0;

    private bool HasActiveSession => !_disposed && IsOpen &&
        _sessionCancellation is { IsCancellationRequested: false };

    public bool CanCancel => HasActiveSession && !IsLoading && !IsApplyingAction &&
        SelectedTask is { } task && !IsTerminal(task.Status);

    public bool CanRetry => HasActiveSession && !IsLoading && !IsApplyingAction &&
        RunDetail is not null && SelectedTask is { Status: var status } && IsRetryable(status);

    public bool CanLoadMoreEvents => HasActiveSession && !IsLoading && !IsLoadingMoreEvents &&
        !IsApplyingAction && EventsHasMore && RunDetail is not null;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCancel), nameof(CanRetry))]
    [NotifyPropertyChangedFor(nameof(ShowsEmptyGraph), nameof(ShowsGraphLoadError))]
    private bool _isOpen;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCancel), nameof(CanRetry))]
    [NotifyPropertyChangedFor(nameof(ShowsEmptyGraph), nameof(ShowsGraphLoadError))]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isAwaitingInitialGraph;

    [ObservableProperty]
    private bool _isUsingPreviousGraph;

    [ObservableProperty]
    private bool _isLoadingMoreEvents;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCancel), nameof(CanRetry))]
    private bool _isApplyingAction;

    [ObservableProperty]
    private string? _messageId;

    [ObservableProperty]
    private MessageTaskLookup? _lookup;

    [ObservableProperty]
    private string? _requestedTaskId;

    [ObservableProperty]
    private string? _requestedRunId;

    [ObservableProperty]
    private MessageTaskGraphNodeItemViewModel? _selectedNode;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasTask))]
    [NotifyPropertyChangedFor(nameof(CanCancel))]
    [NotifyPropertyChangedFor(nameof(CanRetry))]
    [NotifyPropertyChangedFor(nameof(Prerequisites))]
    [NotifyPropertyChangedFor(nameof(HasPrerequisites))]
    [NotifyPropertyChangedFor(nameof(ShowsMissingAcceptanceReport))]
    [NotifyPropertyChangedFor(nameof(AcceptanceCharacterCountLabel))]
    [NotifyPropertyChangedFor(nameof(RequirementPlan))]
    [NotifyPropertyChangedFor(nameof(HasRequirementPlan))]
    [NotifyPropertyChangedFor(nameof(RequirementPlanRevisionLabel))]
    [NotifyPropertyChangedFor(nameof(HasFrozenModel))]
    [NotifyPropertyChangedFor(nameof(FrozenModelDisplay))]
    [NotifyPropertyChangedFor(nameof(FrozenThinkingLevelDisplay))]
    [NotifyPropertyChangedFor(nameof(HasFrozenThinkingLevel))]
    [NotifyPropertyChangedFor(nameof(FrozenModelRevisionDisplay))]
    private MessageTask? _selectedTask;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasRun))]
    [NotifyPropertyChangedFor(nameof(CanRetry))]
    [NotifyPropertyChangedFor(nameof(AcceptanceReport))]
    [NotifyPropertyChangedFor(nameof(HasAcceptanceReport))]
    [NotifyPropertyChangedFor(nameof(ShowsMissingAcceptanceReport))]
    [NotifyPropertyChangedFor(nameof(AcceptanceCharacterCountLabel))]
    private MessageTaskRunDetail? _runDetail;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EventsCountLabel))]
    private int _eventsTotal;

    [ObservableProperty]
    private bool _eventsHasMore;

    [ObservableProperty]
    private string _retryInstruction = string.Empty;

    [ObservableProperty]
    private string _cancelReason = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ShowsEmptyGraph), nameof(ShowsGraphLoadError))]
    private string? _errorMessage;

    public async Task OpenAsync(
        MessageTaskGraphRequest request,
        CancellationToken cancellationToken = default)
    {
        if (_disposed) return;
        CancelSession();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var load = BeginLoad();
        await UpdateAsync(load, () => Reset(request)).ConfigureAwait(false);
        await LoadGraphInternalAsync(request, load).ConfigureAwait(false);
    }

    public async Task<TaskReplyInspectorSnapshot> LoadInspectorAsync(
        MessageTaskGraphRequest request,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var task = await _service.FetchTaskAsync(
            request.MessageId,
            request.TaskId,
            request.Lookup,
            cancellationToken).ConfigureAwait(false);
        cancellationToken.ThrowIfCancellationRequested();
        var runId = task.LastRunId ?? request.RunId;
        MessageTaskRunDetail? run = null;
        if (!string.IsNullOrWhiteSpace(runId))
        {
            run = await _service.FetchRunAsync(
                request.MessageId,
                runId,
                request.Lookup,
                true,
                40,
                0,
                cancellationToken).ConfigureAwait(false);
        }

        cancellationToken.ThrowIfCancellationRequested();
        return new TaskReplyInspectorSnapshot(task, run);
    }

    [RelayCommand]
    private void Close()
    {
        CancelSession();
        IsOpen = false;
        IsLoading = false;
        IsAwaitingInitialGraph = false;
        IsUsingPreviousGraph = false;
        IsLoadingMoreEvents = false;
        IsApplyingAction = false;
    }

    [RelayCommand]
    private async Task SelectNodeAsync(MessageTaskGraphNodeItemViewModel? node)
    {
        if (!HasActiveSession || node is null || !Nodes.Contains(node) ||
            MessageId is not { } messageId || Lookup is not { } lookup)
        {
            return;
        }

        var load = BeginLoad();
        await UpdateAsync(load, () =>
        {
            SelectedNode = node;
            RequestedTaskId = node.Id;
            RequestedRunId = null;
            RetryInstruction = string.Empty;
            CancelReason = string.Empty;
            ClearTaskDetail();
        }).ConfigureAwait(false);
        await RunLoadAsync(load, () => LoadTaskInternalAsync(
            messageId, node.Id, node.Task.LastRunId, lookup, load)).ConfigureAwait(false);
    }

    [RelayCommand]
    private async Task RefreshAsync()
    {
        if (IsApplyingAction) return;
        var sessionToken = _sessionCancellation?.Token;
        await RefreshCurrentAsync().ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            if (sessionToken == _sessionCancellation?.Token && HasActiveSession) StartRealtime();
        }).ConfigureAwait(false);
    }

    private async Task RefreshCurrentAsync(bool isBackground = false)
    {
        if (!HasActiveSession || MessageId is not { } messageId || RequestedTaskId is not { } taskId ||
            Lookup is not { } lookup)
        {
            return;
        }

        var request = new MessageTaskGraphRequest(messageId, taskId, RequestedRunId, lookup);
        await LoadGraphInternalAsync(request, BeginLoad(), isBackground).ConfigureAwait(false);
    }

    [RelayCommand]
    private async Task LoadMoreEventsAsync()
    {
        if (!CanLoadMoreEvents || RunDetail is not { } detail ||
            MessageId is not { } messageId || Lookup is not { } lookup || _loadCancellation is null)
        {
            return;
        }

        var load = _isRefreshing ? BeginLoad() : new LoadRequest(_generation, _loadCancellation.Token);
        StopPolling();
        IsLoadingMoreEvents = true;
        ErrorMessage = null;
        _pollingErrorMessage = null;
        try
        {
            var next = await _service.FetchRunAsync(
                messageId,
                detail.Run.Id,
                lookup,
                true,
                40,
                RunEvents.Count,
                load.Token).ConfigureAwait(false);
            await UpdateAsync(load, () =>
            {
                if (RunDetail?.Run.Id != detail.Run.Id)
                {
                    return;
                }

                var known = RunEvents.Select(static value => value.Id).ToHashSet(StringComparer.Ordinal);
                foreach (var item in next.Events.Where(value => known.Add(value.Id)))
                {
                    RunEvents.Add(item);
                }

                EventsTotal = next.EventsTotal;
                EventsHasMore = next.EventsHasMore;
            }).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (load.Token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await UpdateAsync(load, () => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsLatest(load))
                {
                    IsLoadingMoreEvents = false;
                    SchedulePolling(load);
                }
            }).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private Task CancelTaskAsync()
    {
        if (!CanCancel || MessageId is not { } messageId || SelectedTask is not { } task ||
            Lookup is not { } lookup || _sessionCancellation is null || _loadCancellation is null)
        {
            return Task.CompletedTask;
        }

        var load = _isRefreshing ? BeginLoad() : new LoadRequest(_generation, _loadCancellation.Token);
        var reason = CancelReason;
        return ApplyActionAsync(load, async token =>
        {
            await _service.CancelTaskAsync(
                messageId,
                task.Id,
                lookup,
                reason,
                token).ConfigureAwait(false);
            await RefreshAfterActionAsync(load).ConfigureAwait(false);
        });
    }

    [RelayCommand]
    private Task RetryRunAsync()
    {
        if (!HasActiveSession || IsLoading || IsApplyingAction) return Task.CompletedTask;
        if (!CanRetry)
        {
            ErrorMessage = "Only a blocked, failed, or cancelled task can be retried.";
            return Task.CompletedTask;
        }

        if (MessageId is not { } messageId || RunDetail is not { } detail || Lookup is not { } lookup)
        {
            ErrorMessage = "The task run is missing its conversation context. Reopen the task graph and try again.";
            return Task.CompletedTask;
        }

        if (_sessionCancellation is null || _loadCancellation is null)
        {
            ErrorMessage = "The task graph session is no longer active. Reopen the task graph and try again.";
            return Task.CompletedTask;
        }

        var load = _isRefreshing ? BeginLoad() : new LoadRequest(_generation, _loadCancellation.Token);
        var instruction = RetryInstruction;
        return ApplyActionAsync(load, async token =>
        {
            var run = await _service.RetryRunAsync(
                messageId,
                detail.Run.Id,
                lookup,
                instruction,
                token).ConfigureAwait(false);
            await RefreshAfterActionAsync(load, () =>
            {
                RequestedRunId = run.Id.StartsWith("pending-", StringComparison.Ordinal)
                    ? null
                    : run.Id;
                if (RetryInstruction == instruction) RetryInstruction = string.Empty;
            }).ConfigureAwait(false);
        });
    }

    public void Dispose()
    {
        if (_disposed) return;
        Close();
        _disposed = true;
    }

    private async Task LoadGraphInternalAsync(
        MessageTaskGraphRequest request,
        LoadRequest load,
        bool isBackground = false)
    {
        await RunLoadAsync(load, async () =>
        {
            var retriesRemaining = isBackground ? 0 : EmptyGraphRetryLimit;
            MessageTaskGraphSnapshot graph;
            while (true)
            {
                if (!CanApply(load)) return;
                graph = await _service.FetchGraphAsync(
                    request.MessageId,
                    request.Lookup,
                    load.Token).ConfigureAwait(false);
                if (!CanApply(load)) return;
                if (graph.Nodes.Count > 0) break;

                var preserveGraph = false;
                MessageTaskGraphNodeItemViewModel? retainedSelection = null;
                await UpdateAsync(load, () =>
                {
                    preserveGraph = Nodes.Count > 0;
                    if (preserveGraph)
                    {
                        IsUsingPreviousGraph = true;
                        retainedSelection = SelectedNode;
                    }
                    IsAwaitingInitialGraph = !preserveGraph && retriesRemaining > 0;
                }).ConfigureAwait(false);
                if (!CanApply(load)) return;
                if (preserveGraph)
                {
                    // Keep the canvas stable, but still fetch authoritative task/run status after an action.
                    if (retainedSelection is not null)
                    {
                        await LoadTaskInternalAsync(request.MessageId, retainedSelection.Id,
                            request.RunId, request.Lookup, load, isBackground).ConfigureAwait(false);
                    }
                    return;
                }
                if (retriesRemaining == 0) break;

                retriesRemaining--;
                await Task.Delay(EmptyGraphRetryInterval, _timeProvider, load.Token).ConfigureAwait(false);
            }
            MessageTaskGraphNodeItemViewModel? selected = null;
            await UpdateAsync(load, () =>
            {
                IsAwaitingInitialGraph = false;
                IsUsingPreviousGraph = false;
                Nodes.Clear();
                foreach (var node in graph.Nodes
                             .OrderBy(static value => value.Depth)
                             .ThenBy(static value => value.Task.Title, StringComparer.CurrentCultureIgnoreCase))
                {
                    var item = new MessageTaskGraphNodeItemViewModel(node);
                    Nodes.Add(item);
                    if (item.Id == request.TaskId)
                    {
                        selected = item;
                    }
                }

                selected ??= Nodes.FirstOrDefault();
                var selectionChanged = SelectedNode?.Id != selected?.Id;
                if (selectionChanged)
                {
                    RetryInstruction = string.Empty;
                    CancelReason = string.Empty;
                }
                SelectedNode = selected;
                RequestedTaskId = selected?.Id ?? request.TaskId;
                if (selected?.Id != request.TaskId) RequestedRunId = null;
                if (!isBackground || selectionChanged) ClearTaskDetail();
            }).ConfigureAwait(false);

            if (CanApply(load) && selected is not null)
            {
                await LoadTaskInternalAsync(
                    request.MessageId,
                    selected.Id,
                    selected.Id == request.TaskId ? request.RunId ?? selected.Task.LastRunId : selected.Task.LastRunId,
                    request.Lookup,
                    load,
                    isBackground).ConfigureAwait(false);
            }
        }, isBackground).ConfigureAwait(false);
    }

    private async Task LoadTaskInternalAsync(
        string messageId,
        string taskId,
        string? runId,
        MessageTaskLookup lookup,
        LoadRequest load,
        bool isBackground = false)
    {
        var taskResult = await _service.FetchTaskAsync(messageId, taskId, lookup, load.Token)
            .ConfigureAwait(false);
        if (!CanApply(load)) return;
        runId = isBackground || string.IsNullOrWhiteSpace(runId) ? taskResult.LastRunId : runId;
        MessageTaskRunDetail? run = null;
        if (!string.IsNullOrWhiteSpace(runId))
        {
            run = await _service.FetchRunAsync(
                messageId,
                runId,
                lookup,
                true,
                40,
                0,
                load.Token).ConfigureAwait(false);
        }

        await UpdateAsync(load, () =>
        {
            if (SelectedNode?.Id != taskId)
            {
                return;
            }

            SelectedTask = taskResult;
            // Polling must not discard event pages that the user already loaded.
            var previousEvents = isBackground && RunDetail?.Run.Id == run?.Run.Id
                ? RunEvents.ToArray()
                : [];
            RunDetail = run;
            RunEvents.Clear();
            if (run is not null)
            {
                var refreshed = new Dictionary<string, MessageTaskRunEvent>(StringComparer.Ordinal);
                foreach (var item in run.Events) refreshed[item.Id] = item;
                foreach (var item in previousEvents)
                {
                    RunEvents.Add(refreshed.GetValueOrDefault(item.Id, item));
                }
                var known = previousEvents.Select(static item => item.Id).ToHashSet(StringComparer.Ordinal);
                foreach (var item in run.Events.Where(item => known.Add(item.Id)))
                {
                    RunEvents.Add(item);
                }
            }

            EventsTotal = run?.EventsTotal ?? 0;
            EventsHasMore = run is not null && run.EventsHasMore && RunEvents.Count < run.EventsTotal;
        }).ConfigureAwait(false);
    }

    private async Task RunLoadAsync(LoadRequest load, Func<Task> operation, bool isBackground = false)
    {
        try
        {
            await UpdateAsync(load, () =>
            {
                _isRefreshing = isBackground;
                if (!isBackground)
                {
                    IsLoading = true;
                    IsAwaitingInitialGraph = false;
                    IsLoadingMoreEvents = false;
                    ErrorMessage = null;
                    _pollingErrorMessage = null;
                }
            }).ConfigureAwait(false);
            if (CanApply(load)) await operation().ConfigureAwait(false);
            if (isBackground)
            {
                await UpdateAsync(load, () =>
                {
                    if (ErrorMessage == _pollingErrorMessage) ErrorMessage = null;
                    _pollingErrorMessage = null;
                }).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (load.Token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await UpdateAsync(load, () =>
            {
                ErrorMessage = exception.Message;
                _pollingErrorMessage = isBackground ? exception.Message : null;
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsLatest(load))
                {
                    IsLoading = false;
                    IsAwaitingInitialGraph = false;
                    _isRefreshing = false;
                    SchedulePolling(load);
                }
            }).ConfigureAwait(false);
        }
    }

    private async Task ApplyActionAsync(LoadRequest load, Func<CancellationToken, Task> action)
    {
        if (_sessionCancellation is null || IsApplyingAction || !CanApply(load)) return;
        var token = _sessionCancellation.Token;
        StopPolling();
        IsApplyingAction = true;
        ErrorMessage = null;
        _pollingErrorMessage = null;
        try
        {
            await action(token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await UpdateAsync(load, () => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (!_disposed && _sessionCancellation?.Token == token)
                {
                    IsApplyingAction = false;
                    if (_loadCancellation is not null)
                        SchedulePolling(new LoadRequest(_generation, _loadCancellation.Token));
                }
            }).ConfigureAwait(false);
        }
    }

    private async Task RefreshAfterActionAsync(LoadRequest load, Action? update = null)
    {
        Task? refresh = null;
        await UpdateAsync(load, () =>
        {
            update?.Invoke();
            // Capture and start the follow-up on the UI thread before another selection can intervene.
            refresh = RefreshCurrentAsync();
        }).ConfigureAwait(false);
        if (refresh is not null) await refresh.ConfigureAwait(false);
    }

    private readonly record struct LoadRequest(long Generation, CancellationToken Token);

    private void SchedulePolling(LoadRequest load)
    {
        StopPolling();
        if (!CanApply(load) || !HasActiveSession || IsLoading || _isRefreshing ||
            IsLoadingMoreEvents || IsApplyingAction)
            return;
        if (TryRefreshFromRealtime()) return;
        if (!Nodes.Any(static node => IsActive(node.Status))) return;

        _pollingCancellation = CancellationTokenSource.CreateLinkedTokenSource(load.Token);
        _ = PollOnceAsync(load, _pollingCancellation.Token);
    }

    private async Task PollOnceAsync(LoadRequest load, CancellationToken token)
    {
        try
        {
            var interval = _hasActiveRealtimeStream ? TimeSpan.FromSeconds(60) : ActiveGraphPollingInterval;
            await Task.Delay(interval, _timeProvider, token).ConfigureAwait(false);
            Task? refresh = null;
            await _dispatcher.InvokeAsync(() =>
            {
                // Check after dispatch too: a user action may have cancelled this timer in the queue.
                if (token.IsCancellationRequested || !CanApply(load) || !HasActiveSession) return;
                if (IsLoading || _isRefreshing || IsLoadingMoreEvents || IsApplyingAction)
                {
                    SchedulePolling(load);
                    return;
                }
                refresh = RefreshCurrentAsync(isBackground: true);
            }).ConfigureAwait(false);
            if (refresh is not null) await refresh.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
    }

    private void StopPolling()
    {
        _pollingCancellation?.Cancel();
        _pollingCancellation?.Dispose();
        _pollingCancellation = null;
    }

    private LoadRequest BeginLoad()
    {
        CancelLoad();
        _loadCancellation = CancellationTokenSource.CreateLinkedTokenSource(_sessionCancellation!.Token);
        return new LoadRequest(_generation, _loadCancellation.Token);
    }

    private bool IsLatest(LoadRequest load) =>
        !_disposed && _sessionCancellation is not null && load.Generation == _generation;

    private bool CanApply(LoadRequest load) => IsLatest(load) && !load.Token.IsCancellationRequested;

    private Task UpdateAsync(LoadRequest load, Action update) => _dispatcher.InvokeAsync(() =>
    {
        // Check inside the dispatched callback as well: it may have waited behind a new selection.
        if (CanApply(load)) update();
    });

    private void ClearTaskDetail()
    {
        SelectedTask = null;
        RunDetail = null;
        RunEvents.Clear();
        EventsTotal = 0;
        EventsHasMore = false;
        IsLoadingMoreEvents = false;
    }

    private void CancelLoad()
    {
        Interlocked.Increment(ref _generation);
        StopPolling();
        _isRefreshing = false;
        _loadCancellation?.Cancel();
        _loadCancellation?.Dispose();
        _loadCancellation = null;
    }

    private void Reset(MessageTaskGraphRequest request)
    {
        IsOpen = true;
        IsLoading = true;
        IsAwaitingInitialGraph = false;
        IsUsingPreviousGraph = false;
        IsLoadingMoreEvents = false;
        IsApplyingAction = false;
        MessageId = request.MessageId;
        Lookup = request.Lookup;
        RequestedTaskId = request.TaskId;
        RequestedRunId = request.RunId;
        SelectedNode = null;
        SelectedTask = null;
        RunDetail = null;
        EventsTotal = 0;
        EventsHasMore = false;
        RetryInstruction = string.Empty;
        CancelReason = string.Empty;
        ErrorMessage = null;
        _pollingErrorMessage = null;
        Nodes.Clear();
        RunEvents.Clear();
        StartRealtime();
    }

    private void CancelSession()
    {
        StopRealtime();
        CancelLoad();
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }

    private static bool IsTerminal(string? status) => status?.Trim().ToLowerInvariant() is
        "completed" or "succeeded" or "success" or "failed" or "cancelled" or "canceled" or "done";

    private static bool IsActive(string? status) => status?.Trim().ToLowerInvariant() is
        "pending" or "queued" or "ready" or "running" or "processing" or "in_progress" or "doing";

    private static bool IsRetryable(string? status) => status?.Trim().ToLowerInvariant() is
        "blocked" or "failed" or "cancelled" or "canceled";
}

public sealed record TaskReplyInspectorSnapshot(
    MessageTask Task,
    MessageTaskRunDetail? Run);

public sealed class MessageTaskGraphNodeItemViewModel
{
    public MessageTaskGraphNodeItemViewModel(MessageTaskGraphNode node)
    {
        Node = node;
    }

    public MessageTaskGraphNode Node { get; }

    public MessageTask Task => Node.Task;

    public string Id => Task.Id;

    public string Title => Task.Title;

    public string Status => Task.Status ?? "unknown";

    public int Depth => Node.Depth;

    public bool HasPrerequisites => Task.PrerequisiteTaskIds.Count > 0 ||
        Task.PrerequisiteTasks.Count > 0;

    public string DependencySummary
    {
        get
        {
            if (!HasPrerequisites) return string.Empty;
            var references = Task.PrerequisiteTasks.ToDictionary(value => value.Id, StringComparer.Ordinal);
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var labels = Task.PrerequisiteTaskIds.Concat(Task.PrerequisiteTasks.Select(value => value.Id))
                .Where(seen.Add)
                .Select(id => references.GetValueOrDefault(id)?.Title ?? id);
            return $"← {string.Join(", ", labels)}";
        }
    }

    public string AccessibilityName => HasPrerequisites
        ? $"{Title}, {Status}, {DependencySummary}"
        : $"{Title}, {Status}";

    public bool HasRunIdentity => Task.LastRunId is { Length: > 0 };

    public string DisplayTitle => $"{new string('　', Depth)}{Title}";

    public string RunIdentity => Task.LastRunId is { Length: > 0 } runId
        ? $"Run {runId}"
        : string.Empty;
}
