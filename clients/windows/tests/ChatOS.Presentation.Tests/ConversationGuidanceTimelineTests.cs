using System.Text.Json;
using System.Text.Json.Nodes;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;

namespace ChatOS.Presentation.Tests;

public sealed class ConversationGuidanceTimelineTests
{
    private static readonly DateTimeOffset Timestamp = DateTimeOffset.FromUnixTimeMilliseconds(1_000);

    [Fact]
    public void DurableOrdinalsDisambiguateGuidanceAndRepliesInTheSameMillisecond()
    {
        var item = Timeline(
            [Message("later-guidance", 5), Message("earlier-guidance", 2)],
            [Message("handoff", 3), Message("callback", 4)]);

        Assert.Equal(new[] { "earlier-guidance", "handoff", "callback", "later-guidance" }, Ids(item));
        Assert.Same(item.Replies[0], item.TimelineMessages[1]);
        Assert.Same(item.Replies[1], item.TimelineMessages[2]);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void LegacyMessagesUseTimestampsWhenEitherOrdinalIsMissing(bool userHasOrdinal)
    {
        var item = Timeline(
            [Message("before", userHasOrdinal ? 1UL : null, -1),
             Message("after", userHasOrdinal ? 99UL : null, 1)],
            [Message("reply", userHasOrdinal ? null : 5UL)]);

        Assert.Equal(new[] { "before", "reply", "after" }, Ids(item));
    }

    [Fact]
    public void DurableOrdinalsTakePriorityOverConflictingTimestamps()
    {
        var item = Timeline(
            [Message("later", 4, -10), Message("earlier", 1, 10)],
            [Message("reply", 3)]);

        Assert.Equal(new[] { "earlier", "reply", "later" }, Ids(item));
    }

    [Fact]
    public void EqualLegacyTimestampsKeepGuidanceOrderAndPlaceItBeforeReply()
    {
        var item = Timeline([Message("second"), Message("first")], [Message("reply")]);

        Assert.Equal(new[] { "second", "first", "reply" }, Ids(item));
    }

    [Fact]
    public void HandoffRemainsBeforeCallbacksEvenWhenTheirOrdinalsInterleave()
    {
        var item = Timeline([Message("guidance", 3)], [Message("handoff", 4), Message("callback", 2)]);

        Assert.Equal(new[] { "guidance", "handoff", "callback" }, Ids(item));
    }

    [Fact]
    public void FinalReplyFallbackAlsoInterleavesGuidance()
    {
        var turn = Turn([Message("before", 2), Message("after", 4)], []) with
        {
            FinalAssistantMessage = Message("final", 3),
        };

        Assert.Equal(new[] { "before", "final", "after" }, Ids(new ConversationTurnItemViewModel(turn)));
    }

    [Fact]
    public void GuidanceWithoutRepliesPreservesTextTimestampAndAttachments()
    {
        var attachment = new ConversationAttachmentReference(
            "file", "notes.txt", "text/plain", 5, ConversationAttachmentKind.File);
        var message = Message("guidance", 2) with { Text = "Continue\nwith this file", Attachments = [attachment] };
        var item = Timeline([message], []);

        var guidance = Assert.IsType<ConversationUserMessageItemViewModel>(Assert.Single(item.TimelineMessages));
        Assert.Equal("guidance", guidance.Id);
        Assert.Equal(message.Text, guidance.Text);
        Assert.Equal(message.CreatedAt, guidance.CreatedAt);
        Assert.Equal(attachment, Assert.Single(guidance.Attachments).Attachment);
        Assert.Empty(item.Replies);
    }

    [Fact]
    public void GuidanceAndOrdinalsSurviveTheHistorySerializationContract()
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        var turn = Turn([Message("guidance", 2)], [Message("reply", 3)]);

        var restored = JsonSerializer.Deserialize<ConversationTurn>(JsonSerializer.Serialize(turn, options), options)!;

        Assert.Equal((ulong)2, Assert.Single(restored.AdditionalUserMessages).StorageOrdinal);
        Assert.Equal(new[] { "guidance", "reply" }, Ids(new ConversationTurnItemViewModel(restored)));
    }

    [Fact]
    public void OlderHistoryWithoutGuidanceOrOrdinalsStillLoads()
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        var json = JsonSerializer.SerializeToNode(Turn([], [Message("reply")]), options)!.AsObject();
        json.Remove("additionalUserMessages");
        json["userMessage"]!.AsObject().Remove("storageOrdinal");

        var restored = json.Deserialize<ConversationTurn>(options)!;

        Assert.Empty(restored.AdditionalUserMessages);
        Assert.Null(restored.UserMessage.StorageOrdinal);
        Assert.Equal(new[] { "reply" }, Ids(new ConversationTurnItemViewModel(restored)));
    }

    private static string[] Ids(ConversationTurnItemViewModel item) => item.TimelineMessages.Select(value => value switch
    {
        ConversationUserMessageItemViewModel user => user.Id,
        ConversationReplyItemViewModel reply => reply.Id,
        _ => throw new InvalidOperationException("Unexpected timeline item."),
    }).ToArray();

    private static ConversationTurnItemViewModel Timeline(ChatMessage[] users, ChatMessage[] replies) =>
        new(Turn(users, replies));

    private static ConversationTurn Turn(ChatMessage[] users, ChatMessage[] replies) => new(
        "turn", "conversation", 1, 1, Message("primary", 1), [], null,
        replies.Select(message => new ConversationAssistantReply(message, null)).ToArray(),
        null, false, TurnStatus.Completed, Timestamp, Timestamp)
    {
        AdditionalUserMessages = users,
    };

    private static ChatMessage Message(string id, ulong? ordinal = null, int milliseconds = 0) =>
        new(id, ChatMessageRole.User, id, Timestamp.AddMilliseconds(milliseconds), [])
        {
            StorageOrdinal = ordinal,
        };
}
