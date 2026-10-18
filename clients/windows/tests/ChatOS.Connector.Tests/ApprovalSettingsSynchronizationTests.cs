using System.Collections.Concurrent;
using ChatOS.Connector.Approval;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed partial class ApprovalSettingsViewModelTests
{
    [Theory]
    [InlineData(ConnectorApprovalAction.Accept)]
    [InlineData(ConnectorApprovalAction.Decline)]
    [InlineData(ConnectorApprovalAction.AcceptForSession)]
    public async Task DecisionFromAnotherEntryPointUpdatesHistory(ConnectorApprovalAction action)
    {
        var store = new MemoryApprovalStore();
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        var request = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        await WaitUntilAsync(() => viewModel.Pending.Count == 1);

        Assert.True(await coordinator.ResolveAsync(viewModel.Pending[0].Id, action));
        var result = await request;
        await WaitUntilAsync(() => viewModel.HistoryCount == 1 && !viewModel.IsHistoryRefreshing);

        Assert.Empty(viewModel.Pending);
        Assert.Equal(action != ConnectorApprovalAction.Decline, result.Approved);
        Assert.Equal(result.Approved, viewModel.History[0].Entry.Approved);
        Assert.Null(viewModel.ActionMessage);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task PolicyDecisionWithoutPendingQueueUpdatesHistory()
    {
        var store = new MemoryApprovalStore { Mode = ConnectorApprovalMode.FullControl };
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();

        var result = await coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        await WaitUntilAsync(() => viewModel.HistoryCount == 1);

        Assert.True(result.Approved);
        Assert.Empty(viewModel.Pending);
        Assert.Equal(ConnectorApprovalReviewer.Policy, viewModel.History[0].Entry.Reviewer);
        Assert.False(viewModel.IsBusy);
    }

    [Fact]
    public async Task RememberedSessionDecisionUpdatesHistoryWithoutNewPendingApproval()
    {
        using var viewModel = Create(new MemoryApprovalStore(), out var coordinator);
        await viewModel.OpenAsync();
        var first = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        await coordinator.ResolveAsync(coordinator.Snapshot()[0].Id, ConnectorApprovalAction.AcceptForSession);
        await first;

        var repeated = await coordinator.RequestAsync(Request() with { RequestId = "another-request" },
            new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        await WaitUntilAsync(() => viewModel.HistoryCount == 2);

        Assert.Equal(ConnectorApprovalReviewer.Session, repeated.Reviewer);
        Assert.Equal(ConnectorApprovalReviewer.Session, viewModel.History[0].Entry.Reviewer);
        Assert.Empty(viewModel.Pending);
    }

    [Fact]
    public async Task ConnectorDisconnectUpdatesCancelledApprovalHistory()
    {
        using var viewModel = Create(new MemoryApprovalStore(), out var coordinator);
        await viewModel.OpenAsync();
        var pending = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));

        await coordinator.CancelAllAsync("connector disconnected");
        Assert.False((await pending).Approved);
        await WaitUntilAsync(() => viewModel.HistoryCount == 1);

        Assert.Empty(viewModel.Pending);
        Assert.Equal(ConnectorApprovalReviewer.System, viewModel.History[0].Entry.Reviewer);
        Assert.Equal("connector disconnected", viewModel.History[0].Reason);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DecisionBurstHasOneReadInFlightAndOneFollowupWithoutClearingActionState(bool staleFailure)
    {
        var store = HistoryStore(53);
        store.Mode = ConnectorApprovalMode.FullControl;
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        viewModel.NextHistoryPageCommand.Execute(null);
        viewModel.ErrorMessage = "previous action error";
        viewModel.ActionMessage = "previous action result";
        var blocked = new TaskCompletionSource<IReadOnlyList<ConnectorApprovalHistoryEntry>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var reads = 0;
        var active = 0;
        var peak = 0;
        store.HistoryReadHandler = async _ =>
        {
            peak = Math.Max(peak, Interlocked.Increment(ref active));
            try
            {
                return Interlocked.Increment(ref reads) == 1
                    ? await blocked.Task
                    : store.History.ToArray();
            }
            finally { Interlocked.Decrement(ref active); }
        };

        for (var index = 0; index < 100; index++)
            await coordinator.RequestAsync(Request() with { RequestId = $"burst-{index}" },
                new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));

        Assert.Equal(1, reads);
        Assert.False(viewModel.IsBusy);
        Assert.True(viewModel.IsHistoryRefreshing);
        Assert.False(viewModel.CanRefreshHistory);
        if (staleFailure) blocked.SetException(new IOException("obsolete history failure"));
        else blocked.SetResult([]);
        await WaitUntilAsync(() => viewModel.HistoryCount == 153 && !viewModel.IsHistoryRefreshing);

        Assert.Equal(2, reads);
        Assert.Equal(1, peak);
        Assert.Equal(1, viewModel.HistoryPageIndex);
        Assert.Equal("11–20 / 153", viewModel.HistoryRangeLabel);
        Assert.Equal("previous action error", viewModel.ErrorMessage);
        Assert.Equal("previous action result", viewModel.ActionMessage);
        Assert.True(viewModel.CanRefreshHistory);
        Assert.False(viewModel.HasHistorySyncError);
    }

    [Fact]
    public async Task NotificationsDuringForegroundRefreshDiscardStaleResultAndMergeFollowup()
    {
        var store = HistoryStore(53);
        store.Mode = ConnectorApprovalMode.FullControl;
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        viewModel.NextHistoryPageCommand.Execute(null);
        var blocked = new TaskCompletionSource<IReadOnlyList<ConnectorApprovalHistoryEntry>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var reads = 0;
        store.HistoryReadHandler = _ => Interlocked.Increment(ref reads) == 1
            ? blocked.Task : Task.FromResult<IReadOnlyList<ConnectorApprovalHistoryEntry>>(store.History.ToArray());
        var refreshing = viewModel.OpenAsync();

        for (var index = 0; index < 100; index++)
            await coordinator.RequestAsync(Request() with { RequestId = $"foreground-{index}" },
                new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));

        Assert.True(viewModel.IsBusy);
        Assert.Equal(1, reads);
        blocked.SetResult([]);
        await refreshing;
        await WaitUntilAsync(() => viewModel.HistoryCount == 153 && !viewModel.IsHistoryRefreshing);

        Assert.Equal(2, reads);
        Assert.Equal(1, viewModel.HistoryPageIndex);
        Assert.False(viewModel.IsBusy);
    }

    [Fact]
    public async Task HistoryApplyChecksForNewDecisionsInsideQueuedUiCallback()
    {
        var store = HistoryStore(53);
        store.Mode = ConnectorApprovalMode.FullControl;
        var dispatcher = new ApprovalPausingDispatcher();
        var coordinator = new CommandApprovalCoordinator(store);
        using var viewModel = CreateWithDispatcher(coordinator, dispatcher);
        await viewModel.OpenAsync();
        viewModel.NextHistoryPageCommand.Execute(null);
        store.HistoryReadHandler = _ =>
        {
            dispatcher.Paused = true;
            return Task.FromResult<IReadOnlyList<ConnectorApprovalHistoryEntry>>([]);
        };
        var opening = viewModel.OpenAsync();
        Assert.Equal(1, dispatcher.PendingCount);

        await coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));
        Assert.Equal(2, dispatcher.PendingCount);
        store.HistoryReadHandler = null;
        dispatcher.Drain();
        await opening;
        await WaitUntilAsync(() => viewModel.HistoryCount == 54 && !viewModel.IsHistoryRefreshing);

        Assert.Equal(1, viewModel.HistoryPageIndex);
        Assert.Equal("11–20 / 54", viewModel.HistoryRangeLabel);
    }

    [Fact]
    public async Task BackgroundReadFailurePreservesCachedPageAndManualRefreshRecovers()
    {
        var store = HistoryStore(53);
        store.Mode = ConnectorApprovalMode.FullControl;
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        viewModel.NextHistoryPageCommand.Execute(null);
        viewModel.ErrorMessage = "action failed";
        store.HistoryReadError = new IOException("audit unavailable");

        await coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));
        await WaitUntilAsync(() => viewModel.HasHistorySyncError && !viewModel.IsHistoryRefreshing);

        Assert.Equal("audit unavailable", viewModel.HistorySyncErrorMessage);
        Assert.Equal("action failed", viewModel.ErrorMessage);
        Assert.Equal("11–20 / 53", viewModel.HistoryRangeLabel);
        Assert.Equal("history-10", viewModel.History[0].Entry.Id);

        store.HistoryReadError = null;
        await viewModel.OpenAsync();
        Assert.False(viewModel.HasHistorySyncError);
        Assert.Equal("11–20 / 54", viewModel.HistoryRangeLabel);
    }

    [Fact]
    public async Task CommittedDecisionRemainsSuccessfulWhenHistoryRefreshFails()
    {
        var store = new MemoryApprovalStore();
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        var pending = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        await WaitUntilAsync(() => viewModel.Pending.Count == 1 && !viewModel.IsHistoryRefreshing);
        store.HistoryReadError = new IOException("history read failed");

        await viewModel.ResolveAsync(viewModel.Pending[0], ConnectorApprovalAction.Decline);

        Assert.False((await pending).Approved);
        Assert.Single(store.History);
        Assert.Empty(viewModel.Pending);
        Assert.Equal("审批已处理。", viewModel.ActionMessage);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("history read failed", viewModel.HistorySyncErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DisposeCancelsQueuedOperationsAndIgnoresLateOpenCompletion(bool fail)
    {
        var store = new MemoryApprovalStore();
        var blocked = new TaskCompletionSource<IReadOnlyList<ConnectorApprovalHistoryEntry>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken observed = default;
        store.HistoryReadHandler = token => { observed = token; return blocked.Task; };
        var viewModel = Create(store, out _);
        var changes = 0;
        viewModel.PropertyChanged += (_, _) => changes++;
        var opening = viewModel.OpenAsync();
        var queuedMode = viewModel.SetModeAsync(ConnectorApprovalMode.FullControl, true);
        var afterClose = changes;

        viewModel.Dispose();
        viewModel.Dispose();
        Assert.True(observed.IsCancellationRequested);
        if (fail) blocked.SetException(new IOException("late read error"));
        else blocked.SetResult([History("late", true)]);
        await Task.WhenAll(opening, queuedMode);
        await viewModel.OpenAsync();
        await viewModel.SetModeAsync(ConnectorApprovalMode.FullControl, true);

        Assert.Equal(afterClose, changes);
        Assert.Empty(viewModel.History);
        Assert.Equal(ConnectorApprovalMode.RequestApproval, viewModel.Mode);
        Assert.Null(store.Mode);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ExternalCancellationRejectsLateCompletionAndAllowsRetry(bool fail)
    {
        var store = new MemoryApprovalStore();
        var blocked = new TaskCompletionSource<IReadOnlyList<ConnectorApprovalHistoryEntry>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        store.HistoryReadHandler = _ => blocked.Task;
        using var viewModel = Create(store, out _);
        using var cancellation = new CancellationTokenSource();
        var opening = viewModel.OpenAsync(cancellation.Token);

        cancellation.Cancel();
        if (fail) blocked.SetException(new IOException("late failure after cancellation"));
        else blocked.SetResult([History("cancelled", true)]);
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => opening);
        Assert.Empty(viewModel.History);
        Assert.False(viewModel.IsBusy);
        Assert.Null(viewModel.ErrorMessage);

        store.HistoryReadHandler = null;
        store.History.Add(History("retry", true));
        await viewModel.OpenAsync();
        Assert.Equal("retry", viewModel.History[0].Entry.Id);
    }

    [Fact]
    public async Task DisposeRejectsLateBackgroundReadAndDoesNotStartFollowup()
    {
        var store = new MemoryApprovalStore { Mode = ConnectorApprovalMode.FullControl };
        var blocked = new TaskCompletionSource<IReadOnlyList<ConnectorApprovalHistoryEntry>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        CancellationToken observed = default;
        store.HistoryReadHandler = token => { observed = token; return blocked.Task; };
        await coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));
        await coordinator.RequestAsync(Request() with { RequestId = "followup" },
            new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));
        var reads = store.HistoryReadCount;
        var changes = 0;
        viewModel.PropertyChanged += (_, _) => changes++;

        viewModel.Dispose();
        blocked.SetResult(store.History.ToArray());
        await WaitUntilAsync(() => observed.IsCancellationRequested);
        await coordinator.RequestAsync(Request() with { RequestId = "after-dispose" },
            new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low));

        Assert.Empty(viewModel.History);
        Assert.Equal(reads, store.HistoryReadCount);
        Assert.Equal(0, changes);
    }

    [Fact]
    public async Task QueuedPendingNotificationCannotUpdateDisposedViewModel()
    {
        var store = new MemoryApprovalStore();
        var dispatcher = new ApprovalPausingDispatcher();
        var coordinator = new CommandApprovalCoordinator(store);
        using var viewModel = CreateWithDispatcher(coordinator, dispatcher);
        await viewModel.OpenAsync();
        dispatcher.Paused = true;
        var pending = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        Assert.Equal(1, dispatcher.PendingCount);

        viewModel.Dispose();
        await coordinator.CancelAllAsync("cleanup");
        dispatcher.Drain();
        Assert.False((await pending).Approved);
        Assert.Empty(viewModel.Pending);
        Assert.Equal(1, store.HistoryReadCount);
    }

    [Fact]
    public async Task QueuedOpenApplyRechecksDisposalEvenWhenDispatcherIgnoresCancellation()
    {
        var dispatcher = new ApprovalPausingDispatcher();
        var store = new MemoryApprovalStore
        {
            HistoryReadHandler = _ =>
            {
                dispatcher.Paused = true;
                return Task.FromResult<IReadOnlyList<ConnectorApprovalHistoryEntry>>([History("queued", true)]);
            },
        };
        var coordinator = new CommandApprovalCoordinator(store);
        var viewModel = CreateWithDispatcher(coordinator, dispatcher);
        var opening = viewModel.OpenAsync();
        Assert.Equal(1, dispatcher.PendingCount);

        viewModel.Dispose();
        dispatcher.Drain();
        await opening;

        Assert.Empty(viewModel.History);
        Assert.Null(viewModel.ErrorMessage);
    }

    private static ApprovalSettingsViewModel CreateWithDispatcher(
        CommandApprovalCoordinator coordinator, IUiDispatcher dispatcher) => new(coordinator,
            new LocalizationViewModel(new AppPreferencesManager(new MemoryPreferencesStore()), dispatcher), dispatcher);

    private sealed class ApprovalPausingDispatcher : IUiDispatcher
    {
        private readonly ConcurrentQueue<(Action Action, TaskCompletionSource Completion)> _pending = new();
        public bool Paused { get; set; }
        public int PendingCount => _pending.Count;

        public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            if (!Paused) { action(); return Task.CompletedTask; }
            var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            _pending.Enqueue((action, completion));
            return completion.Task;
        }

        public void Drain()
        {
            Paused = false;
            while (_pending.TryDequeue(out var callback))
            {
                try { callback.Action(); callback.Completion.SetResult(); }
                catch (Exception exception) { callback.Completion.SetException(exception); }
            }
        }
    }
}
