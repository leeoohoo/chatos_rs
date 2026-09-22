using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed partial class LocalStateDatabase
{
    private static async Task MigrateAgentTodoExecutionContractsAsync(
        SqliteConnection connection,
        CancellationToken cancellationToken)
    {
        var columns = new HashSet<string>(StringComparer.Ordinal);
        using (var inspect = connection.CreateCommand())
        {
            inspect.CommandText = "PRAGMA table_info(agent_todos)";
            await using var reader = await inspect.ExecuteReaderAsync(cancellationToken)
                .ConfigureAwait(false);
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                columns.Add(reader.GetString(1));
        }

        using var transaction = connection.BeginTransaction();
        if (!columns.Contains("execution_contract_json"))
        {
            using var addContract = connection.CreateCommand();
            addContract.Transaction = transaction;
            addContract.CommandText = """
                ALTER TABLE agent_todos
                    ADD COLUMN execution_contract_json TEXT NOT NULL DEFAULT '{}';
                """;
            await addContract.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        if (!columns.Contains("execution_plan_json"))
        {
            using var addPlan = connection.CreateCommand();
            addPlan.Transaction = transaction;
            addPlan.CommandText = """
                ALTER TABLE agent_todos
                    ADD COLUMN execution_plan_json TEXT NOT NULL DEFAULT '{}';
                """;
            await addPlan.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        using var migrate = connection.CreateCommand();
        migrate.Transaction = transaction;
        migrate.CommandText = """
            CREATE TABLE IF NOT EXISTS agent_todo_sources (
                owner_user_id TEXT NOT NULL,
                todo_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                relation TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, todo_id, conversation_id, message_id, relation)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_todo_sources_message
                ON agent_todo_sources(owner_user_id, conversation_id, message_id);

            INSERT OR IGNORE INTO agent_todo_sources (
                owner_user_id, todo_id, conversation_id, message_id, relation,
                created_at_unix_ms)
            SELECT owner_user_id, id, room_id, source_message_id, 'Created',
                   created_at_unix_ms
            FROM agent_todos
            WHERE source_message_id IS NOT NULL;

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (17, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
            """;
        await migrate.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
    }
}
