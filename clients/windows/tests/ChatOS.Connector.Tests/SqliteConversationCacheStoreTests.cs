using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class SqliteConversationCacheStoreTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task RoundTripsConversationTurnsAndCanDeleteSnapshot(bool includeGuidance)
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
            if (includeGuidance)
            {
                turn = turn with
                {
                    AdditionalUserMessages =
                    [
                        new ChatMessage("guidance-1", ChatMessageRole.User, "Also test Windows",
                            turn.StartedAt, [new ConversationAttachmentReference(
                                "file-1", "notes.txt", "text/plain", 12, ConversationAttachmentKind.File)])
                        {
                            StorageOrdinal = 3,
                        },
                    ],
                };
            }

            await store.SaveAsync("conversation-a", new[] { turn });
            var loaded = Assert.Single(await store.LoadAsync("conversation-a"));

            Assert.Equal(turn.Id, loaded.Id);
            Assert.Equal(turn.FinalAssistantMessage?.Text, loaded.FinalAssistantMessage?.Text);
            if (includeGuidance)
            {
                var guidance = Assert.Single(loaded.AdditionalUserMessages);
                Assert.Equal("guidance-1", guidance.Id);
                Assert.Equal("Also test Windows", guidance.Text);
                Assert.Equal((ulong)3, guidance.StorageOrdinal);
                Assert.Equal("notes.txt", Assert.Single(guidance.Attachments).Name);
            }
            else
            {
                Assert.Empty(loaded.AdditionalUserMessages);
            }

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
            timestamp);
    }
}
