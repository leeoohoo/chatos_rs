using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed partial class LocalStateDatabase
{
    private static async Task MigrateAgentTodoAssetSnapshotsAsync(
        SqliteConnection connection,
        CancellationToken cancellationToken)
    {
        using var command = connection.CreateCommand();
        command.CommandText = """
            CREATE TABLE IF NOT EXISTS agent_todo_asset_snapshots (
                owner_user_id TEXT NOT NULL,
                todo_id TEXT NOT NULL,
                asset_id TEXT NOT NULL,
                team_room_id TEXT NOT NULL,
                category TEXT NOT NULL,
                title TEXT NOT NULL,
                markdown TEXT NOT NULL,
                revision INTEGER NOT NULL,
                captured_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, todo_id, asset_id)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_todo_asset_snapshots_todo
                ON agent_todo_asset_snapshots(owner_user_id, todo_id, category, asset_id);

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (18, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
            """;
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
