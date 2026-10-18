using ChatOS.Core.Domain;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Threading;
using System.Threading.Channels;

namespace ChatOS.Presentation.Tests;

public sealed partial class MessageTaskGraphViewModelTests
{
    [Fact]
    public async Task InitialEmptyGraphIsRetriedUntilTasksArrive()
    {
        var service = new TaskGraphServiceDouble();
        var graph = await service.FetchGraphAsync("message-1", null);
        var calls = 0;
        service.GraphHandler = (_, _) => Task.FromResult(++calls == 1 ? graph with { Nodes = [] } : graph);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);

        var opening = viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        Assert.True(viewModel.IsAwaitingInitialGraph);
        Assert.True(viewModel.IsLoading);
        Assert.False(viewModel.ShowsEmptyGraph);
        Assert.False(viewModel.ShowsGraphLoadError);
        Assert.Equal(1, calls);
        timer.Fire();
        await opening.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.Equal(2, calls);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.False(viewModel.IsUsingPreviousGraph);
    }

    [Fact]
    public async Task RepeatedInitialEmptyGraphStopsAfterThreeRetries()
    {
        var service = new TaskGraphServiceDouble();
        var graph = await service.FetchGraphAsync("message-1", null);
        var calls = 0;
        service.GraphHandler = (_, _) => { calls++; return Task.FromResult(graph with { Nodes = [] }); };
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);

        var opening = viewModel.OpenAsync(Request());
        await FinishEmptyGraphRetriesAsync(opening, clock);

        Assert.Equal(4, calls);
        Assert.True(viewModel.IsEmpty);
        Assert.False(viewModel.IsLoading);
        Assert.Null(viewModel.ErrorMessage);
        Assert.True(viewModel.ShowsEmptyGraph);
        Assert.False(viewModel.ShowsGraphLoadError);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.Equal(3, clock.CreatedCount);
    }

    [Fact]
    public async Task TransientEmptyGraphPreservesNodesDetailsEventsAndDraftWhenDetailRefreshFails()
    {
        var service = new TaskGraphServiceDouble();
        var graph = await service.FetchGraphAsync("message-1", null);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        await viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        viewModel.RetryInstruction = "keep this instruction";
        viewModel.CancelReason = "keep this reason";
        var nodes = viewModel.Nodes.ToArray();
        var selected = viewModel.SelectedNode;
        var task = viewModel.SelectedTask;
        var run = viewModel.RunDetail;
        var events = viewModel.RunEvents.ToArray();
        service.GraphHandler = (_, _) => Task.FromResult(graph with { Nodes = [] });
        service.TaskHandler = (_, _, _) => throw new IOException("detail unavailable");

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal(nodes, viewModel.Nodes);
        Assert.Same(selected, viewModel.SelectedNode);
        Assert.Same(task, viewModel.SelectedTask);
        Assert.Same(run, viewModel.RunDetail);
        Assert.Equal(events, viewModel.RunEvents);
        Assert.Equal("keep this instruction", viewModel.RetryInstruction);
        Assert.Equal("keep this reason", viewModel.CancelReason);
        Assert.True(viewModel.IsUsingPreviousGraph);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.Equal(0, clock.CreatedCount);
        Assert.Equal("detail unavailable", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task TransientEmptyGraphStillRefreshesSelectedTaskAndLatestRun()
    {
        var service = new TaskGraphServiceDouble();
        var queued = (await service.FetchTaskAsync("message-1", "task-2", null)) with
        {
            Status = "queued", LastRunId = "run-new",
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request() with { RunId = null });
        var nodes = viewModel.Nodes.ToArray();
        service.GraphHandler = EmptyGraphService().GraphHandler;
        service.TaskHandler = (_, _, _) => Task.FromResult(queued);

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal(nodes, viewModel.Nodes);
        Assert.Equal("queued", viewModel.SelectedTask?.Status);
        Assert.Equal("run-new", viewModel.RunDetail?.Run.Id);
        Assert.True(viewModel.IsUsingPreviousGraph);
        Assert.False(viewModel.CanRetry);
    }

    [Fact]
    public async Task RetryWithTransientEmptyGraphStillLoadsTheNewQueuedRun()
    {
        var service = new TaskGraphServiceDouble();
        var queued = (await service.FetchTaskAsync("message-1", "task-2", null)) with
        {
            Status = "queued", LastRunId = "run-new",
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        service.GraphHandler = EmptyGraphService().GraphHandler;
        service.TaskHandler = (_, _, _) => Task.FromResult(queued);

        await viewModel.RetryRunCommand.ExecuteAsync(null);

        Assert.Equal("run-new", viewModel.RequestedRunId);
        Assert.Equal("run-new", viewModel.RunDetail?.Run.Id);
        Assert.Equal("queued", viewModel.SelectedTask?.Status);
        Assert.True(viewModel.IsUsingPreviousGraph);
        Assert.False(viewModel.CanRetry);
        Assert.False(viewModel.IsApplyingAction);
    }

    [Fact]
    public async Task ManualRefreshStartsANewBoundedEmptyGraphAttempt()
    {
        var service = EmptyGraphService();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await FinishEmptyGraphRetriesAsync(viewModel.OpenAsync(Request()), clock);
        Assert.Equal(4, service.GraphFetchCount);

        var refreshing = viewModel.RefreshCommand.ExecuteAsync(null);
        await FinishEmptyGraphRetriesAsync(refreshing, clock);

        Assert.Equal(8, service.GraphFetchCount);
        Assert.Equal(6, clock.CreatedCount);
        Assert.True(viewModel.ShowsEmptyGraph);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ClosingOrDisposingDuringEmptyGraphWaitStopsRetries(bool dispose)
    {
        var service = EmptyGraphService();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        var opening = viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();

        if (dispose) viewModel.Dispose();
        else viewModel.CloseCommand.Execute(null);
        await opening.WaitAsync(TimeSpan.FromSeconds(5));
        timer.Fire(evenIfDisposed: true);

        Assert.True(timer.IsDisposed);
        Assert.Equal(1, service.GraphFetchCount);
        Assert.Equal(1, clock.CreatedCount);
        Assert.False(viewModel.IsOpen);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.ShowsEmptyGraph);
        Assert.False(viewModel.ShowsGraphLoadError);
    }

    [Fact]
    public async Task ReopeningCancelsWaitingAndKeepsTheNewGraph()
    {
        var service = EmptyGraphService();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        var opening = viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        service.GraphHandler = null;

        await viewModel.OpenAsync(Request("message-2"));
        await opening.WaitAsync(TimeSpan.FromSeconds(5));
        timer.Fire(evenIfDisposed: true);

        Assert.True(timer.IsDisposed);
        Assert.Equal("message-2", viewModel.MessageId);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.False(viewModel.IsUsingPreviousGraph);
        Assert.Equal(2, service.GraphFetchCount);
    }

    [Fact]
    public async Task RefreshWhileWaitingCancelsThePreviousDelayAndOwnsTheLoadingState()
    {
        var service = EmptyGraphService();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        var opening = viewModel.OpenAsync(Request());
        var oldTimer = await clock.NextTimerAsync();

        var refreshing = viewModel.RefreshCommand.ExecuteAsync(null);
        await opening.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.True(oldTimer.IsDisposed);
        Assert.True(viewModel.IsAwaitingInitialGraph);
        Assert.True(viewModel.IsLoading);
        await FinishEmptyGraphRetriesAsync(refreshing, clock);
        Assert.Equal(5, service.GraphFetchCount);
        Assert.False(viewModel.IsLoading);
        Assert.True(viewModel.ShowsEmptyGraph);
    }

    [Fact]
    public async Task CallerCancellationStopsThePendingEmptyGraphDelay()
    {
        var service = EmptyGraphService();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        using var cancellation = new CancellationTokenSource();
        var opening = viewModel.OpenAsync(Request(), cancellation.Token);
        var timer = await clock.NextTimerAsync();

        cancellation.Cancel();
        await opening.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.True(timer.IsDisposed);
        Assert.Equal(1, service.GraphFetchCount);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.IsAwaitingInitialGraph);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task GraphFailureHasAnExplicitErrorStateAndCanRecover(bool failOnRetry)
    {
        var service = EmptyGraphService();
        var emptyHandler = service.GraphHandler!;
        var clock = new GraphTimeProvider();
        service.GraphHandler = (message, token) => failOnRetry && service.GraphFetchCount == 1
            ? emptyHandler(message, token) : throw new IOException("graph unavailable");
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        var opening = viewModel.OpenAsync(Request());
        if (failOnRetry) (await clock.NextTimerAsync()).Fire();
        await opening.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.True(viewModel.ShowsGraphLoadError);
        Assert.False(viewModel.ShowsEmptyGraph);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.False(viewModel.IsLoading);
        Assert.Equal("graph unavailable", viewModel.ErrorMessage);
        Assert.Equal(failOnRetry ? 2 : 1, service.GraphFetchCount);

        service.GraphHandler = null;
        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.False(viewModel.ShowsGraphLoadError);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
    }

    [Fact]
    public async Task ANewNonemptySnapshotClearsThePreviousGraphNotice()
    {
        var service = new TaskGraphServiceDouble();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        service.GraphHandler = EmptyGraphService().GraphHandler;
        await viewModel.RefreshCommand.ExecuteAsync(null);
        Assert.True(viewModel.IsUsingPreviousGraph);

        service.GraphHandler = null;
        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.False(viewModel.IsUsingPreviousGraph);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
        Assert.Equal(0, clock.CreatedCount);
    }

    [Fact]
    public async Task NewSessionCannotReuseThePreviousMessagesGraph()
    {
        var service = new TaskGraphServiceDouble();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        service.GraphHandler = EmptyGraphService().GraphHandler;

        await FinishEmptyGraphRetriesAsync(viewModel.OpenAsync(Request("message-2")), clock);

        Assert.True(viewModel.IsEmpty);
        Assert.Null(viewModel.SelectedTask);
        Assert.Null(viewModel.RunDetail);
        Assert.False(viewModel.IsUsingPreviousGraph);
        Assert.True(viewModel.ShowsEmptyGraph);
        Assert.Equal("message-2", viewModel.MessageId);
    }

    [Fact]
    public async Task OldEmptyResponseCannotStartRetriesOrMarkANewGraphAsPrevious()
    {
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble { GraphHandler = (_, _) => pending.Task };
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        var opening = viewModel.OpenAsync(Request());
        service.GraphHandler = null;
        await viewModel.OpenAsync(Request("message-2"));

        pending.SetResult(EmptyGraph());
        await opening;

        Assert.Equal(0, clock.CreatedCount);
        Assert.False(viewModel.IsUsingPreviousGraph);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
        Assert.Equal("message-2", viewModel.MessageId);
    }

    private static MessageTaskGraphSnapshot EmptyGraph() => new([], [], [], null, null, null);

    private static TaskGraphServiceDouble EmptyGraphService() => new()
    {
        GraphHandler = (_, _) => Task.FromResult(EmptyGraph()),
    };

    private static async Task FinishEmptyGraphRetriesAsync(Task loading, GraphTimeProvider clock)
    {
        for (var attempt = 0; attempt < 3; attempt++)
        {
            var timer = await clock.NextTimerAsync();
            Assert.Equal(TimeSpan.FromMilliseconds(600), timer.DueTime);
            Assert.False(loading.IsCompleted);
            timer.Fire();
        }
        await loading.WaitAsync(TimeSpan.FromSeconds(5));
    }

    private sealed class GraphTimeProvider : TimeProvider
    {
        private readonly Channel<GraphTimer> _timers = Channel.CreateUnbounded<GraphTimer>();
        private int _createdCount;
        public int CreatedCount => Volatile.Read(ref _createdCount);

        public Task<GraphTimer> NextTimerAsync() =>
            _timers.Reader.ReadAsync().AsTask().WaitAsync(TimeSpan.FromSeconds(5));

        public override ITimer CreateTimer(TimerCallback callback, object? state, TimeSpan dueTime, TimeSpan period)
        {
            var timer = new GraphTimer(callback, state, dueTime);
            Interlocked.Increment(ref _createdCount);
            _timers.Writer.TryWrite(timer);
            return timer;
        }
    }

    private sealed class GraphTimer(TimerCallback callback, object? state, TimeSpan dueTime) : ITimer
    {
        private int _disposed;
        public TimeSpan DueTime { get; } = dueTime;
        public bool IsDisposed => Volatile.Read(ref _disposed) != 0;
        public bool Change(TimeSpan dueTime, TimeSpan period) => !IsDisposed;
        public void Fire(bool evenIfDisposed = false)
        {
            if (!IsDisposed || evenIfDisposed) callback(state);
        }
        public void Dispose() => Interlocked.Exchange(ref _disposed, 1);
        public ValueTask DisposeAsync() { Dispose(); return ValueTask.CompletedTask; }
    }
}
