using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteConversationCacheStore : IConversationCacheStore
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
        Converters = { new JsonStringEnumConverter() },
    };

    private readonly LocalStateDatabase _database;

    public SqliteConversationCacheStore(LocalStateDatabase database)
    {
        _database = database;
    }

    public async Task<IReadOnlyList<ConversationTurn>> LoadAsync(
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT payload_json
            FROM conversation_cache
            WHERE conversation_id = $conversation_id
            ORDER BY updated_at, message_id;
            """;
        command.Parameters.AddWithValue("$conversation_id", conversationId);
        var turns = new List<ConversationTurn>();
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            try
            {
                var turn = JsonSerializer.Deserialize<ConversationTurn>(
                    reader.GetString(0),
                    JsonOptions);
                if (turn is not null &&
                    string.Equals(turn.ConversationId, conversationId, StringComparison.Ordinal))
                {
                    turns.Add(turn);
                }
            }
            catch (JsonException)
            {
                // A corrupt cache row must never prevent the server history from loading.
            }
        }

        return turns;
    }

    public async Task SaveAsync(
        string conversationId,
        IReadOnlyList<ConversationTurn> turns,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        await using var transaction = (SqliteTransaction)await connection
            .BeginTransactionAsync(cancellationToken)
            .ConfigureAwait(false);
        var delete = connection.CreateCommand();
        delete.Transaction = transaction;
        delete.CommandText = "DELETE FROM conversation_cache WHERE conversation_id = $conversation_id;";
        delete.Parameters.AddWithValue("$conversation_id", conversationId);
        await delete.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);

        foreach (var turn in turns.Where(turn =>
                     string.Equals(turn.ConversationId, conversationId, StringComparison.Ordinal)))
        {
            var insert = connection.CreateCommand();
            insert.Transaction = transaction;
            insert.CommandText = """
                INSERT INTO conversation_cache(
                    conversation_id,
                    message_id,
                    event_sequence,
                    payload_json,
                    updated_at
                ) VALUES (
                    $conversation_id,
                    $message_id,
                    $event_sequence,
                    $payload_json,
                    $updated_at
                );
                """;
            insert.Parameters.AddWithValue("$conversation_id", conversationId);
            insert.Parameters.AddWithValue("$message_id", turn.Id);
            insert.Parameters.AddWithValue("$event_sequence", turn.Revision);
            insert.Parameters.AddWithValue("$payload_json", JsonSerializer.Serialize(turn, JsonOptions));
            insert.Parameters.AddWithValue("$updated_at", DateTimeOffset.UtcNow.ToString("O"));
            await insert.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteAsync(
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "DELETE FROM conversation_cache WHERE conversation_id = $conversation_id;";
        command.Parameters.AddWithValue("$conversation_id", conversationId);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
