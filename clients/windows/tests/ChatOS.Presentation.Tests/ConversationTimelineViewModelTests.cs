using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;

namespace ChatOS.Presentation.Tests;

public sealed class ConversationTimelineViewModelTests
{
    [Fact]
    public void TurnTaskGraphRequestFallsBackToMessageLookupBeforeCallbackArrives()
    {
        var lookup = new MessageTaskLookup("conversation-1", "turn-1", "message-1");
        var turn = new ConversationTurn(
            "turn-1",
            "conversation-1",
            1,
            2,
            new ChatMessage(
                "message-1",
                ChatMessageRole.User,
                "Create a task",
                DateTimeOffset.UtcNow,
                []),
            [],
            null,
            [],
            lookup,
            true,
            TurnStatus.Streaming,
            DateTimeOffset.UtcNow,
            null);

        var item = new ConversationTurnItemViewModel(turn);

        Assert.NotNull(item.TaskGraphRequest);
        Assert.Equal("message-1", item.TaskGraphRequest!.MessageId);
        Assert.Equal(string.Empty, item.TaskGraphRequest.TaskId);
        Assert.Equal(lookup, item.TaskGraphRequest.Lookup);
    }

    [Fact]
    public void ProcessEventsAreCollapsedUntilTheUserRequestsThem()
    {
        var item = new ConversationTurnItemViewModel(Turn(
            processEvents:
            [
                new TurnProcessEvent("event-1", "读取项目", "README.md", TurnStatus.Streaming),
            ]));

        Assert.True(item.HasProcessEvents);
        Assert.False(item.IsProcessExpanded);

        item.ToggleProcessCommand.Execute(null);

        Assert.True(item.IsProcessExpanded);
        item.ToggleProcessCommand.Execute(null);
        Assert.False(item.IsProcessExpanded);
    }

    [Fact]
    public void TimelineEvidenceUsesStableRuntimeIdentitiesAndStatuses()
    {
        var item = new ConversationTurnItemViewModel(Turn(
            processEvents:
            [
                new TurnProcessEvent("event-1", "读取项目", "README.md", TurnStatus.Completed),
            ],
            replies:
            [
                new ConversationAssistantReply(
                    Message("assistant", ChatMessageRole.Assistant),
                    new TaskExecutionCallbackReference(
                        "task-1",
                        "run-1",
                        "completed",
                        "completed",
                        "conversation-1",
                        "turn-1",
                        "user")),
            ]));

        Assert.Equal("ChatOS.Chat.Turn.turn-1", item.AutomationId);
        Assert.Equal("ChatOS.Chat.UserMessage.turn-1", item.UserMessageAutomationId);
        Assert.Equal("ChatOS.Chat.ProcessToggle.turn-1", item.ProcessToggleAutomationId);
        Assert.Equal("ChatOS.Chat.TaskGraph.Open.turn-1", item.TaskGraphAutomationId);
        Assert.Equal("turn=turn-1; revision=1; status=completed", item.AutomationStatus);
        Assert.Equal("ChatOS.Chat.LiveProcess.event-1", Assert.Single(item.ProcessEvents).LiveAutomationId);
        Assert.Equal("ChatOS.Chat.HistoryProcess.event-1", Assert.Single(item.ProcessEvents).HistoryAutomationId);
        var reply = Assert.Single(item.Replies);
        Assert.Equal("ChatOS.Chat.AssistantMessage.assistant", reply.AssistantAutomationId);
        Assert.Equal(
            "message=assistant; task=task-1; run=run-1; status=completed",
            reply.AutomationStatus);
    }

    [Fact]
    public void TaskCallbackProvidesTheUserMessageTaskGraphRequest()
    {
        var item = new ConversationTurnItemViewModel(Turn(
            replies:
            [
                new ConversationAssistantReply(
                    Message("assistant", ChatMessageRole.Assistant),
                    new TaskExecutionCallbackReference(
                        "task-1",
                        "run-1",
                        "completed",
                        "completed",
                        "conversation-1",
                        "turn-1",
                        "user")),
            ]));

        Assert.NotNull(item.TaskGraphRequest);
        Assert.Equal("user", item.TaskGraphRequest!.MessageId);
        Assert.Equal("task-1", item.TaskGraphRequest.TaskId);
        Assert.Equal("run-1", item.TaskGraphRequest.RunId);
    }

    [Fact]
    public void TaskReplyInspectorSwitchesSectionsAndCollapsesTheSelectedSection()
    {
        var reply = new ConversationReplyItemViewModel(
            "assistant",
            "已完成",
            DateTimeOffset.UtcNow,
            "task-1",
            "run-1",
            "completed",
            new MessageTaskGraphRequest(
                "user",
                "task-1",
                "run-1",
                new MessageTaskLookup("conversation-1", "turn-1", "user")));

        Assert.True(reply.BeginInspector(TaskReplyInspectorSection.Process));
        Assert.True(reply.IsProcessExpanded);
        Assert.True(reply.HasInspector);
        Assert.True(reply.BeginInspector(TaskReplyInspectorSection.Detail));
        Assert.True(reply.IsDetailExpanded);
        Assert.False(reply.BeginInspector(TaskReplyInspectorSection.Detail));
        Assert.False(reply.HasInspector);
    }

    [Fact]
    public void HistoricalImageAttachmentWithAViewUrlUsesTheInlinePreview()
    {
        var attachment = new ConversationAttachmentItemViewModel(
            new ConversationAttachmentReference(
                "image",
                "preview.png",
                "image/png",
                128,
                ConversationAttachmentKind.Image,
                ViewUrl: "file:///C:/ChatOS/preview.png",
                Sha256: "abc"));

        Assert.True(attachment.HasImagePreview);
        Assert.False(attachment.IsFileChip);
        Assert.Equal("128 B", attachment.SizeLabel);
    }

    private static ConversationTurn Turn(
        IReadOnlyList<TurnProcessEvent>? processEvents = null,
        IReadOnlyList<ConversationAssistantReply>? replies = null) => new(
            "turn-1",
            "conversation-1",
            1,
            1,
            Message("user", ChatMessageRole.User),
            processEvents ?? Array.Empty<TurnProcessEvent>(),
            null,
            replies ?? Array.Empty<ConversationAssistantReply>(),
            new MessageTaskLookup("conversation-1", "turn-1", "user"),
            replies?.Count > 0,
            TurnStatus.Completed,
            DateTimeOffset.UtcNow,
            DateTimeOffset.UtcNow);

    private static ChatMessage Message(string id, ChatMessageRole role) => new(
        id,
        role,
        role == ChatMessageRole.User ? "继续" : "已完成",
        DateTimeOffset.UtcNow,
        Array.Empty<ConversationAttachmentReference>());
}
