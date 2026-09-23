using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed partial class LocalStateDatabase
{
    private static async Task MigrateAgentStaffingProposalsAsync(
        SqliteConnection connection,
        CancellationToken cancellationToken)
    {
        using var command = connection.CreateCommand();
        command.CommandText = """
            CREATE TABLE IF NOT EXISTS agent_staffing_proposals (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                kind TEXT NOT NULL,
                source_room_id TEXT NOT NULL,
                proposer_agent_id TEXT NOT NULL,
                source_delivery_id TEXT NOT NULL,
                request_key TEXT NOT NULL,
                draft_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_agent_id TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                resolved_at_unix_ms INTEGER,
                PRIMARY KEY(owner_user_id, id),
                UNIQUE(owner_user_id, source_room_id, proposer_agent_id,
                    source_delivery_id, request_key)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_staffing_proposals_room
                ON agent_staffing_proposals(
                    owner_user_id, source_room_id, status, created_at_unix_ms DESC);

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (15, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
            """;
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
