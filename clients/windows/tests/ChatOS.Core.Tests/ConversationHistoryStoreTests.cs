using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Core.Tests;

public sealed class ConversationHistoryStoreTests
{
    [Fact]
    public void OlderRevisionAndEqualRevisionCannotOverwriteAcceptedTurn()
    {
        var store = new ConversationHistoryStore();
        store.MergeCachedTurns(new[] { Turn("1", 1, 8, "accepted") }, "conversation-a");
        store.MergeCachedTurns(new[] { Turn("1", 1, 3, "older") }, "conversation-a");
        store.MergeCachedTurns(new[] { Turn("1", 1, 8, "conflicting replay") }, "conversation-a");

        var turn = Assert.Single(store.Snapshot("conversation-a").Turns);
        Assert.Equal(8, turn.Revision);
        Assert.Equal("accepted", turn.FinalAssistantMessage?.Text);
    }

    [Fact]
    public void StalePageGenerationCannotMoveCursorBackward()
    {
        var store = new ConversationHistoryStore();
        store.MergePage(
            new HistoryPage(new[] { Turn("2", 2) }, "new-cursor", true, 9, 12),
            "conversation-a");
        store.MergePage(
            new HistoryPage(new[] { Turn("1", 1) }, "stale-cursor", false, 4, 8),
            "conversation-a");

        var snapshot = store.Snapshot("conversation-a");
        Assert.Equal(new[] { "1", "2" }, snapshot.Turns.Select(static turn => turn.Id));
        Assert.Equal("new-cursor", snapshot.OlderCursor);
        Assert.True(snapshot.HasOlder);
    }

    [Fact]
    public void LatestRefreshDoesNotResetCursorAfterOlderPageLoaded()
    {
        var store = new ConversationHistoryStore();
        store.MergePage(
            new HistoryPage(new[] { Turn("11", 11), Turn("12", 12) }, "turn-11", true, 1, 1),
            "conversation-a");
        store.MergePage(
            new HistoryPage(new[] { Turn("9", 9), Turn("10", 10) }, "turn-9", true, 2, 2),
            "conversation-a",
            ConversationHistoryPageOrigin.Older);
        store.MergePage(
            new HistoryPage(new[] { Turn("12", 12, 2) }, "turn-11", true, 3, 3),
            "conversation-a");

        var snapshot = store.Snapshot("conversation-a");
        Assert.Equal("turn-9", snapshot.OlderCursor);
        Assert.Equal(new[] { "9", "10", "11", "12" }, snapshot.Turns.Select(static turn => turn.Id));
        Assert.Equal(2, snapshot.Turns[^1].Revision);
    }

    [Fact]
    public void MissingGlobalSequenceUsesCreationTimeAcrossPages()
    {
        var store = new ConversationHistoryStore();
        store.MergePage(
            new HistoryPage(
                new[] { Turn("latest-1", 1, timestamp: 300), Turn("latest-2", 2, timestamp: 400) },
                "older",
                true,
                1,
                1),
            "conversation-a");
        store.MergePage(
            new HistoryPage(
                new[] { Turn("older-1", 1, timestamp: 100), Turn("older-2", 2, timestamp: 200) },
                null,
                false,
                2,
                2),
            "conversation-a",
            ConversationHistoryPageOrigin.Older);

        Assert.Equal(
            new[] { "older-1", "older-2", "latest-1", "latest-2" },
            store.Snapshot("conversation-a").Turns.Select(static turn => turn.Id));
    }

    [Fact]
    public void RealtimeEventsAreDeduplicatedAndUnreadOnlyTracksChanges()
    {
        var store = new ConversationHistoryStore();
        var stable = Turn("1", 1, 3);

        store.ApplyRealtime(new RealtimeTurnEvent("event-1", 1, stable), true);
        store.ApplyRealtime(new RealtimeTurnEvent("event-1", 1, stable), true);
        store.ApplyRealtime(new RealtimeTurnEvent("event-2", 2, stable), true);

        var snapshot = store.Snapshot("conversation-a");
        Assert.Single(snapshot.Turns);
        Assert.Equal(1, snapshot.UnreadNewerCount);
    }

    [Fact]
    public void PinnedViewportClearsUnreadState()
    {
        var store = new ConversationHistoryStore();
        store.SetViewportAnchor(new ViewportAnchor("1", 0, false), "conversation-a");
        store.ApplyRealtime(new RealtimeTurnEvent("event-1", 1, Turn("1", 1)), true);
        Assert.Equal(1, store.Snapshot("conversation-a").UnreadNewerCount);

        store.SetViewportAnchor(new ViewportAnchor("1", 0, true), "conversation-a");

        Assert.Equal(0, store.Snapshot("conversation-a").UnreadNewerCount);
    }

    [Fact]
    public void OptimisticDiscardNeverDeletesPersistedTurn()
    {
        var store = new ConversationHistoryStore();
        store.MergeCachedTurns(
            new[] { Turn("optimistic", 1, 0), Turn("persisted", 2, 1) },
            "conversation-a");

        store.DiscardOptimisticTurn("conversation-a", "optimistic");
        store.DiscardOptimisticTurn("conversation-a", "persisted");

        Assert.Equal("persisted", Assert.Single(store.Snapshot("conversation-a").Turns).Id);
    }

    [Fact]
    public void ReplacementKeepsHistoryButDisablesSupersededTaskGraph()
    {
        var store = new ConversationHistoryStore();
        var old = Turn("old", 1) with
        {
            ProjectExecutionContext = new ProjectExecutionContext(ExecutionGroupId: "old"),
        };
        var replacement = Turn("new", 2) with
        {
            ProjectExecutionContext = new ProjectExecutionContext(
                ExecutionGroupId: "new",
                ReplacedExecutionGroupId: "old"),
        };

        store.MergeCachedTurns(new[] { old, replacement }, "conversation-a");

        var snapshot = store.Snapshot("conversation-a");
        Assert.False(snapshot.Turns[0].IsTaskGraphAvailable);
        Assert.True(snapshot.Turns[1].IsTaskGraphAvailable);
    }

    [Fact]
    public void TurnFromAnotherConversationCannotLeakIntoSnapshot()
    {
        var store = new ConversationHistoryStore();
        store.MergeCachedTurns(new[] { Turn("foreign", 1, conversationId: "conversation-b") }, "conversation-a");
        Assert.Empty(store.Snapshot("conversation-a").Turns);
    }

    private static ConversationTurn Turn(
        string id,
        long sequence,
        long revision = 1,
        string assistantText = "assistant",
        long? timestamp = null,
        string conversationId = "conversation-a")
    {
        var date = timestamp is null
            ? DateTimeOffset.FromUnixTimeSeconds(sequence)
            : DateTimeOffset.FromUnixTimeSeconds(timestamp.Value);
        return new ConversationTurn(
            id,
            conversationId,
            sequence,
            revision,
            new ChatMessage(
                $"user-{id}",
                ChatMessageRole.User,
                "user",
                date,
                Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<TurnProcessEvent>(),
            new ChatMessage(
                $"assistant-{id}",
                ChatMessageRole.Assistant,
                assistantText,
                date,
                Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<ConversationAssistantReply>(),
            null,
            true,
            TurnStatus.Completed,
            date,
            date);
    }
}
