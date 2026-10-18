using ChatOS.Core.Domain;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed partial class MessageTaskGraphViewModelTests
{
    [Fact]
    public async Task ActiveGraphPollsAfterTwoSecondsAndStopsAtTerminalState()
    {
        var service = new TaskGraphServiceDouble("running", "running");
        var completed = await new TaskGraphServiceDouble("completed", "completed").FetchGraphAsync("message-1", null);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        Assert.Equal(TimeSpan.FromSeconds(2), timer.DueTime);
        Assert.Equal(1, service.GraphFetchCount);
        Assert.False(viewModel.IsLoading);
        service.GraphHandler = (_, _) => Task.FromResult(completed);
        service.TaskHandler = (_, id, _) => new TaskGraphServiceDouble("completed", "completed").FetchTaskAsync("message-1", id, null);
        var finished = ObservePollCompletion(viewModel);

        timer.Fire();
        await finished.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.Equal(2, service.GraphFetchCount);
        Assert.Equal("completed", viewModel.SelectedTask?.Status);
        Assert.Equal(1, clock.CreatedCount);
    }

    [Fact]
    public async Task PollWaitsForThePreviousGraphAndInspectorBeforeSchedulingAgain()
    {
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var run = new TaskCompletionSource<MessageTaskRunDetail>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble("running", "running");
        var graph = await service.FetchGraphAsync("message-1", null);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        service.GraphHandler = (_, _) => pending.Task;
        service.RunHandler = (_, _, _, _) => run.Task;

        timer.Fire();
        await WaitUntilAsync(() => service.GraphFetchCount == 3);
        Assert.Equal(1, clock.CreatedCount);
        pending.SetResult(graph);
        await WaitUntilAsync(() => service.RunFetchCount == 2);
        Assert.Equal(1, clock.CreatedCount);
        run.SetResult(await service.RunPage("run-2", 0));
        var next = await clock.NextTimerAsync();

        Assert.Equal(TimeSpan.FromSeconds(2), next.DueTime);
        Assert.Equal(2, clock.CreatedCount);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ClosingOrDisposingStopsTheActiveGraphTimer(bool dispose)
    {
        var service = new TaskGraphServiceDouble("running", "running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();

        if (dispose) viewModel.Dispose();
        else viewModel.CloseCommand.Execute(null);
        timer.Fire(evenIfDisposed: true);

        Assert.True(timer.IsDisposed);
        Assert.Equal(1, service.GraphFetchCount);
        Assert.Equal(1, clock.CreatedCount);
    }

    [Theory]
    [InlineData("pending")]
    [InlineData("queued")]
    [InlineData("ready")]
    [InlineData(" RUNNING ")]
    [InlineData("processing")]
    [InlineData("in_progress")]
    [InlineData("doing")]
    public async Task AllMacActiveStatusesStartPolling(string status)
    {
        var service = new TaskGraphServiceDouble(status);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());

        Assert.Equal(TimeSpan.FromSeconds(2), (await clock.NextTimerAsync()).DueTime);
        Assert.Equal(1, service.GraphFetchCount);
    }

    [Theory]
    [InlineData("blocked")]
    [InlineData("completed")]
    [InlineData("failed")]
    [InlineData("cancelled")]
    [InlineData("unknown")]
    public async Task InactiveGraphDoesNotStartPolling(string status)
    {
        var service = new TaskGraphServiceDouble(status);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());

        Assert.Equal(0, clock.CreatedCount);
    }

    [Fact]
    public async Task PollsOtherActiveNodesWhileTheSelectedTaskIsCompleted()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request() with { TaskId = "task-1", RunId = "run-1" });

        Assert.Equal("completed", viewModel.SelectedTask?.Status);
        Assert.Equal(TimeSpan.FromSeconds(2), (await clock.NextTimerAsync()).DueTime);
    }

    [Fact]
    public async Task EmptyGraphRecoveryTransitionsToTheNormalPollingInterval()
    {
        var service = new TaskGraphServiceDouble("queued");
        var graph = await service.FetchGraphAsync("message-1", null);
        var calls = 0;
        service.GraphHandler = (_, _) => Task.FromResult(++calls == 1 ? EmptyGraph() : graph);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        var opening = viewModel.OpenAsync(Request());
        var retry = await clock.NextTimerAsync();
        Assert.Equal(TimeSpan.FromMilliseconds(600), retry.DueTime);
        retry.Fire();
        await opening;

        Assert.Equal(TimeSpan.FromSeconds(2), (await clock.NextTimerAsync()).DueTime);
        Assert.Equal(2, calls);
        Assert.False(viewModel.IsAwaitingInitialGraph);
    }

    [Fact]
    public async Task CallerCancellationStopsNormalPolling()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var cancellation = new CancellationTokenSource();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request(), cancellation.Token);
        var timer = await clock.NextTimerAsync();

        cancellation.Cancel();
        timer.Fire(evenIfDisposed: true);

        Assert.True(timer.IsDisposed);
        Assert.Equal(1, service.GraphFetchCount);
        Assert.False(viewModel.CanCancel);
    }

    [Fact]
    public async Task ManualRefreshReplacesTheTimerWithoutStartingDuplicatePolls()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var old = await clock.NextTimerAsync();

        await viewModel.RefreshCommand.ExecuteAsync(null);
        var current = await clock.NextTimerAsync();
        old.Fire(evenIfDisposed: true);
        Assert.True(old.IsDisposed);
        Assert.False(current.IsDisposed);
        Assert.Equal(2, service.GraphFetchCount);
        current.Fire();
        await clock.NextTimerAsync();
        Assert.Equal(3, service.GraphFetchCount);
    }

    [Fact]
    public async Task QueuedTimerCannotRefreshAReopenedPanel()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        var dispatcher = new PausingUiDispatcher();
        using var viewModel = new MessageTaskGraphViewModel(service, dispatcher, clock);
        await viewModel.OpenAsync(Request());
        var old = await clock.NextTimerAsync();
        dispatcher.PauseNext = true;
        old.Fire();
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(5));

        await viewModel.OpenAsync(Request("message-2"));
        var current = await clock.NextTimerAsync();
        dispatcher.Release.SetResult();
        current.Fire();
        await clock.NextTimerAsync();

        Assert.Equal("message-2", viewModel.MessageId);
        Assert.Equal(3, service.GraphFetchCount);
        Assert.Equal(3, clock.CreatedCount);
    }

    [Fact]
    public async Task PollKeepsDetailsDraftsAndPreviouslyLoadedEventPages()
    {
        var service = new TaskGraphServiceDouble("running", "running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var first = await clock.NextTimerAsync();
        await viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        var timer = await clock.NextTimerAsync();
        Assert.True(first.IsDisposed);
        viewModel.RetryInstruction = "keep draft";
        viewModel.CancelReason = "keep reason";
        var detail = viewModel.RunDetail;
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var graph = await service.FetchGraphAsync("message-1", null);
        service.GraphHandler = (_, _) => pending.Task;

        timer.Fire();
        await WaitUntilAsync(() => service.GraphFetchCount == 3);
        Assert.Same(detail, viewModel.RunDetail);
        Assert.NotNull(viewModel.SelectedTask);
        Assert.False(viewModel.IsLoading);
        Assert.True(viewModel.CanCancel);
        pending.SetResult(graph);
        await clock.NextTimerAsync();

        Assert.Equal(new[] { "event-1", "event-2" }, viewModel.RunEvents.Select(item => item.Id));
        Assert.Equal("task-2", viewModel.SelectedNode?.Id);
        Assert.Equal("keep draft", viewModel.RetryInstruction);
        Assert.Equal("keep reason", viewModel.CancelReason);
        Assert.False(viewModel.EventsHasMore);
    }

    [Fact]
    public async Task PollFailureRetainsDetailsAndRecoversOnTheNextInterval()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        var task = viewModel.SelectedTask;
        var run = viewModel.RunDetail;
        service.GraphHandler = (_, _) => throw new IOException("offline");
        timer.Fire();
        var retry = await clock.NextTimerAsync();
        Assert.Equal("offline", viewModel.ErrorMessage);
        Assert.Same(task, viewModel.SelectedTask);
        Assert.Same(run, viewModel.RunDetail);
        Assert.False(viewModel.IsLoading);

        service.GraphHandler = null;
        retry.Fire();
        await clock.NextTimerAsync();

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal(3, service.GraphFetchCount);
    }

    [Fact]
    public async Task PollDetailFailureRetainsTheLastInspectorAndRetries()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        var run = viewModel.RunDetail;
        service.TaskHandler = (_, _, _) => throw new IOException("details unavailable");
        timer.Fire();
        var retry = await clock.NextTimerAsync();
        Assert.Equal("details unavailable", viewModel.ErrorMessage);
        Assert.Same(run, viewModel.RunDetail);
        Assert.NotNull(viewModel.SelectedTask);

        service.TaskHandler = null;
        retry.Fire();
        await clock.NextTimerAsync();
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task PollUsesTheLatestRunAndClearsThePreviousRunsEventPages()
    {
        var service = new TaskGraphServiceDouble("running");
        var nextTask = (await service.FetchTaskAsync("message-1", "task-2", null)) with { LastRunId = "run-new" };
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        await viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        var timer = await clock.NextTimerAsync();
        service.TaskHandler = (_, _, _) => Task.FromResult(nextTask);

        timer.Fire();
        await clock.NextTimerAsync();

        Assert.Equal("run-new", viewModel.RunDetail?.Run.Id);
        Assert.Single(viewModel.RunEvents);
        Assert.True(viewModel.EventsHasMore);
    }

    [Fact]
    public async Task CancelPreemptsAnInFlightPollAndKeepsItsFailureVisible()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken pollToken = default;
        service.GraphHandler = (_, token) => { pollToken = token; return pending.Task; };
        timer.Fire();
        await WaitUntilAsync(() => pollToken.CanBeCanceled);
        service.CancelHandler = _ => throw new IOException("cancel unavailable");
        viewModel.CancelReason = "keep reason";

        await viewModel.CancelTaskCommand.ExecuteAsync(null);
        var resumed = await clock.NextTimerAsync();
        Assert.True(pollToken.IsCancellationRequested);
        Assert.Equal("cancel unavailable", viewModel.ErrorMessage);
        Assert.Equal("keep reason", viewModel.CancelReason);
        pending.SetResult(EmptyGraph());
        service.GraphHandler = null;
        resumed.Fire();
        await clock.NextTimerAsync();

        Assert.Equal("cancel unavailable", viewModel.ErrorMessage);
        Assert.False(viewModel.IsApplyingAction);
        Assert.Equal(3, service.GraphFetchCount);
    }

    [Fact]
    public async Task PagingPreemptsAnInFlightPollWithoutLosingItsResult()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        var graph = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var page = new TaskCompletionSource<MessageTaskRunDetail>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken pollToken = default;
        service.GraphHandler = (_, token) => { pollToken = token; return graph.Task; };
        timer.Fire();
        await WaitUntilAsync(() => pollToken.CanBeCanceled);
        service.RunHandler = (_, _, _, _) => page.Task;

        var paging = viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        Assert.True(pollToken.IsCancellationRequested);
        Assert.True(viewModel.IsLoadingMoreEvents);
        graph.SetResult(EmptyGraph());
        page.SetResult(await service.RunPage("run-2", 1));
        await paging;
        await clock.NextTimerAsync();

        Assert.Equal(new[] { "event-1", "event-2" }, viewModel.RunEvents.Select(item => item.Id));
        Assert.False(viewModel.IsLoadingMoreEvents);
        Assert.Equal(2, service.GraphFetchCount);
    }

    [Fact]
    public async Task PendingCancelPausesPollingUntilItsAuthoritativeRefreshFinishes()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        service.CancelHandler = _ => pending.Task;
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();

        var cancelling = viewModel.CancelTaskCommand.ExecuteAsync(null);
        timer.Fire(evenIfDisposed: true);
        Assert.True(timer.IsDisposed);
        Assert.True(viewModel.IsApplyingAction);
        Assert.Equal(1, service.GraphFetchCount);
        pending.SetResult();
        await cancelling;
        await clock.NextTimerAsync();

        Assert.Equal(2, service.GraphFetchCount);
        Assert.Equal(2, clock.CreatedCount);
        Assert.False(viewModel.IsApplyingAction);
    }

    [Fact]
    public async Task RetryOfABlockedTaskPausesOtherActiveTasksPolling()
    {
        var service = new TaskGraphServiceDouble();
        var graph = await service.FetchGraphAsync("message-1", null);
        graph = graph with
        {
            Nodes = graph.Nodes.Select(node => node.Id == "task-1"
                ? node with { Task = node.Task with { Status = "running" } } : node).ToArray(),
        };
        service.GraphHandler = (_, _) => Task.FromResult(graph);
        var pending = new TaskCompletionSource<MessageTaskRun>(TaskCreationOptions.RunContinuationsAsynchronously);
        service.RetryHandler = _ => pending.Task;
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        viewModel.RetryInstruction = "retry this task";

        var retrying = viewModel.RetryRunCommand.ExecuteAsync(null);
        timer.Fire(evenIfDisposed: true);
        Assert.True(timer.IsDisposed);
        Assert.True(viewModel.IsApplyingAction);
        Assert.Equal(2, service.GraphFetchCount);
        pending.SetResult(NewRun());
        await retrying;
        await clock.NextTimerAsync();

        Assert.Equal(("message-1", "run-2", "retry this task"), service.LastRetry);
        Assert.Equal(string.Empty, viewModel.RetryInstruction);
        Assert.Equal("run-new", viewModel.RunDetail?.Run.Id);
        Assert.Equal(3, service.GraphFetchCount);
    }

    [Fact]
    public async Task NodeSelectionCancelsTheOldPollAndRefreshesTheNewSelection()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken oldToken = default;
        service.GraphHandler = (_, token) => { oldToken = token; return pending.Task; };
        timer.Fire();
        await WaitUntilAsync(() => oldToken.CanBeCanceled);

        await viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes.Single(node => node.Id == "task-1"));
        var next = await clock.NextTimerAsync();
        Assert.True(oldToken.IsCancellationRequested);
        pending.SetException(new IOException("stale poll error"));
        service.GraphHandler = null;
        next.Fire();
        await clock.NextTimerAsync();

        Assert.Equal("task-1", viewModel.SelectedTask?.Id);
        Assert.Equal("run-1", viewModel.RunDetail?.Run.Id);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ReopeningOrClosingCancelsThePollRequestAndIgnoresItsLateFailure(bool reopen)
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken oldToken = default;
        service.GraphHandler = (_, token) => { oldToken = token; return pending.Task; };
        timer.Fire();
        await WaitUntilAsync(() => oldToken.CanBeCanceled);

        service.GraphHandler = null;
        if (reopen) await viewModel.OpenAsync(Request("message-2"));
        else viewModel.CloseCommand.Execute(null);
        Assert.True(oldToken.IsCancellationRequested);
        pending.SetException(new IOException("old session unavailable"));
        if (reopen)
        {
            (await clock.NextTimerAsync()).Fire();
            await clock.NextTimerAsync();
            Assert.Equal("message-2", viewModel.MessageId);
            Assert.Equal(4, service.GraphFetchCount);
        }

        Assert.Equal(reopen, viewModel.IsOpen);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoading);
    }

    [Fact]
    public async Task TransientEmptyPollPreservesTheGraphAndContinuesTheNormalInterval()
    {
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock);
        await viewModel.OpenAsync(Request());
        var timer = await clock.NextTimerAsync();
        var nodes = viewModel.Nodes.ToArray();
        service.GraphHandler = EmptyGraphService().GraphHandler;

        timer.Fire();
        var next = await clock.NextTimerAsync();

        Assert.Equal(nodes, viewModel.Nodes);
        Assert.True(viewModel.IsUsingPreviousGraph);
        Assert.False(viewModel.IsAwaitingInitialGraph);
        Assert.Equal(TimeSpan.FromSeconds(2), next.DueTime);
        Assert.Equal(2, service.GraphFetchCount);
    }

    private static Task ObservePollCompletion(MessageTaskGraphViewModel viewModel)
    {
        var finished = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        // A new selection is published only after the graph read; detail follows it.
        viewModel.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(viewModel.SelectedTask) && viewModel.SelectedTask is not null)
                finished.TrySetResult();
        };
        return finished.Task;
    }

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        while (!condition()) await Task.Delay(10, timeout.Token);
    }
}
