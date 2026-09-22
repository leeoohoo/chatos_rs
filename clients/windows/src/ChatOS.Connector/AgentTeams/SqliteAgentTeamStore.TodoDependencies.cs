using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    internal const string ReleaseDependentTodosCommandText = """
        UPDATE agent_todos
        SET status = 'Ready', revision = revision + 1, updated_at_unix_ms = @p0
        WHERE owner_user_id = @p1 AND status = 'Pending'
          AND EXISTS (
              SELECT 1
              FROM json_each(agent_todos.dependency_ids_json) completed_link
              WHERE completed_link.value = @p2
          )
          AND NOT EXISTS (
              SELECT 1
              FROM json_each(agent_todos.dependency_ids_json) required_link
              LEFT JOIN agent_todos required
                ON required.owner_user_id = agent_todos.owner_user_id
               AND required.id = required_link.value
              WHERE required.id IS NULL OR required.status != 'Completed'
          )
        """;

    private static async Task<bool> DependenciesCompleteAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        IReadOnlyList<string> dependencyIds,
        CancellationToken cancellationToken)
    {
        if (dependencyIds.Count == 0) return true;

        var ids = dependencyIds.Distinct(StringComparer.Ordinal).ToArray();
        var placeholders = string.Join(", ", Enumerable.Range(1, ids.Length)
            .Select(index => $"@p{index}"));
        using var command = Command(connection, transaction, $"""
            SELECT COUNT(*) FROM agent_todos
            WHERE owner_user_id = @p0 AND status = 'Completed'
              AND id IN ({placeholders})
            """, [ownerUserId, .. ids.Cast<object>()]);
        var completedCount = Convert.ToInt32(
            await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false));
        return completedCount == ids.Length;
    }

    private static async Task ReleaseDependentTodosAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string completedTodoId,
        long now,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            ReleaseDependentTodosCommandText, now, ownerUserId, completedTodoId);
        _ = await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
