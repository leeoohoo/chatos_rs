using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentAskUserPromptServiceTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");
    private static readonly JsonSerializerOptions CommandJsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    [Fact]
    public async Task FetchMapsTheExactPausedMainChatInteractionIncludingImages()
    {
        var run = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 7,
            Question("interaction-1", options: true));
        var store = await StoreAsync(RecoveredMainChat(run));
        var service = Service(store, new AskUserClient(run));

        var prompt = Assert.Single(await service.FetchPromptsAsync("thread-1"));

        Assert.Equal("interaction-1", prompt.Id);
        Assert.Equal("thread-1", prompt.ConversationId);
        Assert.Equal("turn-1", prompt.TurnId);
        Assert.Equal("visual_direction", prompt.Kind);
        Assert.Equal("选择视觉方向", prompt.Title);
        Assert.Equal("请选择首页的视觉方向", prompt.Message);
        Assert.False(prompt.AllowsCancel);
        Assert.True(prompt.Choice?.AllowsMultiple);
        Assert.Equal(2, prompt.Choice?.Options.Count);
        Assert.Equal(["reference://hero-a", "reference://hero-b"], prompt.ImageReferences);
    }

    [Fact]
    public async Task FetchMapsOnlyTheUniqueCurrentTaskRunToItsSourceConversation()
    {
        var current = Run("run-current", "task_runner", "task", "task-1", "project-1", 3,
            Question("interaction-task", options: false));
        var historical = Run("run-old", "task_runner", "task", "task-1", "project-1", 9,
            Question("interaction-old", options: false));
        var task = new LocalAgentTaskSnapshot(
            "task-1", 4, "thread-source", "turn-source", "project-1", "run-old",
            "run-current", ["run-old", "run-current"], "Design", ["Approved"], "paused",
            "model-1", 2, Now, Now);
        var store = await StoreAsync(
            new WindowsLocalAgentRecoveredRun(current, Detail(current), null, 3),
            new WindowsLocalAgentRecoveredRun(historical, Detail(historical), null, 9),
            task);
        var service = Service(store, new AskUserClient(current));

        var prompt = Assert.Single(await service.FetchPromptsAsync("thread-source"));

        Assert.Equal("interaction-task", prompt.Id);
        Assert.Equal("turn-source", prompt.TurnId);
    }

    [Fact]
    public async Task SubmitSendsExactRunInteractionAndAnswerThenRemovesPromptFromProjection()
    {
        var run = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 7,
            Question("interaction-1", options: true));
        var completed = run with
        {
            Status = LocalAgentRunStatus.ModelRunning,
            Version = 8,
            PendingInteraction = null,
            UpdatedAt = Now.AddMinutes(1),
        };
        var store = await StoreAsync(RecoveredMainChat(run));
        var client = new AskUserClient(completed);
        var service = Service(store, client);

        var result = await service.SubmitAsync(
            "interaction-1",
            "thread-1",
            new AskUserSubmission(
                new Dictionary<string, string> { ["answer"] = "  更克制一些  " },
                new AskUserSelection.Multiple(["editorial", "minimal"])));

        Assert.Equal(AskUserPromptStatus.Ok, result.Status);
        var command = JsonSerializer.SerializeToElement(client.AcceptedCommand, CommandJsonOptions);
        Assert.Equal("answer_user_question", command.GetProperty("type").GetString());
        var payload = command.GetProperty("payload");
        Assert.Equal("run-1", payload.GetProperty("run_id").GetString());
        Assert.Equal("interaction-1", payload.GetProperty("interaction_id").GetString());
        Assert.Equal("更克制一些", payload.GetProperty("answer").GetProperty("text").GetString());
        Assert.Equal(["editorial", "minimal"], payload.GetProperty("answer")
            .GetProperty("selected_option_ids").EnumerateArray().Select(value => value.GetString()));
        Assert.Empty(await service.FetchPromptsAsync("thread-1"));
        Assert.Equal((ulong)8, (await store.GetAsync())!.Runs["run-1"].Run.Version);
    }

    [Fact]
    public async Task CancelSendsExactRunVersionAndRefreshesAuthoritativeState()
    {
        var run = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 11,
            Question("interaction-1", options: false, allowsCancel: true));
        var cancelled = run with
        {
            Status = LocalAgentRunStatus.Cancelled,
            Version = 12,
            PendingInteraction = null,
            UpdatedAt = Now.AddMinutes(1),
        };
        var store = await StoreAsync(RecoveredMainChat(run));
        var client = new AskUserClient(cancelled);
        var service = Service(store, client);

        var result = await service.CancelAsync("interaction-1", "thread-1");

        Assert.Equal(AskUserPromptStatus.Canceled, result.Status);
        var command = JsonSerializer.SerializeToElement(client.AcceptedCommand, CommandJsonOptions);
        Assert.Equal("cancel_run", command.GetProperty("type").GetString());
        Assert.Equal("run-1", command.GetProperty("payload").GetProperty("run_id").GetString());
        Assert.Equal((ulong)11,
            command.GetProperty("payload").GetProperty("expected_version").GetUInt64());
        Assert.Empty(await service.FetchPromptsAsync("thread-1"));
    }

    [Theory]
    [MemberData(nameof(InvalidSubmissions))]
    public async Task InvalidSubmissionFailsBeforeCallingTheHost(AskUserSubmission submission)
    {
        var run = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 7,
            Question("interaction-1", options: true));
        var store = await StoreAsync(RecoveredMainChat(run));
        var client = new AskUserClient(run);
        var service = Service(store, client);

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            service.SubmitAsync("interaction-1", "thread-1", submission));

        Assert.Null(client.AcceptedCommand);
    }

    public static IEnumerable<object[]> InvalidSubmissions()
    {
        yield return [new AskUserSubmission(new Dictionary<string, string>())];
        yield return [new AskUserSubmission(new Dictionary<string, string>(),
            new AskUserSelection.Multiple(["editorial", "editorial"]))];
        yield return [new AskUserSubmission(new Dictionary<string, string>(),
            new AskUserSelection.Single("not-allowed"))];
    }

    [Fact]
    public async Task WrongConversationOrStaleInteractionFailsClosed()
    {
        var run = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 7,
            Question("interaction-1", options: false));
        var store = await StoreAsync(RecoveredMainChat(run));
        var client = new AskUserClient(run);
        var service = Service(store, client);

        await Assert.ThrowsAsync<InvalidOperationException>(() => service.SubmitAsync(
            "interaction-1", "thread-other",
            new AskUserSubmission(new Dictionary<string, string> { ["answer"] = "yes" })));
        await Assert.ThrowsAsync<InvalidOperationException>(() => service.SubmitAsync(
            "interaction-stale", "thread-1",
            new AskUserSubmission(new Dictionary<string, string> { ["answer"] = "yes" })));

        Assert.Null(client.AcceptedCommand);
    }

    [Fact]
    public async Task CrossAccountRunFailsClosedEvenWhenItsConversationMatches()
    {
        var run = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 7,
            Question("interaction-1", options: false)) with { OwnerUserId = "account-other" };
        var store = await StoreAsync(RecoveredMainChat(run));
        var service = Service(store, new AskUserClient(run));

        await Assert.ThrowsAsync<InvalidDataException>(() => service.FetchPromptsAsync("thread-1"));
    }

    [Fact]
    public async Task DuplicateInteractionIdentityAcrossRunsFailsClosed()
    {
        var first = Run("run-1", "main_chat", "conversation", "thread-1", "project-1", 7,
            Question("interaction-duplicate", options: false));
        var second = Run("run-2", "main_chat", "conversation", "thread-1", "project-1", 8,
            Question("interaction-duplicate", options: false));
        var firstRecovered = RecoveredMainChat(first);
        var secondRecovered = RecoveredMainChat(second, "turn-2", "message-2");
        var store = await StoreAsync(firstRecovered, secondRecovered);
        var service = Service(store, new AskUserClient(first));

        await Assert.ThrowsAsync<InvalidDataException>(() => service.FetchPromptsAsync("thread-1"));
    }

    private static WindowsLocalAgentAskUserPromptService Service(
        IWindowsLocalAgentProjectionStore store,
        ILocalAgentIPCClient client)
    {
        var session = new AccountSession(client);
        return new WindowsLocalAgentAskUserPromptService(
            store, session, new WindowsLocalAgentRunProjectionRefresher(store, session));
    }

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        WindowsLocalAgentRecoveredRun run,
        LocalAgentTaskSnapshot? task = null)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1",
            new Dictionary<string, WindowsLocalAgentRecoveredRun> { [run.Run.RunId] = run },
            task is null
                ? new Dictionary<string, LocalAgentTaskSnapshot>()
                : new Dictionary<string, LocalAgentTaskSnapshot> { [task.TaskId] = task },
            0,
            run.SnapshotEventSequence));
        return store;
    }

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        WindowsLocalAgentRecoveredRun first,
        WindowsLocalAgentRecoveredRun second,
        LocalAgentTaskSnapshot task)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1",
            new Dictionary<string, WindowsLocalAgentRecoveredRun>
            {
                [first.Run.RunId] = first,
                [second.Run.RunId] = second,
            },
            new Dictionary<string, LocalAgentTaskSnapshot> { [task.TaskId] = task },
            0,
            Math.Max(first.SnapshotEventSequence, second.SnapshotEventSequence)));
        return store;
    }

    private static async Task<WindowsLocalAgentProjectionStore> StoreAsync(
        WindowsLocalAgentRecoveredRun first,
        WindowsLocalAgentRecoveredRun second)
    {
        var store = new WindowsLocalAgentProjectionStore();
        await store.ReplaceAsync(new WindowsLocalAgentProjectionSnapshot(
            "account-1",
            new Dictionary<string, WindowsLocalAgentRecoveredRun>
            {
                [first.Run.RunId] = first,
                [second.Run.RunId] = second,
            },
            new Dictionary<string, LocalAgentTaskSnapshot>(),
            0,
            Math.Max(first.SnapshotEventSequence, second.SnapshotEventSequence)));
        return store;
    }

    private static WindowsLocalAgentRecoveredRun RecoveredMainChat(
        LocalAgentRunSnapshot run,
        string turnId = "turn-1",
        string messageId = "message-1")
    {
        var message = new LocalAgentStoredMessage(
            messageId, run.RunId, "thread-1", turnId, 1,
            LocalAgentStoredMessageRole.User, "Design", null, null, null, null,
            LocalAgentStoredMessageMode.Semantic, "main_chat", LocalAgentStoredMemorySyncStatus.Synced,
            Now);
        return new WindowsLocalAgentRecoveredRun(
            run,
            Detail(run),
            new LocalAgentMainChatRunBinding(
                run.RunId, "thread-1", turnId, message.RecordId, message),
            run.Version);
    }

    private static LocalAgentRunDetail Detail(LocalAgentRunSnapshot run) =>
        new(run, [], [], 0, false, run.Version);

    private static LocalAgentRunSnapshot Run(
        string runId,
        string profileKey,
        string ownerEntityType,
        string ownerEntityId,
        string? projectId,
        ulong version,
        JsonElement pendingInteraction) => new(
        runId, profileKey, "account-1", ownerEntityType, ownerEntityId, projectId,
        LocalAgentRunStatus.Paused, version, 2, 1, 0, "model-1", 2,
        JsonDocument.Parse("{\"provider\":\"openai\"}").RootElement.Clone(),
        "provider_compaction", "prompt-1", "capability-1", "batch-1",
        pendingInteraction, null, null, Now, Now);

    private static JsonElement Question(
        string interactionId,
        bool options,
        bool allowsCancel = false)
    {
        var values = options
            ? "[{\"option_id\":\"editorial\",\"label\":\"编辑感\",\"description\":\"强排版\"},{\"option_id\":\"minimal\",\"label\":\"极简\",\"description\":null}]"
            : "[]";
        return JsonDocument.Parse($$"""
            {
              "type": "ask_user",
              "interaction_id": "{{interactionId}}",
              "question": {
                "prompt": "请选择首页的视觉方向",
                "options": {{values}},
                "image_references": ["reference://hero-a", "reference://hero-b"],
                "details": {
                  "title": "选择视觉方向",
                  "kind": "visual_direction",
                  "allows_cancel": {{allowsCancel.ToString().ToLowerInvariant()}},
                  "allows_multiple": {{options.ToString().ToLowerInvariant()}}
                }
              }
            }
            """).RootElement.Clone();
    }

    private sealed class AskUserClient(LocalAgentRunSnapshot refreshedRun)
        : LocalAgentIPCClientStub
    {
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
                refreshedRun, [], [], 0, false, refreshedRun.Version));

        public override Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(
            string runId,
            CancellationToken cancellationToken = default)
        {
            var message = new LocalAgentStoredMessage(
                "message-1", runId, "thread-1", "turn-1", 1,
                LocalAgentStoredMessageRole.User, "Design", null, null, null, null,
                LocalAgentStoredMessageMode.Semantic, "main_chat",
                LocalAgentStoredMemorySyncStatus.Synced, Now);
            return Task.FromResult(new LocalAgentMainChatRunBinding(
                runId, "thread-1", "turn-1", message.RecordId, message));
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
