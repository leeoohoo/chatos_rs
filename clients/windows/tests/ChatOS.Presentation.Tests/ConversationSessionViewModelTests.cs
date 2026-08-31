using System.Runtime.CompilerServices;
using System.Threading.Channels;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ConversationSessionViewModelTests
{
    [Fact]
    public async Task OpenLoadsCacheThenAuthoritativeHistoryRuntimeAndPrompts()
    {
        var cached = Turn("cached", 1, 1, "cached reply");
        var server = Turn("server", 2, 2, "server reply");
        var services = new TestServices
        {
            CachedTurns = new[] { cached },
            HistoryPage = new HistoryPage(new[] { server }, null, false, 2, 1),
            Prompts = new[] { Prompt() },
        };
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync("conversation-a", "项目会话");

        Assert.True(viewModel.IsOpen);
        Assert.False(viewModel.IsLoading);
        Assert.Equal("项目会话", viewModel.Title);
        Assert.Equal(new[] { "cached", "server" }, viewModel.Turns.Select(static turn => turn.Id));
        Assert.Single(viewModel.PendingPrompts);
        Assert.Equal("model-1", viewModel.SelectedModel?.Id);
        Assert.True(viewModel.ReasoningEnabled);
        Assert.True(viewModel.PlanModeEnabled);
        Assert.Equal(2, services.SavedTurns.Count);
    }

    [Fact]
    public async Task FailedSendRemovesOptimisticTurnAndRestoresDraft()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            SendException = new InvalidOperationException("send failed"),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.Draft = "请继续";

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal("请继续", viewModel.Draft);
        Assert.Empty(viewModel.Turns);
        Assert.Equal("send failed", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task ActiveTurnUsesGuidanceInsteadOfNewTurn()
    {
        var running = Turn("running", 1, 2, null, TurnStatus.Streaming);
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(new[] { running }, null, false, 2, 1),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.Draft = "补充要求";

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal(1, services.GuidanceCount);
        Assert.Equal(0, services.NewTurnCount);
        Assert.Equal("补充要求", services.LastCommand?.Content);
    }

    [Fact]
    public async Task AttachmentOnlyDraftIsSentAndAppearsInOptimisticTurn()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var attachment = ConversationAttachmentDraft.Create(
            "design.pdf",
            "application/pdf",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[] { 1, 2, 3 });
        viewModel.AddAttachments(new[] { attachment });

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal(1, services.NewTurnCount);
        Assert.Equal(string.Empty, services.LastCommand?.Content);
        Assert.Equal(attachment, Assert.Single(services.LastCommand!.Attachments));
        Assert.Empty(viewModel.Attachments);
    }

    [Fact]
    public async Task FailedAttachmentSendRestoresTextAndAttachment()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            SendException = new InvalidOperationException("upload failed"),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var attachment = ConversationAttachmentDraft.Create(
            "notes.md",
            "text/markdown",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[] { 1 });
        viewModel.Draft = "请查看";
        viewModel.AddAttachments(new[] { attachment });

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal("请查看", viewModel.Draft);
        Assert.Equal(attachment, Assert.Single(viewModel.Attachments));
        Assert.Equal("upload failed", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task AttachmentValidationRejectsOversizeFileWithoutAffectingAcceptedDrafts()
    {
        var services = new TestServices();
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(null, "Chat");
        var accepted = ConversationAttachmentDraft.Create(
            "small.txt",
            "text/plain",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[] { 1 });
        var rejected = ConversationAttachmentDraft.Create(
            "large.bin",
            "application/octet-stream",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[ConversationSessionViewModel.MaximumAttachmentBytes + 1]);

        viewModel.AddAttachments(new[] { accepted, rejected });

        Assert.Equal(accepted, Assert.Single(viewModel.Attachments));
        Assert.Contains("超过 20 MB", viewModel.AttachmentError);
    }

    [Fact]
    public async Task RealtimeProcessAppearsWithoutWaitingForHistoryRefresh()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");

        await services.Realtime.Writer.WriteAsync(new ConversationRealtimeSignal(
            "event-1",
            1,
            "conversation-a",
            "turn-1",
            ConversationRealtimeKind.Updated,
            "tool.started",
            DateTimeOffset.UtcNow,
            ProcessUpdate: new ConversationRealtimeProcessUpdate(
                "process-1",
                "正在调用工具：read_file",
                "正在读取上下文",
                "running",
                DateTimeOffset.UtcNow)));

        await WaitUntilAsync(() => viewModel.LiveProcesses.Count == 1);
        Assert.Contains("read_file", viewModel.LiveProcesses[0].Title);
    }

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition())
        {
            await Task.Delay(10, timeout.Token);
        }
    }

    private static ConversationTurn Turn(
        string id,
        long sequence,
        long revision,
        string? reply,
        TurnStatus status = TurnStatus.Completed) => new(
            id,
            "conversation-a",
            sequence,
            revision,
            new ChatMessage(
                $"user-{id}",
                ChatMessageRole.User,
                "message",
                DateTimeOffset.FromUnixTimeSeconds(sequence),
                Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<TurnProcessEvent>(),
            reply is null
                ? null
                : new ChatMessage(
                    $"assistant-{id}",
                    ChatMessageRole.Assistant,
                    reply,
                    DateTimeOffset.FromUnixTimeSeconds(sequence),
                    Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<ConversationAssistantReply>(),
            null,
            true,
            status,
            DateTimeOffset.FromUnixTimeSeconds(sequence),
            status == TurnStatus.Streaming ? null : DateTimeOffset.FromUnixTimeSeconds(sequence));

    private static AskUserPrompt Prompt() => new(
        "prompt-1",
        "conversation-a",
        "turn-1",
        null,
        "form",
        AskUserPromptStatus.Pending,
        "需要输入",
        "请输入环境",
        true,
        null,
        Array.Empty<AskUserField>(),
        null,
        null,
        null);

    private sealed class TestServices :
        IConversationHistoryService,
        IConversationCacheStore,
        IConversationCommandService,
        IConversationRuntimeSettingsService,
        IAskUserPromptService,
        IRealtimeClient
    {
        public IReadOnlyList<ConversationTurn> CachedTurns { get; init; } = Array.Empty<ConversationTurn>();

        public HistoryPage HistoryPage { get; init; } = new(
            Array.Empty<ConversationTurn>(), null, false, 0, 1);

        public IReadOnlyList<AskUserPrompt> Prompts { get; init; } = Array.Empty<AskUserPrompt>();

        public Exception? SendException { get; init; }

        public List<ConversationTurn> SavedTurns { get; } = [];

        public int NewTurnCount { get; private set; }

        public int GuidanceCount { get; private set; }

        public ConversationSendCommand? LastCommand { get; private set; }

        public Channel<ConversationRealtimeSignal> Realtime { get; } =
            Channel.CreateUnbounded<ConversationRealtimeSignal>();

        public ConversationSessionViewModel CreateViewModel() => new(
            this,
            this,
            this,
            this,
            this,
            this,
            new ConversationHistoryStore(),
            new ImmediateUiDispatcher());

        public Task<HistoryPage> FetchHistoryAsync(
            ConversationHistoryQuery query,
            CancellationToken cancellationToken = default) => Task.FromResult(
            HistoryPage with { RequestGeneration = query.RequestGeneration });

        public Task<IReadOnlyList<ConversationTurn>> LoadAsync(
            string conversationId,
            CancellationToken cancellationToken = default) => Task.FromResult(CachedTurns);

        public Task SaveAsync(
            string conversationId,
            IReadOnlyList<ConversationTurn> turns,
            CancellationToken cancellationToken = default)
        {
            SavedTurns.Clear();
            SavedTurns.AddRange(turns);
            return Task.CompletedTask;
        }

        public Task DeleteAsync(string conversationId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<ConversationCommandAck> SendNewTurnAsync(
            ConversationSendCommand command,
            CancellationToken cancellationToken = default)
        {
            NewTurnCount++;
            LastCommand = command;
            if (SendException is not null) return Task.FromException<ConversationCommandAck>(SendException);
            return Task.FromResult(new ConversationCommandAck(true, command.TurnId, "message-1"));
        }

        public Task<ConversationCommandAck> SendGuidanceAsync(
            ConversationSendCommand command,
            CancellationToken cancellationToken = default)
        {
            GuidanceCount++;
            LastCommand = command;
            if (SendException is not null) return Task.FromException<ConversationCommandAck>(SendException);
            return Task.FromResult(new ConversationCommandAck(true, command.TurnId, null));
        }

        public Task StopTurnAsync(
            string conversationId,
            string? turnId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<ConversationRuntimeSettings> FetchAsync(
            string conversationId,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new ConversationRuntimeSettings("model-1", "Model", "high", true, true));

        public Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
            CancellationToken cancellationToken = default) => Task.FromResult<IReadOnlyList<ConversationModelOption>>(
            new[] { new ConversationModelOption("model-1", "Model", "gpt-test", "high") });

        public Task<ConversationRuntimeSettings> UpdateModelAsync(
            string conversationId,
            string modelId,
            CancellationToken cancellationToken = default) => FetchAsync(conversationId, cancellationToken);

        public Task<ConversationRuntimeSettings> UpdatePlanModeAsync(
            string conversationId,
            bool enabled,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new ConversationRuntimeSettings("model-1", "Model", "high", true, enabled));

        public Task<ConversationRuntimeSettings> UpdateReasoningAsync(
            string conversationId,
            bool enabled,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new ConversationRuntimeSettings("model-1", "Model", "high", enabled, true));

        public Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
            string conversationId,
            int limit = 100,
            CancellationToken cancellationToken = default) => Task.FromResult(Prompts);

        public Task<AskUserPrompt> SubmitAsync(
            string promptId,
            string conversationId,
            AskUserSubmission submission,
            CancellationToken cancellationToken = default) => Task.FromResult(Prompt() with { Status = AskUserPromptStatus.Ok });

        public Task<AskUserPrompt> CancelAsync(
            string promptId,
            string conversationId,
            CancellationToken cancellationToken = default) => Task.FromResult(Prompt() with { Status = AskUserPromptStatus.Canceled });

        public async IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
            string conversationId,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await foreach (var signal in Realtime.Reader.ReadAllAsync(cancellationToken))
            {
                yield return signal;
            }
        }

        public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await Task.Delay(Timeout.Infinite, cancellationToken);
            yield break;
        }
    }
}
