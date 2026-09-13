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
    private readonly ILocalAgentTaskService _service;
    private readonly ILocalAgentRunControlService _runControls;
    private readonly IUiDispatcher _dispatcher;
    private CancellationTokenSource? _sessionCancellation;
    private long _generation;

    public MessageTaskGraphViewModel(ILocalAgentTaskService service,
        ILocalAgentRunControlService runControls, IUiDispatcher dispatcher)
    {
        _service = service;
        _runControls = runControls;
        _dispatcher = dispatcher;
        _service.AccountProjectionCleared += OnAccountProjectionCleared;
        Nodes.CollectionChanged += (_, _) => OnPropertyChanged(nameof(IsEmpty));
        RunEvents.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasRunEvents));
    }

    public ObservableCollection<MessageTaskGraphNodeItemViewModel> Nodes { get; } = [];
    public ObservableCollection<LocalAgentTaskRunChoiceViewModel> Runs { get; } = [];
    public ObservableCollection<LocalAgentRunTimelineEvent> RunEvents { get; } = [];
    public bool IsEmpty => Nodes.Count == 0;
    public bool HasRunEvents => RunEvents.Count > 0;
    public bool HasTask => SelectedTask is not null;
    public bool HasRun => RunDetail is not null;
    public bool CanCancel =>
        SelectedTask is { CurrentRunId: var currentRunId }
        && RunDetail is { Run.Run: var run }
        && string.Equals(currentRunId, run.RunId, StringComparison.Ordinal)
        && SelectedRunControl?.CanCancel == true;
    public bool CanPause => SelectedRunControl?.CanPause == true;
    public bool CanResume => SelectedRunControl?.CanResume == true;
    public string? ReviewReason => SelectedRunControl?.ReviewReason;
    public bool CanRetry =>
        SelectedTask is { CurrentRunId: var currentRunId }
        && RunDetail is { Run.Run: var run }
        && string.Equals(currentRunId, run.RunId, StringComparison.Ordinal)
        && IsTerminal(run.Status);

    [ObservableProperty] private bool _isOpen;
    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private bool _isLoadingMoreEvents;
    [ObservableProperty] private bool _isApplyingAction;
    [ObservableProperty] private string? _sourceThreadId;
    [ObservableProperty] private string? _sourceTurnId;
    [ObservableProperty] private string? _requestedTaskId;
    [ObservableProperty] private string? _requestedRunId;
    [ObservableProperty] private MessageTaskGraphNodeItemViewModel? _selectedNode;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasTask))]
    [NotifyPropertyChangedFor(nameof(CanCancel))]
    [NotifyPropertyChangedFor(nameof(CanRetry))]
    private LocalAgentTaskSnapshot? _selectedTask;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasRun))]
    [NotifyPropertyChangedFor(nameof(CanCancel))]
    [NotifyPropertyChangedFor(nameof(CanRetry))]
    private LocalAgentTaskRunDetail? _runDetail;

    [ObservableProperty] private LocalAgentTaskRunChoiceViewModel? _selectedRun;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCancel))]
    [NotifyPropertyChangedFor(nameof(CanPause))]
    [NotifyPropertyChangedFor(nameof(CanResume))]
    [NotifyPropertyChangedFor(nameof(ReviewReason))]
    private LocalAgentRunControlState? _selectedRunControl;
    [ObservableProperty] private uint _eventsTotal;
    [ObservableProperty] private bool _eventsHasMore;
    [ObservableProperty] private string _retryInstruction = string.Empty;
    [ObservableProperty] private string? _errorMessage;

    public async Task OpenAsync(MessageTaskGraphRequest request, CancellationToken cancellationToken = default)
    {
        CancelSession();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() => Reset(request), token);
        await LoadGraphInternalAsync(request, generation, token).ConfigureAwait(false);
    }

    public async Task SelectRunAsync(
        LocalAgentTaskRunChoiceViewModel? run,
        CancellationToken cancellationToken = default)
    {
        if (run is null || SelectedTask is not { } task || _sessionCancellation is null)
        {
            return;
        }
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            _sessionCancellation.Token,
            cancellationToken);
        SelectedRun = run;
        RequestedRunId = run.RunId;
        try
        {
            await LoadRunInternalAsync(
                task,
                run.RunId,
                Interlocked.Increment(ref _generation),
                linked.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (linked.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private void Close()
    {
        CancelSession();
        IsOpen = false;
    }

    [RelayCommand]
    private async Task SelectNodeAsync(MessageTaskGraphNodeItemViewModel? node)
    {
        if (node is null || _sessionCancellation is null)
        {
            return;
        }
        SelectedNode = node;
        RequestedTaskId = node.Id;
        RequestedRunId = node.Task.CurrentRunId;
        var token = _sessionCancellation.Token;
        try
        {
            await LoadTaskInternalAsync(
                node.Id,
                node.Task.CurrentRunId,
                Interlocked.Increment(ref _generation),
                token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task RefreshAsync()
    {
        if (SourceThreadId is not { } sourceThreadId
            || SourceTurnId is not { } sourceTurnId
            || RequestedTaskId is not { } taskId
            || _sessionCancellation is null)
        {
            return;
        }
        await LoadGraphInternalAsync(
            new MessageTaskGraphRequest(sourceThreadId, sourceTurnId, taskId, RequestedRunId),
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token).ConfigureAwait(false);
    }

    [RelayCommand]
    private async Task LoadMoreEventsAsync()
    {
        if (!EventsHasMore || IsLoadingMoreEvents || RunDetail is not { } detail
            || _sessionCancellation is null)
        {
            return;
        }
        var token = _sessionCancellation.Token;
        var generation = _generation;
        IsLoadingMoreEvents = true;
        try
        {
            var next = await _service.GetRunDetailAsync(
                detail.Task.TaskId,
                detail.Run.Run.RunId,
                40,
                (uint)RunEvents.Count,
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation) return;
                var known = RunEvents.Select(static value => value.EventId)
                    .ToHashSet(StringComparer.Ordinal);
                foreach (var item in next.Events.Where(value => known.Add(value.EventId)))
                {
                    RunEvents.Add(item);
                }
                EventsTotal = next.EventsTotal;
                EventsHasMore = next.EventsHasMore;
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (_sessionCancellation?.Token == token) IsLoadingMoreEvents = false;
            }).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private Task CancelTaskAsync()
    {
        if (!CanCancel || RunDetail is not { Run.Run: var run }) return Task.CompletedTask;
        return ApplyActionAsync(async token =>
        {
            await _runControls.CancelRunAsync(run.RunId, SourceThreadId!, token)
                .ConfigureAwait(false);
            await RefreshAsync().ConfigureAwait(false);
        });
    }

    [RelayCommand]
    private Task PauseRunAsync() => ApplyRunControlAsync(CanPause,
        (runId, threadId, token) => _runControls.PauseRunAsync(runId, threadId, token));

    [RelayCommand]
    private Task ResumeRunAsync() => ApplyRunControlAsync(CanResume,
        (runId, threadId, token) => _runControls.ResumeRunAsync(runId, threadId, token));

    private Task ApplyRunControlAsync(bool allowed,
        Func<string, string, CancellationToken, Task> action)
    {
        if (!allowed || SelectedRunControl is not { } control || SourceThreadId is null)
            return Task.CompletedTask;
        return ApplyActionAsync(async token =>
        {
            await action(control.RunId, SourceThreadId, token).ConfigureAwait(false);
            await RefreshAsync().ConfigureAwait(false);
        });
    }

    [RelayCommand]
    private Task RetryRunAsync()
    {
        if (!CanRetry || RunDetail is not { Run.Run: var run }) return Task.CompletedTask;
        return ApplyActionAsync(async token =>
        {
            var created = await _service.RetryCurrentRunAsync(
                RunDetail!.Task.TaskId,
                run.RunId,
                RetryInstruction,
                token).ConfigureAwait(false);
            RequestedRunId = created.Run.RunId;
            RetryInstruction = string.Empty;
            await RefreshAsync().ConfigureAwait(false);
        });
    }

    public void Dispose()
    {
        CancelSession();
        _service.AccountProjectionCleared -= OnAccountProjectionCleared;
    }

    private async Task LoadGraphInternalAsync(
        MessageTaskGraphRequest request,
        long generation,
        CancellationToken cancellationToken)
    {
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoading = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);
        try
        {
            var graph = await _service.GetGraphAsync(
                request.SourceThreadId,
                request.SourceTurnId,
                cancellationToken).ConfigureAwait(false);
            MessageTaskGraphNodeItemViewModel? selected = null;
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation) return;
                Nodes.Clear();
                foreach (var node in graph.Nodes
                             .OrderBy(static value => value.Depth)
                             .ThenBy(static value => value.Task.Task.Objective,
                                 StringComparer.CurrentCultureIgnoreCase))
                {
                    var item = new MessageTaskGraphNodeItemViewModel(node);
                    Nodes.Add(item);
                    if (item.Id == request.TaskId) selected = item;
                }
                selected ??= Nodes.FirstOrDefault();
                SelectedNode = selected;
            }, cancellationToken).ConfigureAwait(false);
            if (generation == _generation && selected is not null)
            {
                RequestedTaskId = selected.Id;
                await LoadTaskInternalAsync(
                    selected.Id,
                    request.RunId ?? selected.Task.CurrentRunId,
                    generation,
                    cancellationToken).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation) IsLoading = false;
            }).ConfigureAwait(false);
        }
    }

    private async Task LoadTaskInternalAsync(
        string taskId,
        string? runId,
        long generation,
        CancellationToken cancellationToken)
    {
        var task = await _service.GetTaskAsync(taskId, cancellationToken).ConfigureAwait(false);
        runId = string.IsNullOrWhiteSpace(runId) ? task.CurrentRunId : runId;
        if (!task.RunIds.Contains(runId, StringComparer.Ordinal))
        {
            throw new InvalidDataException("The requested run does not belong to this Local Agent task.");
        }
        await _dispatcher.InvokeAsync(() =>
        {
            if (generation != _generation || SelectedNode?.Id != taskId) return;
            SelectedTask = task;
            Runs.Clear();
            foreach (var id in task.RunIds.Reverse())
            {
                Runs.Add(new LocalAgentTaskRunChoiceViewModel(
                    id,
                    string.Equals(id, task.CurrentRunId, StringComparison.Ordinal)));
            }
            RequestedRunId = runId;
            SelectedRun = Runs.First(value => value.RunId == runId);
        }, cancellationToken).ConfigureAwait(false);
        await LoadRunInternalAsync(task, runId, generation, cancellationToken).ConfigureAwait(false);
    }

    private async Task LoadRunInternalAsync(
        LocalAgentTaskSnapshot task,
        string runId,
        long generation,
        CancellationToken cancellationToken)
    {
        var detail = await _service.GetRunDetailAsync(
            task.TaskId,
            runId,
            40,
            0,
            cancellationToken).ConfigureAwait(false);
        var controls = await _runControls.FetchRunControlsAsync(
            task.SourceThreadId, cancellationToken).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            if (generation != _generation || SelectedTask?.TaskId != task.TaskId) return;
            SelectedTask = detail.Task;
            RunDetail = detail;
            SelectedRunControl = controls.SingleOrDefault(value => value.RunId == runId);
            RunEvents.Clear();
            foreach (var item in detail.Events) RunEvents.Add(item);
            EventsTotal = detail.EventsTotal;
            EventsHasMore = detail.EventsHasMore;
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task ApplyActionAsync(Func<CancellationToken, Task> action)
    {
        if (_sessionCancellation is null) return;
        var token = _sessionCancellation.Token;
        IsApplyingAction = true;
        ErrorMessage = null;
        try
        {
            await action(token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (_sessionCancellation?.Token == token) IsApplyingAction = false;
            }).ConfigureAwait(false);
        }
    }

    private void Reset(MessageTaskGraphRequest request)
    {
        ClearProjection();
        IsOpen = true;
        IsLoading = true;
        IsLoadingMoreEvents = false;
        IsApplyingAction = false;
        SourceThreadId = request.SourceThreadId;
        SourceTurnId = request.SourceTurnId;
        RequestedTaskId = request.TaskId;
        RequestedRunId = request.RunId;
    }

    private void ClearProjection()
    {
        SelectedNode = null;
        SelectedTask = null;
        SelectedRun = null;
        RunDetail = null;
        SelectedRunControl = null;
        EventsTotal = 0;
        EventsHasMore = false;
        RetryInstruction = string.Empty;
        ErrorMessage = null;
        Nodes.Clear();
        Runs.Clear();
        RunEvents.Clear();
    }

    private void CancelSession()
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }

    private void OnAccountProjectionCleared(object? sender, EventArgs e)
    {
        CancelSession();
        Interlocked.Increment(ref _generation);
        _ = _dispatcher.InvokeAsync(() =>
        {
            ClearProjection();
            SourceThreadId = null;
            SourceTurnId = null;
            RequestedTaskId = null;
            RequestedRunId = null;
            IsLoading = false;
            IsOpen = false;
        });
    }

    private static bool IsTerminal(LocalAgentRunStatus status) => status is
        LocalAgentRunStatus.Succeeded or LocalAgentRunStatus.Failed or LocalAgentRunStatus.Cancelled;
}

public sealed class MessageTaskGraphNodeItemViewModel(LocalAgentTaskGraphNode node)
{
    public LocalAgentTaskGraphNode Node { get; } = node;
    public LocalAgentTaskSnapshot Task => Node.Task.Task;
    public string Id => Task.TaskId;
    public string Title => Task.Objective;
    public string Status => Task.Status;
    public int Depth => checked((int)Node.Depth);
    public string DisplayTitle => $"{new string('　', Depth)}{Title}";
    public string RunIdentity => $"Run {Task.CurrentRunId}";
}

public sealed record LocalAgentTaskRunChoiceViewModel(string RunId, bool IsCurrent)
{
    public string DisplayName => IsCurrent ? $"{RunId} · 当前" : RunId;
}
