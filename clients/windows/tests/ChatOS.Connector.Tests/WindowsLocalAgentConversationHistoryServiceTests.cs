using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentConversationHistoryServiceTests
{
    [Fact]
    public async Task HistoryRestoresAdditionalGuidanceAndDurableMessageOrdinals()
    {
        var host = new HistoryHost { IncludeGuidance = true };
        var service = new WindowsLocalAgentConversationHistoryService(
            new WindowsLocalAgentConversationClient(host),
            new WindowsLocalAgentAttachmentVault(Path.Combine(
                Path.GetTempPath(), $"chatos-history-test-{Guid.NewGuid():N}")));
        service.Configure("owner-1");

        var page = await service.FetchHistoryAsync(new ConversationHistoryQuery(
            "conversation-1", 10, null, 1));

        var turn = Assert.Single(page.Turns);
        Assert.Equal("message-1", turn.UserMessage.Id);
        Assert.Equal((ulong)1, turn.UserMessage.StorageOrdinal);
        var guidance = Assert.Single(turn.AdditionalUserMessages);
        Assert.Equal("guidance-1", guidance.Id);
        Assert.Equal(ChatMessageRole.User, guidance.Role);
        Assert.Equal("Also verify Windows", guidance.Text);
        Assert.Equal((ulong)4, guidance.StorageOrdinal);
        var attachment = Assert.Single(guidance.Attachments);
        Assert.Equal("guidance-file", attachment.Id);
        Assert.Equal("notes.txt", attachment.Name);
        Assert.Empty(turn.UserMessage.Attachments);
        Assert.Equal(new ulong?[] { 2, 3 },
            turn.AssistantReplies.Select(reply => reply.Message.StorageOrdinal));
    }

    [Fact]
    public async Task HistoryMapsTaskCallbackMetadataAndMessageLookupLikeMacOS()
    {
        var host = new HistoryHost();
        var service = new WindowsLocalAgentConversationHistoryService(
            new WindowsLocalAgentConversationClient(host),
            new WindowsLocalAgentAttachmentVault(Path.Combine(
                Path.GetTempPath(),
                $"chatos-history-test-{Guid.NewGuid():N}")));
        service.Configure("owner-1");

        var page = await service.FetchHistoryAsync(new ConversationHistoryQuery(
            "conversation-1", 10, null, 1));

        var turn = Assert.Single(page.Turns);
        Assert.True(turn.IsTaskGraphAvailable);
        Assert.Equal(
            new MessageTaskLookup("conversation-1", "turn-1", "message-1"),
            turn.MessageTaskLookup);
        Assert.Equal("Started", turn.FinalAssistantMessage?.Text);
        Assert.Equal(2, turn.AssistantReplies.Count);
        Assert.Null(turn.AssistantReplies[0].TaskCallback);
        var callback = Assert.IsType<TaskExecutionCallbackReference>(
            turn.AssistantReplies[1].TaskCallback);
        Assert.Equal("task-1", callback.TaskId);
        Assert.Equal("run-1", callback.RunId);
        Assert.Equal("completed", callback.Status);
        Assert.Equal("Done", turn.AssistantReplies[1].Message.Text);
    }

    [Fact]
    public async Task HistoryKeepsOnlyLatestTaskGraphGenerationAndCompleteDelivery()
    {
        var host = new HistoryHost { IncludeTaskGraphGenerations = true };
        var service = new WindowsLocalAgentConversationHistoryService(
            new WindowsLocalAgentConversationClient(host),
            new WindowsLocalAgentAttachmentVault(Path.Combine(
                Path.GetTempPath(),
                $"chatos-history-test-{Guid.NewGuid():N}")));
        service.Configure("owner-1");

        var page = await service.FetchHistoryAsync(new ConversationHistoryQuery(
            "conversation-1", 10, null, 1));

        var turn = Assert.Single(page.Turns);
        Assert.Equal(
            new[] { "Started", "Evidence is incomplete", "# Complete delivery\n\nEvidence" },
            turn.AssistantReplies.Select(static reply => reply.Message.Text));
        Assert.Equal(
            new[]
            {
                "assistant-1",
                "task-graph:graph-b:1::task-b",
                "task-graph:graph-a:2",
            },
            turn.AssistantReplies.Select(static reply => reply.Message.Id));
        Assert.DoesNotContain(
            turn.AssistantReplies,
            static reply => reply.Message.Text == "Old blocked summary");
        Assert.Null(turn.AssistantReplies[0].TaskCallback);
        Assert.Equal("evidence_incomplete", turn.AssistantReplies[1].TaskCallback?.Status);
        Assert.Null(turn.AssistantReplies[2].TaskCallback);
        Assert.Equal("# Complete delivery\n\nEvidence", turn.FinalAssistantMessage?.Text);
    }

    private sealed class HistoryHost : ILocalAgentHostClient
    {
        public bool IncludeGuidance { get; init; }
        public bool IncludeTaskGraphGenerations { get; init; }

        public string? ActiveOwnerUserId => "owner-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            if (command is not GetLocalConversationHistoryCommand)
            {
                throw new InvalidOperationException($"Unexpected command: {typeof(TCommand).Name}");
            }

            var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            var assistantMessages = IncludeTaskGraphGenerations
                ? TaskGraphGenerationMessages(now)
                :
                [
                    new WindowsLocalConversationMessageRecord(
                        "assistant-1", "conversation-1", "turn-1", 2, "assistant",
                        JsonSerializer.SerializeToElement(new { content = "Started" }),
                        JsonSerializer.SerializeToElement(new { }), now),
                    new WindowsLocalConversationMessageRecord(
                        "callback-1", "conversation-1", "turn-1", 3, "assistant",
                        JsonSerializer.SerializeToElement("Done"),
                        JsonSerializer.SerializeToElement(new
                        {
                            task_runner_async = new
                            {
                                task_id = "task-1",
                                run_id = "run-1",
                                @event = "task.completed",
                                status = "succeeded",
                                source_session_id = "conversation-1",
                                source_turn_id = "turn-1",
                                source_user_message_id = "message-1",
                            },
                        }), now),
                ];
            var page = new WindowsLocalConversationHistoryPage(
                new WindowsLocalConversationRecord(
                    "conversation-1", "owner-1", "Conversation", 5, now, now),
                [new WindowsLocalConversationTurnRecord(
                    "turn-1", "conversation-1", "message-1", "main-run-1",
                    "succeeded", now, now)],
                [
                    .. IncludeGuidance
                        ? new[]
                        {
                            new WindowsLocalConversationMessageRecord(
                                "guidance-1", "conversation-1", "turn-1", 4, "user",
                                JsonSerializer.SerializeToElement(new { text = "Also verify Windows" }),
                                JsonSerializer.SerializeToElement(new { }), now),
                        }
                        : Array.Empty<WindowsLocalConversationMessageRecord>(),
                    new WindowsLocalConversationMessageRecord(
                        "message-1", "conversation-1", "turn-1", 1, "user",
                        JsonSerializer.SerializeToElement(new { text = "Please test" }),
                        JsonSerializer.SerializeToElement(new { }), now),
                    .. assistantMessages,
                ],
                IncludeGuidance
                    ? [new WindowsLocalConversationAttachmentRecord(
                        "guidance-file", "conversation-1", "turn-1", "guidance-1", 5,
                        "notes.txt", "text/plain", 12, "unavailable", "unavailable",
                        JsonSerializer.SerializeToElement(new { }), now)]
                    : [],
                null);
            object response = new LocalConversationHistoryResult("conversation_history", page);
            return Task.FromResult((TResponse)response);
        }

        private static WindowsLocalConversationMessageRecord[] TaskGraphGenerationMessages(long now) =>
        [
            new(
                "task-graph:graph-a:1", "conversation-1", "turn-1", 2, "assistant",
                JsonSerializer.SerializeToElement(new
                {
                    type = "task_graph_terminal",
                    graph_id = "graph-a",
                    summary = "Old blocked summary",
                    tasks = new[] { new { task_id = "task-a", title = "Implement", status = "blocked" } },
                }),
                JsonSerializer.SerializeToElement(new { kind = "task_graph_terminal" }), now),
            new(
                "assistant-1", "conversation-1", "turn-1", 3, "assistant",
                JsonSerializer.SerializeToElement(new { content = "Started" }),
                JsonSerializer.SerializeToElement(new { }), now),
            new(
                "task-graph:graph-b:1", "conversation-1", "turn-1", 4, "assistant",
                JsonSerializer.SerializeToElement(new
                {
                    type = "task_graph_terminal",
                    graph_id = "graph-b",
                    tasks = new[]
                    {
                        new
                        {
                            task_id = "task-b",
                            title = "Verify",
                            status = "succeeded",
                            display_status = "evidence_incomplete",
                            terminal_outcome = "Evidence is incomplete",
                        },
                    },
                }),
                JsonSerializer.SerializeToElement(new { kind = "task_graph_terminal" }), now),
            new(
                "task-graph:graph-a:2", "conversation-1", "turn-1", 5, "assistant",
                JsonSerializer.SerializeToElement(new
                {
                    type = "task_graph_terminal",
                    summary = "Short preview",
                    delivery_text = "# Complete delivery\n\nEvidence",
                    tasks = new[] { new { task_id = "task-a", title = "Implement", status = "succeeded" } },
                }),
                JsonSerializer.SerializeToElement(new
                {
                    kind = "task_graph_terminal",
                    graph_id = "graph-a",
                }), now),
        ];
    }
}
