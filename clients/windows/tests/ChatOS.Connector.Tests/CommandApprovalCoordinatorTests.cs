using ChatOS.Connector.Approval;

namespace ChatOS.Connector.Tests;

public sealed class CommandApprovalCoordinatorTests
{
    [Fact]
    public async Task RequestApprovalSupportsAcceptForSessionAndDoesNotReuseOtherScope()
    {
        var store = new MemoryApprovalStore();
        var coordinator = new CommandApprovalCoordinator(store);
        var first = coordinator.RequestAsync(Request("request-1", "scope-a"), Risk());
        await WaitUntilAsync(() => coordinator.Snapshot().Count == 1);
        var pending = Assert.Single(coordinator.Snapshot());

        Assert.True(await coordinator.ResolveAsync(
            pending.Id,
            ConnectorApprovalAction.AcceptForSession));
        var firstOutcome = await first;
        Assert.True(firstOutcome.Approved);
        Assert.True(firstOutcome.RememberedForSession);

        var repeated = await coordinator.RequestAsync(Request("request-2", "scope-a"), Risk());
        Assert.True(repeated.Approved);
        Assert.Equal(ConnectorApprovalReviewer.Session, repeated.Reviewer);

        var otherScope = coordinator.RequestAsync(Request("request-3", "scope-b"), Risk());
        await WaitUntilAsync(() => coordinator.Snapshot().Count == 1);
        Assert.False(otherScope.IsCompleted);
        await coordinator.CancelAllAsync("disconnected");
        Assert.False((await otherScope).Approved);
        Assert.Empty(coordinator.Snapshot());
        Assert.Equal(3, store.History.Count);
    }

    [Fact]
    public async Task FullControlRequiresExplicitRiskConfirmation()
    {
        var coordinator = new CommandApprovalCoordinator(new MemoryApprovalStore());

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            coordinator.SetModeAsync(ConnectorApprovalMode.FullControl));
        await coordinator.SetModeAsync(
            ConnectorApprovalMode.FullControl,
            fullControlRiskConfirmed: true);

