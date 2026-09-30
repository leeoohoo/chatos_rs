using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentConversationCommandTests
{
    [Fact]
    public async Task StartsTurnWithLocalModelAndCapabilityRevisions()
    {
        var host = new ConversationHost();
        var model = Model();
        var runtime = new WindowsLocalAgentConversationRuntimeSettingsService(
            new WindowsLocalAgentConversationRuntimeSettingsClient(host));
        var bootstrap = Bootstrap(model);
        runtime.Configure("user-1", bootstrap);
        var service = new WindowsLocalAgentConversationCommandService(
            new WindowsLocalAgentConversationClient(host),
            runtime);
        service.Configure("user-1", bootstrap);

        var result = await service.SendNewTurnAsync(new ConversationSendCommand(
            "conversation-1",
            "turn-1",
            "hello",
            [],
            true));

        Assert.True(result.Accepted);
        Assert.Equal("turn-1", result.TurnId);
        var start = Assert.IsType<StartLocalConversationTurnCommand>(host.LastStart);
        Assert.Equal("model-revision", start.ModelConfigRevision);
        Assert.Equal("capability-revision", start.CapabilityPolicyRevision);
        Assert.Equal((ulong)1, start.ExpectedConversationVersion);
        Assert.Equal("native_main_chat", start.MessageMetadata.GetProperty("source").GetString());
        Assert.True(start.MessageMetadata.GetProperty("reasoning_enabled").GetBoolean());
    }

    [Fact]
    public async Task RejectsAttachmentsBeforeAnyRemoteFallback()
    {
        var host = new ConversationHost();
        var model = Model();
        var runtime = new WindowsLocalAgentConversationRuntimeSettingsService(
            new WindowsLocalAgentConversationRuntimeSettingsClient(host));
        var bootstrap = Bootstrap(model);
        runtime.Configure("user-1", bootstrap);
        var service = new WindowsLocalAgentConversationCommandService(
            new WindowsLocalAgentConversationClient(host),
            runtime);
        service.Configure("user-1", bootstrap);
        var attachment = ConversationAttachmentDraft.Create(
            "note.txt",
            "text/plain",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.PastedText,
            "content"u8.ToArray());

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.SendNewTurnAsync(new ConversationSendCommand(
                "conversation-1",
                "turn-1",
                "hello",
                [attachment])));

        Assert.Contains("attachment authorization", error.Message);
        Assert.Null(host.LastStart);
    }

    private static WindowsLocalAgentBootstrapSnapshot Bootstrap(
        WindowsLocalAgentModelSnapshot model) => new(
            "user-1",
            [model],
            [new ConversationModelOption("model-1", "Model 1", "gpt-test", "medium")],
            new WindowsLocalAgentCapabilitySnapshot(
                "user-1",
                "main_chat",
                "capability-revision",
                null,
                [],
                []));

    private static WindowsLocalAgentModelSnapshot Model() => new(
        "user-1",
        "model-1",
        "model-revision",
        "env:CHATOS_LOCAL_AGENT_MODEL_MODEL_1",
        "https://example.invalid/v1",
        "gpt-test",
        "openai",
        true,
        false,
        null,
        null,
        null,
        "medium",
        false,
        null,
        null,
        null);

    private sealed class ConversationHost : ILocalAgentHostClient
    {
        public StartLocalConversationTurnCommand? LastStart { get; private set; }

        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            object response = command switch
            {
                GetLocalConversationRuntimeSettingsCommand =>
                    new LocalConversationRuntimeSettingsResult(
                        "conversation_runtime_settings",
                        new WindowsLocalAgentConversationRuntimeSettings(
                            "user-1",
                            "conversation-1",
                            "model-1",
                            "model-revision",
                            "medium",
                            null,
                            true,
                            1,
                            1_000)),
                GetLocalConversationCommand => throw new LocalAgentHostRequestException(
                    "not_found",
                    "missing",
                    false),
                CreateLocalConversationCommand create => new LocalConversationResult(
                    "conversation",
                    Detail(create.ConversationId)),
                StartLocalConversationTurnCommand start => Started(start),
                _ => throw new InvalidOperationException(
                    $"Unexpected command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }

        private LocalConversationTurnMutationResult Started(
            StartLocalConversationTurnCommand start)
        {
            LastStart = start;
            var conversation = Detail(start.ConversationId).Conversation with { Version = 2 };
            var turn = new WindowsLocalConversationTurnRecord(
                start.TurnId,
                start.ConversationId,
                start.MessageId,
                start.RunId,
                "running",
                1_000,
                1_000);
            return new(
                "conversation_turn_started",
                new WindowsLocalConversationTurnMutation(
                    conversation,
                    turn,
                    new WindowsLocalConversationMessageRecord(
                        start.MessageId,
                        start.ConversationId,
                        start.TurnId,
                        1,
                        "user",
                        JsonSerializer.SerializeToElement(start.Message),
                        start.MessageMetadata,
                        1_000),
                    []));
        }

        private static WindowsLocalConversationDetail Detail(string conversationId) => new(
            new WindowsLocalConversationRecord(
                conversationId,
                "user-1",
                "Conversation",
                1,
                1_000,
                1_000),
            [],
            [],
            []);
    }
}
