using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentRunProjectionRefresherTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");

    [Fact]
    public async Task RefreshAtomicallyReplacesTheCompleteMainChatRunAndRevalidatesBinding()
    {
        var previous = Run(4);
        var updated = previous with
        {
            Status = LocalAgentRunStatus.ModelRunning,
            Version = 5,
            UpdatedAt = Now.AddMinutes(1),
        };
        var store = await StoreAsync(previous);
        var client = new RefreshClient(updated, Binding(updated));
        var changed = 0;
        store.Changed += (_, _) => changed++;
        var refresher = new WindowsLocalAgentRunProjectionRefresher(
            store, new AccountSession(client));

        var recovered = await refresher.RefreshAsync("account-1", "run-1");

        Assert.Equal((ulong)5, recovered.Run.Version);
        Assert.Equal(2, recovered.Detail?.Events.Count);
        Assert.Equal("turn-1", recovered.MainChatBinding?.TurnId);
        Assert.Equal(1, changed);
        var projected = (await store.GetAsync())!.Runs["run-1"];
        Assert.Equal(LocalAgentRunStatus.ModelRunning, projected.Run.Status);
        Assert.Equal((ulong)15, projected.SnapshotEventSequence);
    }

    [Theory]
    [InlineData("project")]
    [InlineData("model")]
    [InlineData("prompt")]
    [InlineData("capability")]
    [InlineData("context")]
    public async Task RefreshRejectsFrozenIdentityChangesWithoutMutatingProjection(string field)
    {
        var previous = Run(4);
        var changed = (field switch
        {
            "project" => previous with { ProjectId = "project-other" },
            "model" => previous with { ModelConfigId = "model-other" },
            "prompt" => previous with { PromptRevision = "prompt-other" },
            "capability" => previous with { CapabilitySnapshotRef = "capability-other" },
            "context" => previous with { ContextStrategy = "memory_engine" },
            _ => throw new InvalidOperationException(),
        }) with { Version = 5, UpdatedAt = Now.AddMinutes(1) };
        var store = await StoreAsync(previous);
        var refresher = new WindowsLocalAgentRunProjectionRefresher(
            store, new AccountSession(new RefreshClient(changed, Binding(changed))));

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            refresher.RefreshAsync("account-1", "run-1"));

        var projected = (await store.GetAsync())!.Runs["run-1"];
        Assert.Equal(previous, projected.Run);
    }

    [Fact]
    public async Task StoreIgnoresLateOlderAuthorityWithoutPublishingAChange()
    {
        var current = Run(8) with
        {
            Status = LocalAgentRunStatus.ModelRunning,
            UpdatedAt = Now.AddMinutes(2),
        };
        var stale = Run(7) with { UpdatedAt = Now.AddMinutes(1) };
        var store = await StoreAsync(current);
        var changed = 0;
        store.Changed += (_, _) => changed++;

        await store.ReplaceAuthoritativeRunAsync(
            "account-1",
            new WindowsLocalAgentRecoveredRun(stale, Detail(stale), Binding(stale), 7));

        Assert.Equal(0, changed);
        Assert.Equal((ulong)8, (await store.GetAsync())!.Runs["run-1"].Run.Version);
    }

    [Fact]
    public async Task StoreIgnoresSameVersionDetailWithAnOlderSnapshotWatermark()
    {
        var current = Run(8) with
        {
            Status = LocalAgentRunStatus.ModelRunning,
            UpdatedAt = Now.AddMinutes(2),
        };
        var store = await StoreAsync(current);
        var changed = 0;
        store.Changed += (_, _) => changed++;

        await store.ReplaceAuthoritativeRunAsync(
            "account-1",
            new WindowsLocalAgentRecoveredRun(current, Detail(current), Binding(current), 7));

        Assert.Equal(0, changed);
        Assert.Equal((ulong)8, (await store.GetAsync())!.Runs["run-1"].SnapshotEventSequence);
    }

    [Fact]
    public async Task StoreRejectsAChangedRunThatDidNotAdvanceItsVersion()
    {
        var current = Run(8);
        var changedRun = current with { Status = LocalAgentRunStatus.ModelRunning };
        var store = await StoreAsync(current);

        await Assert.ThrowsAsync<InvalidDataException>(() => store.ReplaceAuthoritativeRunAsync(
            "account-1",
            new WindowsLocalAgentRecoveredRun(
                changedRun, Detail(changedRun), Binding(changedRun), 9)));

        Assert.Equal(LocalAgentRunStatus.Paused,
            (await store.GetAsync())!.Runs["run-1"].Run.Status);
    }

    [Fact]
    public async Task RefreshRejectsChangedMainChatSourceBinding()
    {
        var previous = Run(4);
        var updated = previous with { Version = 5, UpdatedAt = Now.AddMinutes(1) };
        var changedBinding = Binding(updated) with { TurnId = "turn-other" };
        var store = await StoreAsync(previous);
        var refresher = new WindowsLocalAgentRunProjectionRefresher(
            store, new AccountSession(new RefreshClient(updated, changedBinding)));

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            refresher.RefreshAsync("account-1", "run-1"));

        Assert.Equal((ulong)4, (await store.GetAsync())!.Runs["run-1"].Run.Version);
    }

    [Fact]
    public async Task RefreshRejectsAnotherAccountBeforeOpeningTheHostClient()
    {
        var previous = Run(4);
        var store = await StoreAsync(previous);
        var session = new AccountSession(new RefreshClient(previous, Binding(previous)));
        var refresher = new WindowsLocalAgentRunProjectionRefresher(store, session);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            refresher.RefreshAsync("account-other", "run-1"));

        Assert.Equal(0, session.ClientRequests);
    }

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        LocalAgentRunSnapshot run)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1",
            new Dictionary<string, WindowsLocalAgentRecoveredRun>
            {
                [run.RunId] = new(run, Detail(run), Binding(run), run.Version),
            },
            new Dictionary<string, LocalAgentTaskSnapshot>(),
            0,
            run.Version));
        return store;
    }

    private static LocalAgentRunSnapshot Run(ulong version) => new(
        "run-1", "main_chat", "account-1", "conversation", "thread-1", "project-1",
        LocalAgentRunStatus.Paused, version, 2, 1, 0, "model-1", 3,
        JsonDocument.Parse("{\"provider\":\"openai\",\"model\":\"gpt\"}")
            .RootElement.Clone(),
        "provider_compaction", "prompt-1", "capability-1", "batch-1",
        JsonDocument.Parse("{\"type\":\"ask_user\"}").RootElement.Clone(),
        null, null, Now, Now);

    private static LocalAgentRunDetail Detail(LocalAgentRunSnapshot run) =>
        new(run, [], [], 0, false, run.Version);

    private static LocalAgentMainChatRunBinding Binding(LocalAgentRunSnapshot run)
    {
        var message = new LocalAgentStoredMessage(
            "message-1", run.RunId, "thread-1", "turn-1", 1,
            LocalAgentStoredMessageRole.User, "Design", null, null, null, null,
            LocalAgentStoredMessageMode.Semantic, "main_chat", LocalAgentStoredMemorySyncStatus.Synced,
            Now);
        return new LocalAgentMainChatRunBinding(
            run.RunId, "thread-1", "turn-1", message.RecordId, message);
    }

    private sealed class RefreshClient(
        LocalAgentRunSnapshot run,
        LocalAgentMainChatRunBinding binding) : LocalAgentIPCClientStub
    {
        public override Task<LocalAgentRunDetail> GetRunDetailAsync(
            string runId,
            uint eventLimit = 40,
            uint eventOffset = 0,
            CancellationToken cancellationToken = default)
        {
            var events = new[]
            {
                new LocalAgentRunTimelineEvent("event-1", "model", "one", Now),
                new LocalAgentRunTimelineEvent("event-2", "model", "two", Now),
            };
            return Task.FromResult(eventOffset == 0
                ? new LocalAgentRunDetail(run, [events[0]], [], 2, true, 14)
                : new LocalAgentRunDetail(run, [events[1]], [], 2, false, 15));
        }

        public override Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(
            string runId,
            CancellationToken cancellationToken = default) => Task.FromResult(binding);
    }

    private sealed class AccountSession(ILocalAgentIPCClient client)
        : IWindowsLocalAgentAccountSession
    {
        public int ClientRequests { get; private set; }

        public Task<ILocalAgentIPCClient> GetClientAsync(
            string accountId,
            CancellationToken cancellationToken = default)
        {
            ClientRequests++;
            return Task.FromResult(client);
        }

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