        var outcome = await coordinator.RequestAsync(Request("request-1", "scope"), Risk());
        Assert.True(outcome.Approved);
        Assert.Equal(ConnectorApprovalReviewer.Policy, outcome.Reviewer);
        Assert.Empty(coordinator.Snapshot());
    }

    [Fact]
    public async Task ResolvedApprovalCannotBeProcessedTwice()
    {
        var coordinator = new CommandApprovalCoordinator(new MemoryApprovalStore());
        var request = coordinator.RequestAsync(Request("request-1", "scope"), Risk());
        await WaitUntilAsync(() => coordinator.Snapshot().Count == 1);
        var id = coordinator.Snapshot()[0].Id;

        Assert.True(await coordinator.ResolveAsync(id, ConnectorApprovalAction.Decline));
        Assert.False(await coordinator.ResolveAsync(id, ConnectorApprovalAction.Accept));
        Assert.False((await request).Approved);
    }

    [Fact]
    public async Task ApprovalFinishingAfterDisconnectCannotRestoreSessionAllowlist()
    {
        var store = new BlockingApprovedHistoryStore();
        var coordinator = new CommandApprovalCoordinator(store);
        var first = coordinator.RequestAsync(Request("request-1", "scope"), Risk());
        await WaitUntilAsync(() => coordinator.Snapshot().Count == 1);
        var resolve = coordinator.ResolveAsync(
            coordinator.Snapshot()[0].Id,
            ConnectorApprovalAction.AcceptForSession);
        await store.ApprovedAppendStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));

        await coordinator.CancelAllAsync("disconnected");
        Assert.False((await first).Approved);
        store.ReleaseApprovedAppend.SetResult();
        Assert.False(await resolve);

        var second = coordinator.RequestAsync(Request("request-2", "scope"), Risk());
        await WaitUntilAsync(() => coordinator.Snapshot().Count == 1);
        Assert.False(second.IsCompleted);
        await coordinator.CancelAllAsync("cleanup");
        Assert.False((await second).Approved);
    }

    [Fact]
    public async Task AutoApprovalPersistsAiDecisionBeforeCompleting()
    {
        var store = new MemoryApprovalStore();
        var reviewer = new StubAiReviewer(new CommandApprovalAiReview(
            CommandApprovalAiDecisionKind.Approve,
            "The command is read-only."));
        var coordinator = new CommandApprovalCoordinator(store, reviewer);
        ConnectorApprovalDecisionEventArgs? recorded = null;
        coordinator.DecisionRecorded += (_, value) => recorded = value;
        await coordinator.SetModeAsync(ConnectorApprovalMode.AutoApproval);

        var outcome = await coordinator.RequestAsync(Request("request-ai-1", "scope-ai"), Risk());

        Assert.True(outcome.Approved);
        Assert.Equal(ConnectorApprovalReviewer.Ai, outcome.Reviewer);
        var history = Assert.Single(store.History);
        Assert.True(history.Approved);
        Assert.Equal(ConnectorApprovalReviewer.Ai, history.Reviewer);
        Assert.Equal("request-ai-1", recorded?.Request.RequestId);
        Assert.Equal(ConnectorApprovalReviewer.Ai, recorded?.Outcome.Reviewer);
        Assert.Empty(coordinator.Snapshot());
    }

    [Fact]
    public async Task AutoApprovalDenialIsFinalAndAudited()
    {
        var store = new MemoryApprovalStore();
        var coordinator = new CommandApprovalCoordinator(
            store,
            new StubAiReviewer(new CommandApprovalAiReview(
                CommandApprovalAiDecisionKind.Deny,
                "The command can destroy local changes.")));
        await coordinator.SetModeAsync(ConnectorApprovalMode.AutoApproval);

        var outcome = await coordinator.RequestAsync(Request("request-ai-2", "scope-ai"), Risk());

        Assert.False(outcome.Approved);
        Assert.Equal(ConnectorApprovalReviewer.Ai, outcome.Reviewer);
        Assert.Equal(ConnectorApprovalReviewer.Ai, Assert.Single(store.History).Reviewer);
    }

    [Fact]
    public async Task AutoApprovalAskUserCreatesActionablePendingApproval()
    {
        var store = new MemoryApprovalStore();
        var coordinator = new CommandApprovalCoordinator(
            store,
            new StubAiReviewer(new CommandApprovalAiReview(
                CommandApprovalAiDecisionKind.AskUser,
                "The target branch cannot be inferred.")));
        await coordinator.SetModeAsync(ConnectorApprovalMode.AutoApproval);

        var request = coordinator.RequestAsync(Request("request-ai-3", "scope-ai"), Risk());
        await WaitUntilAsync(() => coordinator.Snapshot().Count == 1);
        var pending = Assert.Single(coordinator.Snapshot());
        Assert.Contains("target branch", pending.Reason, StringComparison.OrdinalIgnoreCase);

        Assert.True(await coordinator.ResolveAsync(pending.Id, ConnectorApprovalAction.Decline));
        Assert.False((await request).Approved);
        Assert.Equal(ConnectorApprovalReviewer.User, Assert.Single(store.History).Reviewer);
    }

    [Fact]
    public async Task AutoApprovalReviewerFailureFallsBackToUser()
    {
        var coordinator = new CommandApprovalCoordinator(
            new MemoryApprovalStore(),
            new StubAiReviewer(new InvalidOperationException("prompt bundle unavailable")));
        await coordinator.SetModeAsync(ConnectorApprovalMode.AutoApproval);

        var request = coordinator.RequestAsync(Request("request-ai-4", "scope-ai"), Risk());
        await WaitUntilAsync(() => coordinator.Snapshot().Count == 1);

        Assert.Equal(
            "The automatic approval reviewer is unavailable; user approval is required.",
            coordinator.Snapshot()[0].Reason);
        await coordinator.CancelAllAsync("cleanup");
        Assert.False((await request).Approved);
    }

    [Fact]
    public async Task DuplicateAutoApprovalIdentitySharesOneModelReview()
    {
        var reviewer = new ControlledAiReviewer();
        var coordinator = new CommandApprovalCoordinator(new MemoryApprovalStore(), reviewer);
        await coordinator.SetModeAsync(ConnectorApprovalMode.AutoApproval);
        var sameRequest = Request("request-ai-5", "scope-ai");

        var first = coordinator.RequestAsync(sameRequest, Risk());
        var second = coordinator.RequestAsync(sameRequest, Risk());
        await reviewer.Started.Task.WaitAsync(TimeSpan.FromSeconds(2));
        Assert.Equal(1, reviewer.CallCount);

        reviewer.Result.SetResult(new CommandApprovalAiReview(
            CommandApprovalAiDecisionKind.Approve,
            "Safe."));
        Assert.True((await first).Approved);
        Assert.True((await second).Approved);
        Assert.Equal(1, reviewer.CallCount);
    }

    [Fact]
    public async Task AiRememberAllowOnlyAppliesToExactScope()
    {
        var reviewer = new StubAiReviewer(new CommandApprovalAiReview(
            CommandApprovalAiDecisionKind.Approve,
            "Safe for this exact scope.",
            RememberForSession: true));
        var coordinator = new CommandApprovalCoordinator(new MemoryApprovalStore(), reviewer);
        await coordinator.SetModeAsync(ConnectorApprovalMode.AutoApproval);

        Assert.True((await coordinator.RequestAsync(Request("request-ai-6", "scope-a"), Risk())).Approved);
        var sameScope = await coordinator.RequestAsync(Request("request-ai-7", "scope-a"), Risk());
        Assert.Equal(ConnectorApprovalReviewer.Session, sameScope.Reviewer);
        await coordinator.RequestAsync(Request("request-ai-8", "scope-b"), Risk());
        Assert.Equal(2, reviewer.CallCount);
    }

    [Fact]
    public async Task CancellingAutomaticReviewDoesNotLeaveReviewOrPendingState()
    {
        var reviewer = new ControlledAiReviewer();
        var coordinator = new CommandApprovalCoordinator(new MemoryApprovalStore(), reviewer);
        await coordinator.SetModeAsync(ConnectorApprovalMode.AutoApproval);
        using var cancellation = new CancellationTokenSource();
        var request = coordinator.RequestAsync(Request("request-ai-9", "scope-ai"), Risk(), cancellation.Token);
        await reviewer.Started.Task.WaitAsync(TimeSpan.FromSeconds(2));

        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => request);
        Assert.Empty(coordinator.Snapshot());

        var retry = coordinator.RequestAsync(Request("request-ai-9", "scope-ai"), Risk());
        await WaitUntilAsync(() => reviewer.CallCount == 2);
        await coordinator.CancelAllAsync("cleanup");
        Assert.False((await retry).Approved);
    }

    [Theory]
    [InlineData("git status", ConnectorApprovalRiskLevel.Low)]
    [InlineData("git push origin main", ConnectorApprovalRiskLevel.Medium)]
    [InlineData("git reset --hard HEAD~1", ConnectorApprovalRiskLevel.High)]
    [InlineData("powershell -EncodedCommand AAAA", ConnectorApprovalRiskLevel.High)]
    public void RiskEvaluatorClassifiesCommands(string commandLine, ConnectorApprovalRiskLevel expected)
    {
        var parts = commandLine.Split(' ');
        var risk = new CommandRiskEvaluator().Evaluate(parts[0], parts[1..]);
        Assert.Equal(expected, risk.Level);
    }

    private static CommandApprovalRequest Request(string requestId, string scope) => new(
        requestId,
        "owner-1",
        "device-1",
        "workspace-1",
        "git",
        ["status"],
        "C:\\workspace",
        "test",
        scope);

    private static ConnectorApprovalRisk Risk() =>
        new(ConnectorApprovalRiskLevel.Low);

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition())
        {
            await Task.Delay(5, timeout.Token);
        }
    }

    internal sealed class MemoryApprovalStore : IConnectorApprovalStore
    {
        public ConnectorApprovalMode? Mode { get; private set; }

        public List<ConnectorApprovalHistoryEntry> History { get; } = [];

        public Task<ConnectorApprovalMode?> ReadModeAsync(
            CancellationToken cancellationToken = default) => Task.FromResult(Mode);

        public Task SaveModeAsync(
            ConnectorApprovalMode mode,
            CancellationToken cancellationToken = default)
        {
            Mode = mode;
            return Task.CompletedTask;
        }

        public Task AppendAsync(
            ConnectorApprovalHistoryEntry entry,
            CancellationToken cancellationToken = default)
        {
            lock (History)
            {
                History.Add(entry);
            }
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<ConnectorApprovalHistoryEntry>> ReadHistoryAsync(
            int limit = 1_000,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConnectorApprovalHistoryEntry>>(
                History.Take(limit).ToArray());
    }

    private sealed class BlockingApprovedHistoryStore : IConnectorApprovalStore
    {
        public TaskCompletionSource ApprovedAppendStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource ReleaseApprovedAppend { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Task<ConnectorApprovalMode?> ReadModeAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult<ConnectorApprovalMode?>(ConnectorApprovalMode.RequestApproval);

        public Task SaveModeAsync(
            ConnectorApprovalMode mode,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public async Task AppendAsync(
            ConnectorApprovalHistoryEntry entry,
            CancellationToken cancellationToken = default)
        {
            if (!entry.Approved)
            {
                return;
            }

            ApprovedAppendStarted.TrySetResult();
            await ReleaseApprovedAppend.Task.WaitAsync(cancellationToken);
        }

        public Task<IReadOnlyList<ConnectorApprovalHistoryEntry>> ReadHistoryAsync(
            int limit = 1_000,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConnectorApprovalHistoryEntry>>([]);
    }

    private sealed class StubAiReviewer : ICommandApprovalAiReviewer
    {
        private readonly CommandApprovalAiReview? _result;
        private readonly Exception? _exception;

        public StubAiReviewer(CommandApprovalAiReview result) => _result = result;

        public StubAiReviewer(Exception exception) => _exception = exception;

        public int CallCount { get; private set; }

        public Task<CommandApprovalAiReview> ReviewAsync(
            CommandApprovalRequest request,
            ConnectorApprovalRisk risk,
            CancellationToken cancellationToken = default)
        {
            CallCount++;
            return _exception is null
                ? Task.FromResult(_result!)
                : Task.FromException<CommandApprovalAiReview>(_exception);
        }
    }

    private sealed class ControlledAiReviewer : ICommandApprovalAiReviewer
    {
        public TaskCompletionSource Started { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource<CommandApprovalAiReview> Result { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public int CallCount { get; private set; }

        public async Task<CommandApprovalAiReview> ReviewAsync(
            CommandApprovalRequest request,
            ConnectorApprovalRisk risk,
            CancellationToken cancellationToken = default)
        {
            CallCount++;
            Started.TrySetResult();
            return await Result.Task.WaitAsync(cancellationToken);
        }
    }
}
