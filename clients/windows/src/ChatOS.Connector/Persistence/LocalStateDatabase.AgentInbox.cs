using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed partial class LocalStateDatabase
{
    private static async Task MigrateAgentInboxAsync(
        SqliteConnection connection,
        CancellationToken cancellationToken)
    {
        var hasMessageTimestamp = false;
        using (var columns = connection.CreateCommand())
        {
            columns.CommandText = "PRAGMA table_info(agent_read_cursors)";
            await using var reader = await columns.ExecuteReaderAsync(cancellationToken)
                .ConfigureAwait(false);
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                if (string.Equals(reader.GetString(1),
                        "through_message_created_at_unix_ms", StringComparison.Ordinal))
                {
                    hasMessageTimestamp = true;
                    break;
                }
            }
        }

        using var command = connection.CreateCommand();
        command.CommandText = (hasMessageTimestamp ? """
            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (16, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
            """ : """
            ALTER TABLE agent_read_cursors
                ADD COLUMN through_message_created_at_unix_ms INTEGER NOT NULL DEFAULT 0;
            UPDATE agent_read_cursors
            SET through_message_created_at_unix_ms = COALESCE((
                SELECT created_at_unix_ms FROM agent_messages message
                WHERE message.owner_user_id = agent_read_cursors.owner_user_id
                  AND message.room_id = agent_read_cursors.room_id
                  AND message.id = agent_read_cursors.through_message_id), 0);
            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (16, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
            """) + """
            CREATE INDEX IF NOT EXISTS ix_agent_messages_owner_created
                ON agent_messages(owner_user_id, created_at_unix_ms, id, room_id);
            """;
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
