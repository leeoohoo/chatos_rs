using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ConversationSessionViewModelTests
{
    private static readonly LocalAgentConversationScope ProjectScope =
        new("account-1", "thread-1", "project-1", "agent-1");

    [Fact]
    public async Task OpenConsumesOnlyTheAccountScopedLocalProjection()
    {
        var services = new TestServices();
        services.Turns.Add(services.Turn("turn-1", LocalAgentRunStatus.Succeeded, 3,
            "Make it calm", "The visual hierarchy is ready."));
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync(ProjectScope, "Website");

        Assert.True(viewModel.IsOpen);
        Assert.False(viewModel.IsLoading);
        Assert.Equal(ProjectScope, viewModel.Scope);
        var turn = Assert.Single(viewModel.Turns);
        Assert.Equal("Make it calm", turn.UserText);
        Assert.Equal("The visual hierarchy is ready.", Assert.Single(turn.Replies).Text);
        Assert.Equal("model-1", viewModel.SelectedModel?.Id);
        Assert.True(viewModel.ReasoningEnabled);
    }

    [Fact]
    public async Task SendPreservesFrozenScopeAndImmediatelyReadsCreatedRunProjection()
    {
        var services = new TestServices();
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(ProjectScope, "Website");
        viewModel.Draft = "Refine the hero";

        await viewModel.SendCommand.ExecuteAsync(null);

        var command = Assert.Single(services.CreatedTurns);
        Assert.Equal(ProjectScope, command.Scope);
        Assert.Equal("project-1", command.Scope.ProjectId);
        Assert.Equal("Refine the hero", command.Content);
        Assert.Equal(command.TurnId, Assert.Single(viewModel.Turns).Id);
        Assert.Empty(viewModel.Attachments);
    }

    [Fact]
    public async Task ActiveRunRejectsASecondTurnInsteadOfChangingItsMeaningToGuidance()
    {
        var services = new TestServices();
        services.Turns.Add(services.Turn("turn-running", LocalAgentRunStatus.ModelRunning, 7,
            "Initial request", null));
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(ProjectScope, "Website");
        viewModel.Draft = "This must remain a new turn";

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Empty(services.CreatedTurns);
        Assert.Equal("This must remain a new turn", viewModel.Draft);
        Assert.False(viewModel.CanSendDraft);
    }

    [Fact]
    public async Task StopUsesExactThreadTurnRunAndObservedVersion()
    {
        var services = new TestServices();
        services.Turns.Add(services.Turn("turn-running", LocalAgentRunStatus.WaitingToolResult, 19,
            "Build it", null));
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(ProjectScope, "Website");

        await viewModel.StopCommand.ExecuteAsync(null);

        Assert.Equal(("thread-1", "turn-running", "run-turn-running", (ulong)19), services.CancelledRun);
    }

    [Fact]
    public async Task ProjectionEventReplacesAssistantProcessAndTaskContentFromTheHostSnapshot()
    {
        var services = new TestServices();
        services.Turns.Add(services.Turn("turn-1", LocalAgentRunStatus.ModelRunning, 2,
            "Design", null));
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(ProjectScope, "Website");

        services.Turns[0] = services.Turn("turn-1", LocalAgentRunStatus.Succeeded, 5,
            "Design", "Finished", includeProcess: true, includeTask: true);
        services.PublishProjectionChanged();

        await WaitUntilAsync(() => viewModel.Turns.Single().RunVersion == 5);
        var turn = Assert.Single(viewModel.Turns);
        Assert.Equal("Finished", turn.Replies[0].Text);
        Assert.True(turn.IsTaskGraphAvailable);
        Assert.Equal("task-1", turn.Replies[1].TaskId);
        Assert.Single(turn.ProcessEvents);
    }

    [Fact]
    public async Task ProjectionClearImmediatelyDropsAccountDataAndClosesSession()
    {
        var services = new TestServices();
        services.Turns.Add(services.Turn("turn-1", LocalAgentRunStatus.Succeeded, 3, "Hi", "Done"));
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(ProjectScope, "Website");

        services.PublishProjectionCleared();

        await WaitUntilAsync(() => !viewModel.IsOpen);
        Assert.Null(viewModel.Scope);
        Assert.Empty(viewModel.Turns);
        Assert.Empty(viewModel.PendingPrompts);
    }

    [Fact]
    public async Task FailedAttachmentCreationRestoresDraftAndExactAttachment()
    {
        var services = new TestServices { CreateError = new IOException("Host unavailable") };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(ProjectScope, "Website");
        var attachment = ConversationAttachmentDraft.Create(
            "design.png", "image/png", ConversationAttachmentKind.Image,
            ConversationAttachmentOrigin.PastedImage, [1, 2, 3]);
        viewModel.Draft = "Use this reference";
        viewModel.AddAttachments([attachment]);

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal("Use this reference", viewModel.Draft);
        Assert.Equal(attachment, Assert.Single(viewModel.Attachments));
        Assert.Equal("Host unavailable", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task LateFailureFromClosedConversationCannotMutateTheNewConversationDraft()
    {
        var services = new TestServices
        {
            PendingCreate = new TaskCompletionSource<LocalAgentRunCreatedResponse>(
                TaskCreationOptions.RunContinuationsAsynchronously),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(ProjectScope, "First");
        viewModel.Draft = "old draft";
        var sending = viewModel.SendCommand.ExecuteAsync(null);
        await WaitUntilAsync(() => services.CreatedTurns.Count == 1);

        var next = new LocalAgentConversationScope("account-1", "thread-2", null, "agent-2");
        await viewModel.OpenAsync(next, "Second");
        services.PendingCreate.SetException(new IOException("late old failure"));
        await sending;

        Assert.Equal(next, viewModel.Scope);
        Assert.Equal(string.Empty, viewModel.Draft);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Empty(viewModel.Attachments);
    }

    [Fact]
    public async Task ProjectionIdentityMismatchFailsClosed()
    {
        var services = new TestServices { SnapshotAccountId = "another-account" };
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync(ProjectScope, "Website");

        Assert.Empty(viewModel.Turns);
        Assert.Contains("changed identity", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task AttachmentValidationUsesTheHostFiveMegabyteLimit()
    {
        var services = new TestServices();
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(null, "ChatOS");
        var tooLarge = ConversationAttachmentDraft.Create(
            "large.bin", "application/octet-stream", ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[ConversationSessionViewModel.MaximumAttachmentBytes + 1]);

        viewModel.AddAttachments([tooLarge]);

        Assert.Empty(viewModel.Attachments);
        Assert.Contains("5 MB", viewModel.AttachmentError);
    }

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition()) await Task.Delay(10, timeout.Token);
    }

    private sealed class TestServices :
        ILocalAgentMainChatService,
        IConversationRuntimeSettingsService,
        IAskUserPromptService
    {
        private readonly DateTimeOffset _now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");

        public event EventHandler? ProjectionChanged;
        public event EventHandler? ProjectionCleared;
        public List<LocalAgentMainChatTurn> Turns { get; } = [];
        public List<LocalAgentCreateConversationTurn> CreatedTurns { get; } = [];
        public Exception? CreateError { get; init; }
        public TaskCompletionSource<LocalAgentRunCreatedResponse>? PendingCreate { get; init; }
        public string SnapshotAccountId { get; init; } = "account-1";
        public (string ThreadId, string TurnId, string RunId, ulong Version)? CancelledRun { get; private set; }

        public ConversationSessionViewModel CreateViewModel() =>
            new(this, this, this, new ImmediateUiDispatcher());

        public void PublishProjectionChanged() => ProjectionChanged?.Invoke(this, EventArgs.Empty);
        public void PublishProjectionCleared() => ProjectionCleared?.Invoke(this, EventArgs.Empty);

        public Task<LocalAgentConversationSnapshot> GetConversationAsync(
            string threadId, CancellationToken cancellationToken = default) =>
            Task.FromResult(new LocalAgentConversationSnapshot(SnapshotAccountId, threadId, Turns.ToArray()));

        public Task<LocalAgentRunCreatedResponse> CreateTurnAsync(
            LocalAgentCreateConversationTurn command, CancellationToken cancellationToken = default)
        {
            CreatedTurns.Add(command);
            if (PendingCreate is not null) return PendingCreate.Task;
            if (CreateError is not null) return Task.FromException<LocalAgentRunCreatedResponse>(CreateError);
            var turn = Turn(command.TurnId, LocalAgentRunStatus.Queued, 1,
                command.Content ?? string.Empty, null);
            Turns.Add(turn);
            return Task.FromResult(new LocalAgentRunCreatedResponse("operation-1", turn.Run));
        }

        public Task CancelTurnAsync(string threadId, string turnId, string runId,
            ulong expectedVersion, CancellationToken cancellationToken = default)
        {
            CancelledRun = (threadId, turnId, runId, expectedVersion);
            return Task.CompletedTask;
        }

        public Task<ConversationRuntimeSettings> FetchAsync(
            string conversationId, CancellationToken cancellationToken = default) =>
            Task.FromResult(new ConversationRuntimeSettings("model-1", "Model", "high", true));

        public Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConversationModelOption>>([
                new("model-1", "Model", "gpt", "high")
            ]);

        public Task<ConversationRuntimeSettings> UpdateModelAsync(
            string conversationId, string modelId, CancellationToken cancellationToken = default) =>
            FetchAsync(conversationId, cancellationToken);

        public Task<ConversationRuntimeSettings> UpdateReasoningAsync(
            string conversationId, bool enabled, CancellationToken cancellationToken = default) =>
            Task.FromResult(new ConversationRuntimeSettings("model-1", "Model", "high", enabled));

        public Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
            string conversationId, int limit = 100, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<AskUserPrompt>>([]);

        public Task<AskUserPrompt> SubmitAsync(string promptId, string conversationId,
            AskUserSubmission submission, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task<AskUserPrompt> CancelAsync(string promptId, string conversationId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public LocalAgentMainChatTurn Turn(
            string turnId,
            LocalAgentRunStatus status,
            ulong version,
            string userText,
            string? assistantText,
            bool includeProcess = false,
            bool includeTask = false)
        {
            var run = new LocalAgentRunSnapshot(
                $"run-{turnId}", "main_chat", "account-1", "conversation", "thread-1",
                "project-1", status, version, 0, 0, 0, "model-1", 1, EmptyJson(),
                "provider_compaction", "prompt-1", "capability-1", null, null, null,
                null, _now, _now);
            var message = new LocalAgentStoredMessage(
                $"message-{turnId}", run.RunId, "thread-1", turnId, 1,
                LocalAgentStoredMessageRole.User, userText, null, null, null, null,
                LocalAgentStoredMessageMode.Semantic, "user", LocalAgentStoredMemorySyncStatus.Synced,
                _now);
            var events = new List<LocalAgentRunTimelineEvent>();
            if (includeProcess)
                events.Add(new LocalAgentRunTimelineEvent("event-tool", "tool_started", "render", _now));
            if (assistantText is not null)
                events.Add(new LocalAgentRunTimelineEvent(
                    "event-assistant", "message_assistant_content", assistantText, _now));
            var detail = new LocalAgentRunDetail(run, events, [], (uint)events.Count, false, version);
            IReadOnlyList<LocalAgentTaskSnapshot> tasks = includeTask
                ? [new LocalAgentTaskSnapshot(
                    "task-1", 1, "thread-1", turnId, "project-1", "task-run-1", "task-run-1",
                    ["task-run-1"], "Build the component", ["Looks correct"], "running",
                    "model-1", 1, _now, _now)]
                : [];
            return new LocalAgentMainChatTurn(
                run,
                new LocalAgentMainChatRunBinding(run.RunId, "thread-1", turnId, message.RecordId, message),
                detail,
                tasks);
        }
    }

    private static JsonElement EmptyJson() => JsonDocument.Parse("{}").RootElement.Clone();
}
