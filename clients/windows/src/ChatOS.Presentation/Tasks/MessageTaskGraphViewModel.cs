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
    private readonly IMessageTaskGraphService _service;
    private readonly IUiDispatcher _dispatcher;
    private CancellationTokenSource? _sessionCancellation;
    private long _generation;

    public MessageTaskGraphViewModel(
        IMessageTaskGraphService service,
        IUiDispatcher dispatcher)
    {
        _service = service;
        _dispatcher = dispatcher;
        Nodes.CollectionChanged += (_, _) => OnPropertyChanged(nameof(IsEmpty));
        RunEvents.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasRunEvents));
    }

    public ObservableCollection<MessageTaskGraphNodeItemViewModel> Nodes { get; } = [];

    public ObservableCollection<MessageTaskRunEvent> RunEvents { get; } = [];

    public bool IsEmpty => Nodes.Count == 0;

    public bool HasRunEvents => RunEvents.Count > 0;

    public bool HasTask => SelectedTask is not null;

    public bool HasRun => RunDetail is not null;

    public bool CanCancel => SelectedTask is { } task && !IsTerminal(task.Status);

    public bool CanRetry => RunDetail is { Run.Status: var status } && IsTerminal(status);

    [ObservableProperty]
    private bool _isOpen;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isLoadingMoreEvents;

    [ObservableProperty]
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
    private MessageTask? _selectedTask;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasRun))]
    [NotifyPropertyChangedFor(nameof(CanRetry))]
    private MessageTaskRunDetail? _runDetail;

    [ObservableProperty]
    private int _eventsTotal;

    [ObservableProperty]
    private bool _eventsHasMore;

    [ObservableProperty]
    private string _retryInstruction = string.Empty;

    [ObservableProperty]
    private string _cancelReason = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    public async Task OpenAsync(
        MessageTaskGraphRequest request,
        CancellationToken cancellationToken = default)
    {
        CancelSession();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() => Reset(request), token);
        await LoadGraphInternalAsync(request, generation, token).ConfigureAwait(false);
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
        if (node is null || MessageId is not { } messageId || Lookup is null || _sessionCancellation is null)
        {
            return;
        }

        SelectedNode = node;
        var generation = Interlocked.Increment(ref _generation);
        await LoadTaskInternalAsync(
            messageId,
            node.Id,
            node.Task.LastRunId,
            Lookup,
            generation,
            _sessionCancellation.Token).ConfigureAwait(false);
    }

    [RelayCommand]
    private async Task RefreshAsync()
    {
        if (MessageId is not { } messageId || RequestedTaskId is not { } taskId ||
            Lookup is null || _sessionCancellation is null)
        {
            return;
        }

        var request = new MessageTaskGraphRequest(messageId, taskId, RequestedRunId, Lookup);
        await LoadGraphInternalAsync(
            request,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token).ConfigureAwait(false);
    }

    [RelayCommand]
    private async Task LoadMoreEventsAsync()
    {
        if (!EventsHasMore || IsLoadingMoreEvents || RunDetail is not { } detail ||
            MessageId is not { } messageId || Lookup is null || _sessionCancellation is null)
        {
            return;
        }

        var token = _sessionCancellation.Token;
        var generation = _generation;
        IsLoadingMoreEvents = true;
        try
        {
            var next = await _service.FetchRunAsync(
                messageId,
                detail.Run.Id,
                Lookup,
                true,
                40,
                RunEvents.Count,
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation)
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
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (_sessionCancellation?.Token == token)
                {
                    IsLoadingMoreEvents = false;
                }
            }).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private Task CancelTaskAsync()
    {
        if (!CanCancel || MessageId is not { } messageId || SelectedTask is not { } task ||
            Lookup is null || _sessionCancellation is null)
        {
            return Task.CompletedTask;
        }

        return ApplyActionAsync(async token =>
        {
            await _service.CancelTaskAsync(
                messageId,
                task.Id,
                Lookup,
                CancelReason,
                token).ConfigureAwait(false);
            await RefreshAsync().ConfigureAwait(false);
        });
    }

    [RelayCommand]
    private Task RetryRunAsync()
    {
        if (!CanRetry || MessageId is not { } messageId || RunDetail is not { } detail ||
            Lookup is null || _sessionCancellation is null)
        {
            return Task.CompletedTask;
        }

        return ApplyActionAsync(async token =>
        {
            var run = await _service.RetryRunAsync(
                messageId,
                detail.Run.Id,
                Lookup,
                RetryInstruction,
                token).ConfigureAwait(false);
            RequestedRunId = run.Id;
            RetryInstruction = string.Empty;
            await RefreshAsync().ConfigureAwait(false);
        });
    }

    public void Dispose() => CancelSession();

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
            var graph = await _service.FetchGraphAsync(
                request.MessageId,
                request.Lookup,
                cancellationToken).ConfigureAwait(false);
            MessageTaskGraphNodeItemViewModel? selected = null;
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation)
                {
                    return;
                }

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
                SelectedNode = selected;
            }, cancellationToken).ConfigureAwait(false);

            if (generation == _generation && selected is not null)
            {
                await LoadTaskInternalAsync(
                    request.MessageId,
                    selected.Id,
                    request.RunId ?? selected.Task.LastRunId,
                    request.Lookup,
                    generation,
                    cancellationToken).ConfigureAwait(false);
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
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    IsLoading = false;
                }
            }).ConfigureAwait(false);
        }
    }

    private async Task LoadTaskInternalAsync(
        string messageId,
        string taskId,
        string? runId,
        MessageTaskLookup lookup,
        long generation,
        CancellationToken cancellationToken)
    {
        var taskResult = await _service.FetchTaskAsync(messageId, taskId, lookup, cancellationToken)
            .ConfigureAwait(false);
        runId = string.IsNullOrWhiteSpace(runId) ? taskResult.LastRunId : runId;
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
                cancellationToken).ConfigureAwait(false);
        }

        await _dispatcher.InvokeAsync(() =>
        {
            if (generation != _generation || SelectedNode?.Id != taskId)
            {
                return;
            }

            SelectedTask = taskResult;
            RunDetail = run;
            RunEvents.Clear();
            if (run is not null)
            {
                foreach (var item in run.Events)
                {
                    RunEvents.Add(item);
                }
            }

            EventsTotal = run?.EventsTotal ?? 0;
            EventsHasMore = run?.EventsHasMore ?? false;
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task ApplyActionAsync(Func<CancellationToken, Task> action)
    {
        if (_sessionCancellation is null)
        {
            return;
        }

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
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (_sessionCancellation?.Token == token)
                {
                    IsApplyingAction = false;
                }
            }).ConfigureAwait(false);
        }
    }

    private void Reset(MessageTaskGraphRequest request)
    {
        IsOpen = true;
        IsLoading = true;
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
        Nodes.Clear();
        RunEvents.Clear();
    }

    private void CancelSession()
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }

    private static bool IsTerminal(string? status) => status?.Trim().ToLowerInvariant() is
        "completed" or "succeeded" or "success" or "failed" or "cancelled" or "canceled" or "done";
}

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

    public string DisplayTitle => $"{new string('　', Depth)}{Title}";

    public string RunIdentity => Task.LastRunId is { Length: > 0 } runId
        ? $"Run {runId}"
        : "尚无运行";
}
