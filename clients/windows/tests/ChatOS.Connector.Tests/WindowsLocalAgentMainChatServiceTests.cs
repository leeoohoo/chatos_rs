using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentMainChatServiceTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");
    private static readonly LocalAgentConversationScope Scope =
        new("account-1", "thread-1", "project-1", "agent-1");
    private static readonly JsonSerializerOptions CommandJsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    [Fact]
    public async Task CreateFreezesAuthorityStagesAttachmentAndPublishesCompleteRunBeforeReturning()
    {
        var store = await EmptyStoreAsync();
        var client = new MainChatClient(Run("run-1", LocalAgentRunStatus.Queued, 1));
        var session = new MainChatAccountSession(client);
        using var service = CreateService(store, session);
        var attachment = ConversationAttachmentDraft.Create(
            "reference.png", "image/png", ConversationAttachmentKind.Image,
            ConversationAttachmentOrigin.PastedImage, [1, 2]);

        var created = await service.CreateTurnAsync(new LocalAgentCreateConversationTurn(
            Scope, "turn-1", "message-1", "  Refine the layout  ", [attachment]));

        Assert.Equal("run-1", created.Run.RunId);
        Assert.Equal("account-1", session.StagedAccountId);
        Assert.Equal(attachment, Assert.Single(session.StagedAttachments!));
        var command = Assert.IsType<LocalAgentCreateMainChatTurn>(client.CreateCommand);
        Assert.Equal("thread-1", command.ThreadId);
        Assert.Equal("project-1", command.ProjectId);
        Assert.Equal("model-1", command.ModelConfigId);
        Assert.Equal("Refine the layout", command.Content);
        Assert.Equal("project-1", command.ProjectSnapshot!.Payload
            .GetProperty("project_id").GetString());
        Assert.Equal(["ask_user", "create_local_task"], command.CapabilitySnapshot.Payload
            .GetProperty("allowed_tools").EnumerateArray().Select(value => value.GetString()));
        Assert.Single(command.Attachments);

        var projection = await store.GetAsync();
        var recovered = Assert.Single(projection!.Runs).Value;
        Assert.Equal("turn-1", recovered.MainChatBinding?.TurnId);
        Assert.NotNull(recovered.Detail);
        Assert.Equal((ulong)8, recovered.SnapshotEventSequence);
    }

    [Fact]
    public async Task HostFailureDiscardsOnlyTheStagedAttachmentGrants()
    {
        var store = await EmptyStoreAsync();
        var client = new MainChatClient(Run("run-1", LocalAgentRunStatus.Queued, 1))
        {
            CreateError = new IOException("pipe closed"),
        };
        var session = new MainChatAccountSession(client);
        using var service = CreateService(store, session);

        await Assert.ThrowsAsync<IOException>(() => service.CreateTurnAsync(
            new LocalAgentCreateConversationTurn(Scope, "turn-1", "message-1", "Design", [
                ConversationAttachmentDraft.Create("a.png", "image/png",
                    ConversationAttachmentKind.Image, ConversationAttachmentOrigin.File, [1])
            ])));

        Assert.Equal("account-1", session.DiscardedAccountId);
        Assert.Equal(session.StagedReferences, session.DiscardedReferences);
        Assert.Empty((await store.GetAsync())!.Runs);
    }

    [Fact]
    public async Task CreatedRunWithChangedProjectIdentityFailsClosed()
    {
        var store = await EmptyStoreAsync();
        var changed = Run("run-1", LocalAgentRunStatus.Queued, 1) with { ProjectId = "project-other" };
        using var service = CreateService(store, new MainChatAccountSession(new MainChatClient(changed)));

        await Assert.ThrowsAsync<InvalidDataException>(() => service.CreateTurnAsync(
            new LocalAgentCreateConversationTurn(Scope, "turn-1", "message-1", "Design", [])));

        Assert.Empty((await store.GetAsync())!.Runs);
    }

    [Fact]
    public async Task ExistingActiveRunPreventsConcurrentTurnBeforeAnyAttachmentIsStaged()
    {
        var active = Recovered(Run("run-existing", LocalAgentRunStatus.ModelRunning, 4),
            "turn-existing", "message-existing");
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1", new Dictionary<string, WindowsLocalAgentRecoveredRun>
            {
                [active.Run.RunId] = active,
            }, new Dictionary<string, LocalAgentTaskSnapshot>(), 0, 0));
        var session = new MainChatAccountSession(new MainChatClient(Run("run-new", LocalAgentRunStatus.Queued, 1)));
        using var service = CreateService(store, session);

        await Assert.ThrowsAsync<InvalidOperationException>(() => service.CreateTurnAsync(
            new LocalAgentCreateConversationTurn(Scope, "turn-2", "message-2", "Continue", [])));

        Assert.Null(session.StagedAttachments);
    }

    [Fact]
    public async Task ConversationProjectionRejectsCrossThreadBindingAndAttachesOnlyExactSourceTasks()
    {
        var recovered = Recovered(Run("run-1", LocalAgentRunStatus.Succeeded, 3),
            "turn-1", "message-1");
        var task = new LocalAgentTaskSnapshot(
            "task-1", 1, "thread-1", "turn-1", "project-1", "task-run-1", "task-run-1",
            ["task-run-1"], "Build", ["Done"], "done", "model-1", 1, Now, Now);
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1", new Dictionary<string, WindowsLocalAgentRecoveredRun> { ["run-1"] = recovered },
            new Dictionary<string, LocalAgentTaskSnapshot> { ["task-1"] = task }, 0, 0));
        using var service = CreateService(store,
            new MainChatAccountSession(new MainChatClient(recovered.Run)));

        var conversation = await service.GetConversationAsync("thread-1");
        Assert.Equal("task-1", Assert.Single(Assert.Single(conversation.Turns).Tasks).TaskId);

        await store.ReplaceAsync((await store.GetAsync())! with
        {
            Runs = new Dictionary<string, WindowsLocalAgentRecoveredRun>
            {
                ["run-1"] = recovered with
                {
                    MainChatBinding = recovered.MainChatBinding! with { ThreadId = "thread-other" }
                }
            }
        });
        await Assert.ThrowsAsync<InvalidDataException>(() => service.GetConversationAsync("thread-1"));
    }

    [Fact]
    public async Task CancelUsesExactProjectedRunAndVersion()
    {
        var recovered = Recovered(Run("run-1", LocalAgentRunStatus.ModelRunning, 11),
            "turn-1", "message-1");
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1", new Dictionary<string, WindowsLocalAgentRecoveredRun> { ["run-1"] = recovered },
            new Dictionary<string, LocalAgentTaskSnapshot>(), 0, 0));
        var client = new MainChatClient(recovered.Run);
        using var service = CreateService(store, new MainChatAccountSession(client));

        await service.CancelTurnAsync("thread-1", "turn-1", "run-1", 11);

        var payload = JsonSerializer.SerializeToElement(client.AcceptedCommand, CommandJsonOptions)
            .GetProperty("payload");
        Assert.Equal("run-1", payload.GetProperty("run_id").GetString());
        Assert.Equal((ulong)11, payload.GetProperty("expected_version").GetUInt64());
    }

    private static WindowsLocalAgentMainChatService CreateService(
        IWindowsLocalAgentProjectionStore store,
        IWindowsLocalAgentAccountSession session) => new(
        store,
        session,
        new RuntimeSettings(),
        new ContactContexts(),
        new Projects(),
        new WindowsLocalAgentMainChatSnapshotFactory());

    private static async Task<WindowsLocalAgentProjectionStore> EmptyStoreAsync()
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1", new Dictionary<string, WindowsLocalAgentRecoveredRun>(),
            new Dictionary<string, LocalAgentTaskSnapshot>(), 0, 0));
        return store;
    }

    private static WindowsLocalAgentRecoveredRun Recovered(
        LocalAgentRunSnapshot run, string turnId, string messageId)
    {
        var message = new LocalAgentStoredMessage(
            messageId, run.RunId, "thread-1", turnId, 1, LocalAgentStoredMessageRole.User,
            "Design", null, null, null, null, LocalAgentStoredMessageMode.Semantic, "main_chat",
            LocalAgentStoredMemorySyncStatus.Synced, Now);
        var detail = new LocalAgentRunDetail(run, [], [], 0, false, 8);
        return new WindowsLocalAgentRecoveredRun(
            run, detail, new LocalAgentMainChatRunBinding(
                run.RunId, "thread-1", turnId, messageId, message), 8);
    }

    private static LocalAgentRunSnapshot Run(
        string id, LocalAgentRunStatus status, ulong version) => new(
        id, "main_chat", "account-1", "conversation", "thread-1", "project-1",
        status, version, 0, 0, 0, "model-1", 1, EmptyJson(), "provider_compaction",
        "prompt-1", "capability-1", null, null, null, null, Now, Now);

    private static JsonElement EmptyJson() => JsonDocument.Parse("{}").RootElement.Clone();

    private sealed class MainChatClient(LocalAgentRunSnapshot createdRun) : LocalAgentIPCClientStub
    {
        public LocalAgentCreateMainChatTurn? CreateCommand { get; private set; }
        public LocalAgentCommand? AcceptedCommand { get; private set; }
        public Exception? CreateError { get; init; }

        public override Task<LocalAgentRunCreatedResponse> CreateMainChatTurnAsync(
            LocalAgentCreateMainChatTurn command, CancellationToken cancellationToken = default)
        {
            CreateCommand = command;
            return CreateError is null
                ? Task.FromResult(new LocalAgentRunCreatedResponse("operation-1", createdRun))
                : Task.FromException<LocalAgentRunCreatedResponse>(CreateError);
        }

        public override Task<LocalAgentRunDetail> GetRunDetailAsync(string runId,
            uint eventLimit = 40, uint eventOffset = 0,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new LocalAgentRunDetail(createdRun, [], [], 0, false, 8));

        public override Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(
            string runId, CancellationToken cancellationToken = default)
        {
            var turnId = CreateCommand?.TurnId ?? "turn-1";
            var messageId = CreateCommand?.MessageId ?? "message-1";
            var message = new LocalAgentStoredMessage(
                messageId, createdRun.RunId, createdRun.OwnerEntityId, turnId, 1,
                LocalAgentStoredMessageRole.User, CreateCommand?.Content, null, null, null, null,
                LocalAgentStoredMessageMode.Semantic, "main_chat", LocalAgentStoredMemorySyncStatus.Pending, Now);
            return Task.FromResult(new LocalAgentMainChatRunBinding(
                createdRun.RunId, createdRun.OwnerEntityId, turnId, messageId, message));
        }

        public override Task<string> AcceptAsync(
            LocalAgentCommand command, CancellationToken cancellationToken = default)
        {
            AcceptedCommand = command;
            return Task.FromResult("operation-cancel");
        }
    }

    private sealed class MainChatAccountSession(ILocalAgentIPCClient client)
        : IWindowsLocalAgentAccountSession
    {
        public string? StagedAccountId { get; private set; }
        public IReadOnlyList<ConversationAttachmentDraft>? StagedAttachments { get; private set; }
        public IReadOnlyList<LocalAgentAttachmentReference>? StagedReferences { get; private set; }
        public string? DiscardedAccountId { get; private set; }
        public IReadOnlyList<LocalAgentAttachmentReference>? DiscardedReferences { get; private set; }

        public Task<IReadOnlyList<LocalAgentAttachmentReference>> StageAttachmentsAsync(
            string accountId, IReadOnlyList<ConversationAttachmentDraft> attachments,
            CancellationToken cancellationToken = default)
        {
            StagedAccountId = accountId;
            StagedAttachments = attachments;
            StagedReferences = attachments.Select(value => new LocalAgentAttachmentReference(
                value.Id, value.MimeType, $"attachment-grant:grant-{Guid.NewGuid():D}",
                "sha256:digest", checked((ulong)value.Size))).ToArray();
            return Task.FromResult(StagedReferences);
        }

        public Task DiscardStagedAttachmentsAsync(
            string accountId, IReadOnlyList<LocalAgentAttachmentReference> references)
        {
            DiscardedAccountId = accountId;
            DiscardedReferences = references;
            return Task.CompletedTask;
        }

        public Task<ILocalAgentIPCClient> GetClientAsync(string accountId,
            CancellationToken cancellationToken = default) => Task.FromResult(client);
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task UpdateAccessTokenAsync(string accountId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task LogoutAsync() => Task.CompletedTask;
        public Task<WindowsLocalAgentHostState> GetStateAsync() => throw new NotSupportedException();
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private sealed class RuntimeSettings : IConversationRuntimeSettingsService
    {
        public Task<ConversationRuntimeSettings> FetchAsync(string conversationId,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new ConversationRuntimeSettings("model-1", "Model", "high", true));
        public Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<ConversationRuntimeSettings> UpdateModelAsync(string conversationId,
            string modelId, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<ConversationRuntimeSettings> UpdateReasoningAsync(string conversationId,
            bool enabled, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }

    private sealed class ContactContexts : ILocalAgentContactRuntimeContextService
    {
        public Task<LocalAgentContactRuntimeContext> FetchAsync(
            string agentId, CancellationToken cancellationToken = default) => Task.FromResult(
            new LocalAgentContactRuntimeContext(agentId, "Designer", "Visual expert", "design",
                "Create polished UI", [], "revision-1"));
    }

    private sealed class Projects : IProjectRegistry
    {
        public Task<LocalProjectRecord?> GetAsync(string ownerUserId, string id,
            CancellationToken cancellationToken = default) => Task.FromResult<LocalProjectRecord?>(new(
            id, ownerUserId, new LocalProjectDraft("Portfolio", "workspace-1", "site", "Calm"),
            2, LocalProjectStatus.Active, 1, 2));
        public Task<IReadOnlyList<LocalProjectRecord>> ListAsync(string ownerUserId,
            bool includeInactive = false, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public Task<LocalProjectRecord> CreateAsync(string ownerUserId, LocalProjectDraft draft,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<LocalProjectRecord> UpdateAsync(string ownerUserId, string id,
            long expectedRevision, LocalProjectDraft draft, LocalProjectStatus status,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
