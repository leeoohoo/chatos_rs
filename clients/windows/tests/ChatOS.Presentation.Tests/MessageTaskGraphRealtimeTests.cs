using System.Runtime.CompilerServices;
using System.Threading.Channels;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed partial class MessageTaskGraphViewModelTests
{
    [Fact]
    public async Task OpeningSubscribesOnceToTheGraphsConversationAndClosingCancelsIt()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), null, realtime);

        await viewModel.OpenAsync(Request());

        var stream = await realtime.NextStreamAsync();
        Assert.Equal("conversation-1", stream.ConversationId);
        await viewModel.RefreshCommand.ExecuteAsync(null);
        Assert.Equal(1, realtime.StreamCount);
        viewModel.CloseCommand.Execute(null);
        await stream.Stopped.Task.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.True(stream.Token.IsCancellationRequested);
    }

    [Fact]
    public async Task ReconcileRefreshesEvenAnInactiveGraphWithoutWaitingForPolling()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), null, realtime);
        await viewModel.OpenAsync(Request());
        var stream = await realtime.NextStreamAsync();
        var refreshed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        service.GraphHandler = (_, _) => { refreshed.TrySetResult(); return Task.FromResult(EmptyGraph()); };

        stream.Send(Signal(ConversationRealtimeKind.Reconcile));
        await refreshed.Task.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.Equal(2, service.GraphFetchCount);
    }

    [Fact]
    public async Task FirstSignalUsesSixtySecondFallbackAndStreamEndReturnsToTwoSeconds()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        var initial = await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();

        stream.Send(Signal(ConversationRealtimeKind.Updated));
        var fallback = await clock.NextTimerAsync();
        Assert.Equal(TimeSpan.FromSeconds(60), fallback.DueTime);
        Assert.True(initial.IsDisposed);
        stream.Complete();
        var polling = await clock.NextTimerAsync();

        Assert.Equal(TimeSpan.FromSeconds(2), polling.DueTime);
        Assert.True(fallback.IsDisposed);
    }

    [Theory]
    [InlineData(ConversationRealtimeKind.Completed)]
    [InlineData(ConversationRealtimeKind.Failed)]
    [InlineData(ConversationRealtimeKind.Cancelled)]
    [InlineData(ConversationRealtimeKind.Persisted)]
    public async Task CurrentTurnTerminalSignalsRefreshTheGraphAndInspector(ConversationRealtimeKind kind)
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();
        viewModel.RetryInstruction = "keep this draft";

        stream.Send(Signal(kind));
        var fallback = await clock.NextTimerAsync();

        Assert.Equal(2, service.GraphFetchCount);
        Assert.Equal(2, service.RunFetchCount);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
        Assert.Equal("keep this draft", viewModel.RetryInstruction);
        Assert.Equal(TimeSpan.FromSeconds(60), fallback.DueTime);
    }

    [Theory]
    [InlineData(ConversationRealtimeKind.Completed, "conversation-1", "turn-other")]
    [InlineData(ConversationRealtimeKind.Failed, "conversation-1", null)]
    [InlineData(ConversationRealtimeKind.Reconcile, "conversation-other", "turn-1")]
    [InlineData(ConversationRealtimeKind.Persisted, "conversation-other", "turn-1")]
    [InlineData(ConversationRealtimeKind.Started, "conversation-1", "turn-1")]
    [InlineData(ConversationRealtimeKind.Updated, "conversation-1", "turn-1")]
    [InlineData(ConversationRealtimeKind.Unknown, "conversation-1", "turn-1")]
    public async Task UnrelatedSignalsDoNotReadTheGraph(
        ConversationRealtimeKind kind, string conversationId, string? turnId)
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), null, realtime);
        await viewModel.OpenAsync(Request());
        var stream = await realtime.NextStreamAsync();

        await stream.SendAndWaitAsync(Signal(kind, conversationId, turnId));

        Assert.Equal(1, service.GraphFetchCount);
        Assert.Equal(1, service.RunFetchCount);
    }

    [Fact]
    public async Task MissingTurnIdOnlyAcceptsConversationReconcile()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), null, realtime);
        await viewModel.OpenAsync(Request() with { Lookup = new("conversation-1", null, "message-1") });
        var stream = await realtime.NextStreamAsync();

        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Completed, turnId: null));
        Assert.Equal(1, service.GraphFetchCount);
        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Reconcile, turnId: null));

        Assert.Equal(2, service.GraphFetchCount);
    }

    [Fact]
    public async Task ReconcileForTheSameConversationDoesNotRequireTheSelectedTurn()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), null, realtime);
        await viewModel.OpenAsync(Request());
        var stream = await realtime.NextStreamAsync();

        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Reconcile, turnId: "turn-other"));

        Assert.Equal(2, service.GraphFetchCount);
    }

    [Fact]
    public async Task BurstDuringASlowReadIsCoalescedIntoOneFollowUp()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var graph = await service.FetchGraphAsync("message-1", null);
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var calls = 0;
        service.GraphHandler = (_, _) => ++calls == 1 ? pending.Task : Task.FromResult(graph);

        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Reconcile));
        for (var index = 0; index < 100; index++) stream.Send(Signal(ConversationRealtimeKind.Completed));
        await WaitUntilAsync(() => stream.ProcessedCount == 101);
        Assert.Equal(3, service.GraphFetchCount);
        Assert.Equal(1, calls);
        Assert.False(viewModel.IsLoading);
        pending.SetResult(graph);
        var next = await clock.NextTimerAsync();

        Assert.Equal(4, service.GraphFetchCount);
        Assert.Equal(2, calls);
        Assert.Equal(TimeSpan.FromSeconds(60), next.DueTime);
    }

    [Fact]
    public async Task SignalsDuringCancelWaitUntilTheOperationAndRefreshFinish()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken actionToken = default;
        service.CancelHandler = token => { actionToken = token; return pending.Task; };

        var cancelling = viewModel.CancelTaskCommand.ExecuteAsync(null);
        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Reconcile));
        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Completed));
        Assert.Equal(1, service.GraphFetchCount);
        Assert.True(viewModel.IsApplyingAction);
        Assert.False(actionToken.IsCancellationRequested);
        pending.SetResult();
        await cancelling;
        await clock.NextTimerAsync();

        Assert.Equal(3, service.GraphFetchCount);
        Assert.False(viewModel.IsApplyingAction);
        Assert.False(actionToken.IsCancellationRequested);
    }

    [Fact]
    public async Task SignalsDuringPagingPreserveTheLoadedPageAndWaitForIt()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();
        var page = new TaskCompletionSource<MessageTaskRunDetail>(TaskCreationOptions.RunContinuationsAsynchronously);
        service.RunHandler = (_, id, offset, _) => offset > 0 ? page.Task : service.RunPage(id, 0);

        var paging = viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Reconcile));
        Assert.True(viewModel.IsLoadingMoreEvents);
        Assert.Equal(1, service.GraphFetchCount);
        page.SetResult(await service.RunPage("run-2", 1));
        await paging;
        await clock.NextTimerAsync();

        Assert.Equal(2, service.GraphFetchCount);
        Assert.Equal(new[] { "event-1", "event-2" }, viewModel.RunEvents.Select(item => item.Id));
        Assert.False(viewModel.IsLoadingMoreEvents);
    }

    [Fact]
    public async Task StreamFailureKeepsOperationErrorsAndFallsBackToRegularPolling()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();
        stream.Send(Signal(ConversationRealtimeKind.Updated));
        var fallback = await clock.NextTimerAsync();
        viewModel.ErrorMessage = "cancel failed";

        stream.Complete(new IOException("connection lost"));
        var polling = await clock.NextTimerAsync();
        Assert.Equal(TimeSpan.FromSeconds(2), polling.DueTime);
        Assert.True(fallback.IsDisposed);
        Assert.Equal("connection lost", viewModel.RealtimeErrorMessage);
        Assert.True(viewModel.HasRealtimeError);
        polling.Fire();
        await clock.NextTimerAsync();

        Assert.Equal("cancel failed", viewModel.ErrorMessage);
        Assert.True(viewModel.HasRealtimeError);
        Assert.Equal(2, service.GraphFetchCount);
    }

    [Fact]
    public async Task ManualRefreshReconnectsAnEndedStreamAndFirstSignalClearsItsError()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var first = await realtime.NextStreamAsync();
        first.Complete(new IOException("connection lost"));
        await clock.NextTimerAsync();

        await viewModel.RefreshCommand.ExecuteAsync(null);
        await clock.NextTimerAsync();
        var second = await realtime.NextStreamAsync();
        Assert.True(viewModel.HasRealtimeError);
        second.Send(Signal(ConversationRealtimeKind.Updated));
        var timer = await clock.NextTimerAsync();

        Assert.False(viewModel.HasRealtimeError);
        Assert.Null(viewModel.RealtimeErrorMessage);
        Assert.Equal(TimeSpan.FromSeconds(60), timer.DueTime);
        Assert.Equal(2, realtime.StreamCount);
    }

    [Theory]
    [InlineData("close")]
    [InlineData("dispose")]
    [InlineData("cancel")]
    public async Task EndingThePanelStopsTheStreamAndFallbackTimer(string action)
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var cancellation = new CancellationTokenSource();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request(), cancellation.Token);
        await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();
        stream.Send(Signal(ConversationRealtimeKind.Updated));
        var fallback = await clock.NextTimerAsync();

        if (action == "close") viewModel.CloseCommand.Execute(null);
        else if (action == "dispose") viewModel.Dispose();
        else cancellation.Cancel();
        await stream.Stopped.Task.WaitAsync(TimeSpan.FromSeconds(5));
        fallback.Fire(evenIfDisposed: true);

        Assert.True(stream.Token.IsCancellationRequested);
        Assert.True(fallback.IsDisposed);
        Assert.Equal(1, service.GraphFetchCount);
        Assert.Equal(2, clock.CreatedCount);
        Assert.False(viewModel.HasRealtimeError);
    }

    [Fact]
    public async Task QueuedOldSignalCannotRefreshTheReopenedConversation()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        var dispatcher = new PausingUiDispatcher();
        using var viewModel = new MessageTaskGraphViewModel(service, dispatcher, clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var first = await realtime.NextStreamAsync();
        dispatcher.PauseNext = true;
        first.Send(Signal(ConversationRealtimeKind.Reconcile));
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(5));

        await viewModel.OpenAsync(Request("message-2") with { Lookup = new("conversation-2", "turn-2", "message-2") });
        var current = await clock.NextTimerAsync();
        var second = await realtime.NextStreamAsync();
        dispatcher.Release.SetResult();
        await first.Stopped.Task.WaitAsync(TimeSpan.FromSeconds(5));
        current.Fire();
        await clock.NextTimerAsync();

        Assert.Equal("conversation-2", second.ConversationId);
        Assert.Equal("message-2", viewModel.MessageId);
        Assert.Equal(3, service.GraphFetchCount);
        Assert.Null(viewModel.RealtimeErrorMessage);
    }

    [Fact]
    public async Task HealthyStreamStillRefreshesAtTheSixtySecondFallback()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble("running");
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        await viewModel.OpenAsync(Request());
        await clock.NextTimerAsync();
        var stream = await realtime.NextStreamAsync();
        stream.Send(Signal(ConversationRealtimeKind.Updated));
        var timer = await clock.NextTimerAsync();

        timer.Fire();
        var next = await clock.NextTimerAsync();

        Assert.Equal(2, service.GraphFetchCount);
        Assert.Equal(TimeSpan.FromSeconds(60), next.DueTime);
        Assert.Equal(1, realtime.StreamCount);
    }

    [Fact]
    public async Task SignalDuringInitialEmptyRetriesDoesNotRestartTheRetryBudget()
    {
        var realtime = new GraphRealtimeClient();
        var service = EmptyGraphService();
        var clock = new GraphTimeProvider();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), clock, realtime);
        var opening = viewModel.OpenAsync(Request());
        var stream = await realtime.NextStreamAsync();
        await stream.SendAndWaitAsync(Signal(ConversationRealtimeKind.Reconcile));

        await FinishEmptyGraphRetriesAsync(opening, clock);

        Assert.Equal(5, service.GraphFetchCount);
        Assert.Equal(3, clock.CreatedCount);
        Assert.True(viewModel.ShowsEmptyGraph);
    }

    [Fact]
    public async Task EmptyConversationContextDoesNotSubscribe()
    {
        var realtime = new GraphRealtimeClient();
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher(), null, realtime);

        await viewModel.OpenAsync(Request() with { Lookup = new("", "turn-1", "message-1") });

        Assert.Equal(0, realtime.StreamCount);
    }

    private static ConversationRealtimeSignal Signal(
        ConversationRealtimeKind kind,
        string conversationId = "conversation-1",
        string? turnId = "turn-1") => new(
        Guid.NewGuid().ToString(), 1, conversationId, turnId, kind, "test", DateTimeOffset.UtcNow);

    private sealed class GraphRealtimeClient : IRealtimeClient
    {
        private readonly Channel<GraphRealtimeStream> _streams = Channel.CreateUnbounded<GraphRealtimeStream>();
        public int StreamCount { get; private set; }
        public Task<GraphRealtimeStream> NextStreamAsync() =>
            _streams.Reader.ReadAsync().AsTask().WaitAsync(TimeSpan.FromSeconds(5));

        public async IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
            string conversationId, [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            var stream = new GraphRealtimeStream(conversationId, cancellationToken);
            StreamCount++;
            _streams.Writer.TryWrite(stream);
            try
            {
                await foreach (var signal in stream.Signals.Reader.ReadAllAsync(cancellationToken))
                {
                    yield return signal;
                    stream.MarkProcessed();
                }
            }
            finally { stream.Stopped.TrySetResult(); }
        }

        public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await Task.CompletedTask;
            yield break;
        }
    }

    private sealed class GraphRealtimeStream(string conversationId, CancellationToken token)
    {
        private int _processedCount;
        public int ProcessedCount => Volatile.Read(ref _processedCount);
        public string ConversationId { get; } = conversationId;
        public CancellationToken Token { get; } = token;
        public Channel<ConversationRealtimeSignal> Signals { get; } = Channel.CreateUnbounded<ConversationRealtimeSignal>();
        public TaskCompletionSource Stopped { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public void Send(ConversationRealtimeSignal signal) => Signals.Writer.TryWrite(signal);
        public void MarkProcessed() => Interlocked.Increment(ref _processedCount);
        public async Task SendAndWaitAsync(ConversationRealtimeSignal signal)
        {
            var expected = ProcessedCount + 1;
            Send(signal);
            await WaitUntilAsync(() => ProcessedCount >= expected);
        }
        public void Complete(Exception? error = null) => Signals.Writer.TryComplete(error);
    }
}
