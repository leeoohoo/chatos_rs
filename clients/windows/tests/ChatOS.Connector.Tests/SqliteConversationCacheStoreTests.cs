using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class SqliteConversationCacheStoreTests
{
    [Fact]
    public async Task RoundTripsConversationTurnsAndCanDeleteSnapshot()
    {
        var directory = Path.Combine(
            Path.GetTempPath(),
            $"chatos-connector-tests-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteConversationCacheStore(database);
            var turn = Turn();

            await store.SaveAsync("conversation-a", new[] { turn });
            var loaded = Assert.Single(await store.LoadAsync("conversation-a"));

            Assert.Equal(turn.Id, loaded.Id);
            Assert.Equal(turn.FinalAssistantMessage?.Text, loaded.FinalAssistantMessage?.Text);
            Assert.Equal("group-1", loaded.ProjectExecutionContext?.ExecutionGroupId);

            await store.DeleteAsync("conversation-a");
            Assert.Empty(await store.LoadAsync("conversation-a"));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    private static ConversationTurn Turn()
    {
        var timestamp = new DateTimeOffset(2026, 8, 30, 10, 0, 0, TimeSpan.Zero);
        return new ConversationTurn(
            "turn-1",
            "conversation-a",
            1,
            4,
            new ChatMessage(
                "user-1",
                ChatMessageRole.User,
                "hello",
                timestamp,
                Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<TurnProcessEvent>(),
            new ChatMessage(
                "assistant-1",
                ChatMessageRole.Assistant,
                "world",
                timestamp,
                Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<ConversationAssistantReply>(),
            null,
            true,
            TurnStatus.Completed,
            timestamp,
            timestamp,
            new ProjectExecutionContext(ExecutionGroupId: "group-1"));
    }
}
