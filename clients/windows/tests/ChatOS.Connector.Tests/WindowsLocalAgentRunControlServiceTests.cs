using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentRunControlServiceTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    [Fact]
    public async Task FetchProjectsNeedsReviewReasonAndOnlyCurrentTaskRun()
    {
        var review = Run("run-main", "main_chat", "conversation", "thread-1",
            LocalAgentRunStatus.NeedsReview, 7, Review("batch-9"));
        var taskRun = Run("run-task", "task_runner", "task", "task-1",
            LocalAgentRunStatus.Paused, 4, Interaction("runtime_blocked"));
        var old = Run("run-old", "task_runner", "task", "task-1",
            LocalAgentRunStatus.ModelRunning, 2, null);
        var task = new LocalAgentTaskSnapshot("task-1", 2, "thread-1", "turn-task",
            "project-1", "run-old", "run-task", ["run-old", "run-task"], "Build", ["Done"],
            "paused", "model-1", 1, Now, Now);
        var store = await StoreAsync(
            [MainRecovered(review), Recovered(taskRun), Recovered(old)], [task]);
        var service = Service(store, new ControlClient(review));

        var controls = await service.FetchRunControlsAsync("thread-1");

        Assert.Equal(["run-main", "run-task"], controls.Select(value => value.RunId)
            .Order(StringComparer.Ordinal));
        var main = controls.Single(value => value.RunId == "run-main");
        Assert.True(main.CanResume);
        Assert.Contains("batch-9", main.ReviewReason);
        Assert.False(main.CanPause);
        Assert.Equal("runtime_blocked",
            controls.Single(value => value.RunId == "run-task").InteractionKind);
    }

    [Theory]
    [InlineData("pause", LocalAgentRunStatus.ModelRunning)]
    [InlineData("resume", LocalAgentRunStatus.NeedsReview)]
    [InlineData("cancel", LocalAgentRunStatus.Paused)]
    public async Task MutationSendsExactRunAndVersionThenRefreshes(
        string action, LocalAgentRunStatus status)
    {
        var run = Run("run-main", "main_chat", "conversation", "thread-1", status, 12,
            status == LocalAgentRunStatus.NeedsReview ? Review("batch-1") : null);
        var refreshed = run with
        {
            Status = action == "cancel" ? LocalAgentRunStatus.Cancelled : LocalAgentRunStatus.ModelReady,
            Version = 13,
            PendingInteraction = null,
            UpdatedAt = Now.AddMinutes(1),
        };
        var store = await StoreAsync([MainRecovered(run)], []);
        var client = new ControlClient(refreshed);
        var service = Service(store, client);

        if (action == "pause") await service.PauseRunAsync("run-main", "thread-1");
        else if (action == "resume") await service.ResumeRunAsync("run-main", "thread-1");
        else await service.CancelRunAsync("run-main", "thread-1");

        var command = JsonSerializer.SerializeToElement(client.AcceptedCommand, JsonOptions);
        Assert.Equal($"{action}_run", command.GetProperty("type").GetString());
        Assert.Equal("run-main", command.GetProperty("payload").GetProperty("run_id").GetString());
        Assert.Equal((ulong)12,
            command.GetProperty("payload").GetProperty("expected_version").GetUInt64());
        Assert.Equal((ulong)13, (await store.GetAsync())!.Runs["run-main"].Run.Version);
    }

    [Fact]
    public async Task AskUserPauseCannotBeResumedByRunControl()
    {
        var run = Run("run-main", "main_chat", "conversation", "thread-1",
            LocalAgentRunStatus.Paused, 4, Interaction("ask_user"));
        var store = await StoreAsync([MainRecovered(run)], []);
        var client = new ControlClient(run);
        var service = Service(store, client);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.ResumeRunAsync("run-main", "thread-1"));

        Assert.Null(client.AcceptedCommand);
    }

    [Fact]
    public async Task WrongConversationFailsBeforeCallingHost()
    {
        var run = Run("run-main", "main_chat", "conversation", "thread-1",
            LocalAgentRunStatus.ModelRunning, 4, null);
        var store = await StoreAsync([MainRecovered(run)], []);
        var client = new ControlClient(run);
        var service = Service(store, client);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.CancelRunAsync("run-main", "thread-other"));
        Assert.Null(client.AcceptedCommand);
    }

    private static WindowsLocalAgentRunControlService Service(
        IWindowsLocalAgentProjectionStore store, ILocalAgentIPCClient client)
    {
        var session = new AccountSession(client);
        return new WindowsLocalAgentRunControlService(store, session,
            new WindowsLocalAgentRunProjectionRefresher(store, session));
    }

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        IReadOnlyList<WindowsLocalAgentRecoveredRun> runs,
        IReadOnlyList<LocalAgentTaskSnapshot> tasks)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot("account-1",
            runs.ToDictionary(value => value.Run.RunId, StringComparer.Ordinal),
            tasks.ToDictionary(value => value.TaskId, StringComparer.Ordinal), 0,
            runs.Max(value => value.SnapshotEventSequence)));
        return store;
    }

    private static WindowsLocalAgentRecoveredRun MainRecovered(LocalAgentRunSnapshot run)
    {
        var message = new LocalAgentStoredMessage("message-1", run.RunId, "thread-1", "turn-main",
            1, LocalAgentStoredMessageRole.User, "Design", null, null, null, null,
            LocalAgentStoredMessageMode.Semantic, "main_chat", LocalAgentStoredMemorySyncStatus.Synced,
            Now);
        return new(run, Detail(run), new LocalAgentMainChatRunBinding(run.RunId, "thread-1",
            "turn-main", message.RecordId, message), run.Version);
    }

    private static WindowsLocalAgentRecoveredRun Recovered(LocalAgentRunSnapshot run) =>
        new(run, Detail(run), null, run.Version);
    private static LocalAgentRunDetail Detail(LocalAgentRunSnapshot run) =>
        new(run, [], [], 0, false, run.Version);

    private static LocalAgentRunSnapshot Run(string id, string profile, string ownerType,
        string ownerId, LocalAgentRunStatus status, ulong version, JsonElement? interaction) => new(
        id, profile, "account-1", ownerType, ownerId, "project-1", status, version, 2, 3, 1,
        "model-1", 1, JsonDocument.Parse("{}").RootElement.Clone(), "provider_compaction",
        "prompt-1", "capability-1", "batch-1", interaction, null, null, Now, Now);

    private static JsonElement Interaction(string type) =>
        JsonSerializer.SerializeToElement(new { type });
    private static JsonElement Review(string batch) =>
        JsonSerializer.SerializeToElement(new
        {
            type = "review_unknown_tool_outcome",
            batch_id = batch,
        });

    private sealed class ControlClient(LocalAgentRunSnapshot refreshed) : LocalAgentIPCClientStub
    {
        public LocalAgentCommand? AcceptedCommand { get; private set; }
        public override Task<string> AcceptAsync(LocalAgentCommand command,
            CancellationToken cancellationToken = default)
        { AcceptedCommand = command; return Task.FromResult("operation-1"); }
        public override Task<LocalAgentRunDetail> GetRunDetailAsync(string runId,
            uint eventLimit = 40, uint eventOffset = 0,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new LocalAgentRunDetail(refreshed, [], [], 0, false,
                refreshed.Version + 10));
        public override Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(string runId,
            CancellationToken cancellationToken = default)
        {
            var message = new LocalAgentStoredMessage("message-1", runId, "thread-1", "turn-main",
                1, LocalAgentStoredMessageRole.User, "Design", null, null, null, null,
                LocalAgentStoredMessageMode.Semantic, "main_chat",
                LocalAgentStoredMemorySyncStatus.Synced, Now);
            return Task.FromResult(new LocalAgentMainChatRunBinding(runId, "thread-1", "turn-main",
                message.RecordId, message));
        }
    }

    private sealed class AccountSession(ILocalAgentIPCClient client) : IWindowsLocalAgentAccountSession
    {
        public Task<ILocalAgentIPCClient> GetClientAsync(string accountId,
            CancellationToken cancellationToken = default) => Task.FromResult(client);
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task UpdateAccessTokenAsync(string accountId, CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task LogoutAsync() => Task.CompletedTask;
        public Task<WindowsLocalAgentHostState> GetStateAsync() => throw new NotSupportedException();
        public Task<IReadOnlyList<LocalAgentAttachmentReference>> StageAttachmentsAsync(string accountId,
            IReadOnlyList<ConversationAttachmentDraft> attachments,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DiscardStagedAttachmentsAsync(string accountId,
            IReadOnlyList<LocalAgentAttachmentReference> references) => throw new NotSupportedException();
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }
}
