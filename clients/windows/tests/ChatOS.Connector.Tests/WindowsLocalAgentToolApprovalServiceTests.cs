using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentToolApprovalServiceTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    [Fact]
    public async Task FetchMapsOnlyAwaitingToolsFromMainChatAndCurrentTaskRuns()
    {
        var main = Run("run-main", "main_chat", "conversation", "thread-1", "project-1", 4);
        var taskRun = Run("run-task", "task_runner", "task", "task-1", "project-1", 5);
        var oldRun = Run("run-old", "task_runner", "task", "task-1", "project-1", 8);
        var task = TaskSnapshot("run-task", ["run-old", "run-task"]);
        var store = await StoreAsync(
            [RecoveredMain(main, Tool("invoke-main", main.RunId)),
             new WindowsLocalAgentRecoveredRun(
                 taskRun, Detail(taskRun, Tool("invoke-task", taskRun.RunId)), null, 5),
             new WindowsLocalAgentRecoveredRun(
                 oldRun, Detail(oldRun, Tool("invoke-old", oldRun.RunId)), null, 8)],
            [task]);
        var service = Service(store, new ApprovalClient(main));

        var approvals = await service.FetchPendingAsync("thread-1");

        Assert.Equal(["invoke-main", "invoke-task"], approvals
            .Select(value => value.InvocationId).Order(StringComparer.Ordinal));
        var mapped = approvals.Single(value => value.InvocationId == "invoke-main");
        Assert.Equal("turn-main", mapped.TurnId);
        Assert.Equal("filesystem.write", mapped.ToolName);
        Assert.Equal(LocalAgentToolEffect.Write, mapped.Effect);
        Assert.Equal("sha256:arguments", mapped.ArgumentsDigest);
    }

    [Theory]
    [InlineData(LocalAgentToolApprovalDecision.Approve, "  reviewed by user  ")]
    [InlineData(LocalAgentToolApprovalDecision.Reject, null)]
    public async Task DecisionUsesExactRunAndInvocationThenRefreshesAuthority(
        LocalAgentToolApprovalDecision decision,
        string? reason)
    {
        var run = Run("run-main", "main_chat", "conversation", "thread-1", "project-1", 4);
        var completedTool = Tool("invoke-main", run.RunId) with
        {
            Status = decision == LocalAgentToolApprovalDecision.Approve
                ? LocalAgentToolExecutionStatus.Approved
                : LocalAgentToolExecutionStatus.Rejected,
            ApprovalReason = reason?.Trim(),
            ApprovalDecidedAt = Now.AddMinutes(1),
        };
        var refreshed = run with { Version = 5, UpdatedAt = Now.AddMinutes(1) };
        var store = await StoreAsync([RecoveredMain(run, Tool("invoke-main", run.RunId))], []);
        var client = new ApprovalClient(refreshed, completedTool);
        var service = Service(store, client);

        await service.DecideAsync("invoke-main", "thread-1", decision, reason);

        var command = JsonSerializer.SerializeToElement(client.AcceptedCommand, JsonOptions);
        Assert.Equal("decide_tool_approval", command.GetProperty("type").GetString());
        var payload = command.GetProperty("payload");
        Assert.Equal("run-main", payload.GetProperty("run_id").GetString());
        Assert.Equal("invoke-main", payload.GetProperty("invocation_id").GetString());
        Assert.Equal(decision.ToString().ToLowerInvariant(),
            payload.GetProperty("decision").GetString());
        if (reason is null) Assert.Equal(JsonValueKind.Null, payload.GetProperty("reason").ValueKind);
        else Assert.Equal(reason.Trim(), payload.GetProperty("reason").GetString());
        Assert.Empty(await service.FetchPendingAsync("thread-1"));
    }

    [Fact]
    public async Task WrongConversationOrCompletedInvocationNeverCallsTheHost()
    {
        var run = Run("run-main", "main_chat", "conversation", "thread-1", "project-1", 4);
        var store = await StoreAsync([RecoveredMain(run, Tool("invoke-main", run.RunId))], []);
        var client = new ApprovalClient(run);
        var service = Service(store, client);

        await Assert.ThrowsAsync<InvalidOperationException>(() => service.DecideAsync(
            "invoke-main", "thread-other", LocalAgentToolApprovalDecision.Approve, null));
        await Assert.ThrowsAsync<InvalidOperationException>(() => service.DecideAsync(
            "invoke-stale", "thread-1", LocalAgentToolApprovalDecision.Approve, null));

        Assert.Null(client.AcceptedCommand);
    }

    [Fact]
    public async Task DecisionForCurrentTaskRunUsesItsFrozenSourceAndNeedsNoMainChatBinding()
    {
        var run = Run("run-task", "task_runner", "task", "task-1", "project-1", 4);
        var updated = run with { Version = 5, UpdatedAt = Now.AddMinutes(1) };
        var task = TaskSnapshot(run.RunId, [run.RunId]);
        var store = await StoreAsync(
            [new WindowsLocalAgentRecoveredRun(
                run, Detail(run, Tool("invoke-task", run.RunId)), null, 4)],
            [task]);
        var client = new ApprovalClient(updated);
        var service = Service(store, client);

        await service.DecideAsync(
            "invoke-task",
            "thread-1",
            LocalAgentToolApprovalDecision.Approve,
            "approved");

        var command = JsonSerializer.SerializeToElement(client.AcceptedCommand, JsonOptions);
        Assert.Equal("run-task", command.GetProperty("payload").GetProperty("run_id").GetString());
        Assert.Empty(await service.FetchPendingAsync("thread-1"));
    }

    [Fact]
    public async Task DuplicateInvocationIdentityFailsClosed()
    {
        var first = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 4);
        var second = Run("run-2", "main_chat", "conversation", "thread-1", "project-1", 5);
        var store = await StoreAsync(
            [RecoveredMain(first, Tool("invoke-duplicate", first.RunId), "turn-1", "message-1"),
             RecoveredMain(second, Tool("invoke-duplicate", second.RunId), "turn-2", "message-2")],
            []);
        var service = Service(store, new ApprovalClient(first));

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            service.FetchPendingAsync("thread-1"));
    }

    [Fact]
    public async Task ToolWhoseRunIdentityDoesNotMatchFailsClosed()
    {
        var run = Run("run-main", "main_chat", "conversation", "thread-1", "project-1", 4);
        var store = await StoreAsync(
            [RecoveredMain(run, Tool("invoke-main", "run-other"))], []);
        var service = Service(store, new ApprovalClient(run));

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            service.FetchPendingAsync("thread-1"));
    }

    [Fact]
    public async Task UnknownDecisionFailsBeforeCallingTheHost()
    {
        var run = Run("run-main", "main_chat", "conversation", "thread-1", "project-1", 4);
        var store = await StoreAsync([RecoveredMain(run, Tool("invoke-main", run.RunId))], []);
        var client = new ApprovalClient(run);
        var service = Service(store, client);

        await Assert.ThrowsAsync<ArgumentOutOfRangeException>(() => service.DecideAsync(
            "invoke-main", "thread-1", (LocalAgentToolApprovalDecision)42, null));

        Assert.Null(client.AcceptedCommand);
    }

    private static WindowsLocalAgentToolApprovalService Service(
        IWindowsLocalAgentProjectionStore store,
        ILocalAgentIPCClient client)
    {
        var session = new AccountSession(client);
        return new WindowsLocalAgentToolApprovalService(
            store, session, new WindowsLocalAgentRunProjectionRefresher(store, session));
    }

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        IReadOnlyList<WindowsLocalAgentRecoveredRun> runs,
        IReadOnlyList<LocalAgentTaskSnapshot> tasks)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1",
            runs.ToDictionary(value => value.Run.RunId, StringComparer.Ordinal),
            tasks.ToDictionary(value => value.TaskId, StringComparer.Ordinal),
            0,
            runs.Count == 0 ? 0 : runs.Max(value => value.SnapshotEventSequence)));
        return store;
    }

    private static WindowsLocalAgentRecoveredRun RecoveredMain(
        LocalAgentRunSnapshot run,
        LocalAgentToolSnapshot tool,
        string turnId = "turn-main",
        string messageId = "message-main")
    {
        var message = new LocalAgentStoredMessage(
            messageId, run.RunId, "thread-1", turnId, 1, LocalAgentStoredMessageRole.User,
            "Design", null, null, null, null, LocalAgentStoredMessageMode.Semantic,
            "main_chat", LocalAgentStoredMemorySyncStatus.Synced, Now);
        return new WindowsLocalAgentRecoveredRun(
            run,
            Detail(run, tool),
            new LocalAgentMainChatRunBinding(
                run.RunId, "thread-1", turnId, message.RecordId, message),
            run.Version);
    }

    private static LocalAgentRunDetail Detail(
        LocalAgentRunSnapshot run,
        params LocalAgentToolSnapshot[] tools) =>
        new(run, [], tools, 0, false, run.Version);

    private static LocalAgentToolSnapshot Tool(string invocationId, string runId) => new(
        invocationId, runId, "batch-1", "call-1", "filesystem.write",
        LocalAgentToolEffect.Write, "sha256:arguments",
        LocalAgentToolExecutionStatus.AwaitingApproval, null, null, null, null, null);

    private static LocalAgentTaskSnapshot TaskSnapshot(
        string currentRunId,
        IReadOnlyList<string> runIds) => new(
        "task-1", 2, "thread-1", "turn-source", "project-1", runIds[0], currentRunId,
        runIds, "Build", ["Done"], "running", "model-1", 3, Now, Now);

    private static LocalAgentRunSnapshot Run(
        string runId,
        string profile,
        string ownerType,
        string ownerId,
        string projectId,
        ulong version) => new(
        runId, profile, "account-1", ownerType, ownerId, projectId,
        LocalAgentRunStatus.WaitingToolResult, version, 2, 1, 0, "model-1", 3,
        JsonDocument.Parse("{\"provider\":\"openai\"}").RootElement.Clone(),
        "provider_compaction", "prompt-1", "capability-1", "batch-1", null, null,
        null, Now, Now);

    private sealed class ApprovalClient : LocalAgentIPCClientStub
    {
        private readonly LocalAgentRunSnapshot _refreshedRun;
        private readonly IReadOnlyList<LocalAgentToolSnapshot> _tools;

        public ApprovalClient(
            LocalAgentRunSnapshot refreshedRun,
            params LocalAgentToolSnapshot[] tools)
        {
            _refreshedRun = refreshedRun;
            _tools = tools;
        }

        public LocalAgentCommand? AcceptedCommand { get; private set; }

        public override Task<string> AcceptAsync(
            LocalAgentCommand command,
            CancellationToken cancellationToken = default)
        {
            AcceptedCommand = command;
            return Task.FromResult("operation-1");
        }

        public override Task<LocalAgentRunDetail> GetRunDetailAsync(
            string runId,
            uint eventLimit = 40,
            uint eventOffset = 0,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new LocalAgentRunDetail(
                _refreshedRun, [], _tools, 0, false, _refreshedRun.Version + 10));

        public override Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(
            string runId,
            CancellationToken cancellationToken = default)
        {
            var message = new LocalAgentStoredMessage(
                "message-main", runId, "thread-1", "turn-main", 1,
                LocalAgentStoredMessageRole.User, "Design", null, null, null, null,
                LocalAgentStoredMessageMode.Semantic, "main_chat",
                LocalAgentStoredMemorySyncStatus.Synced, Now);
            return Task.FromResult(new LocalAgentMainChatRunBinding(
                runId, "thread-1", "turn-main", message.RecordId, message));
        }
    }

    private sealed class AccountSession(ILocalAgentIPCClient client)
        : IWindowsLocalAgentAccountSession
    {
        public Task<ILocalAgentIPCClient> GetClientAsync(
            string accountId,
            CancellationToken cancellationToken = default) => Task.FromResult(client);
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task UpdateAccessTokenAsync(
            string accountId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task LogoutAsync() => Task.CompletedTask;
        public Task<WindowsLocalAgentHostState> GetStateAsync() => throw new NotSupportedException();
        public Task<IReadOnlyList<LocalAgentAttachmentReference>> StageAttachmentsAsync(
            string accountId,
            IReadOnlyList<ConversationAttachmentDraft> attachments,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DiscardStagedAttachmentsAsync(
            string accountId,
            IReadOnlyList<LocalAgentAttachmentReference> references) =>
            throw new NotSupportedException();
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }
}
