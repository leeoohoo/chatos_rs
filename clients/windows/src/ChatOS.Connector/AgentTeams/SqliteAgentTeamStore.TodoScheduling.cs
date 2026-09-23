using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    public async Task<AgentTodoScheduleState> GetTodoScheduleStateAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(agentId, nameof(agentId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        await RequireAgentAsync(connection, null, ownerUserId, agentId, requireActive: true,
            cancellationToken).ConfigureAwait(false);
        var running = await ReadScheduledTodoAsync(connection, null, ownerUserId, agentId,
            AgentTodoStatus.InProgress, cancellationToken).ConfigureAwait(false);
        var ready = await ReadScheduledTodoAsync(connection, null, ownerUserId, agentId,
            AgentTodoStatus.Ready, cancellationToken).ConfigureAwait(false);
        return new AgentTodoScheduleState(running, ready);
    }

    public async Task<AgentDelivery?> StartNextReadyTodoAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(agentId, nameof(agentId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireAgentAsync(connection, transaction, ownerUserId, agentId,
            requireActive: true, cancellationToken).ConfigureAwait(false);
        var delivery = await StartNextReadyTodoAsync(connection, transaction, ownerUserId,
            agentId, Now(), cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return delivery;
    }

    public async Task<IReadOnlyList<AgentTodoAssetSnapshot>> ListTodoAssetSnapshotsAsync(
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(todoId, nameof(todoId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        if (await ReadTodoAsync(connection, null, ownerUserId, todoId, cancellationToken,
            includeSources: false).ConfigureAwait(false) is null)
            throw NotFound("Todo");
        using var command = Command(connection, null, """
            SELECT todo_id, asset_id, team_room_id, category, title, markdown, revision,
                captured_at_unix_ms
            FROM agent_todo_asset_snapshots
            WHERE owner_user_id = @p0 AND todo_id = @p1
            ORDER BY category, asset_id
            """, ownerUserId, todoId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        var output = new List<AgentTodoAssetSnapshot>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            output.Add(new AgentTodoAssetSnapshot(reader.GetString(0), reader.GetString(1),
                reader.GetString(2), ParseEnum<AgentTeamAssetCategory>(reader.GetString(3)),
                reader.GetString(4), reader.GetString(5), reader.GetInt32(6), reader.GetInt64(7)));
        return output;
    }

    private static async Task<AgentDelivery?> StartNextReadyTodoAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string agentId,
        long now,
        CancellationToken cancellationToken)
    {
        using (var busy = Command(connection, transaction, """
            SELECT 1
            FROM agent_todos
            WHERE owner_user_id = @p0 AND agent_id = @p1 AND status = 'InProgress'
            LIMIT 1
            """, ownerUserId, agentId))
        {
            if (await busy.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) is not null)
                return null;
        }

        using (var outstanding = Command(connection, transaction, """
            SELECT 1
            FROM agent_deliveries
            WHERE owner_user_id = @p0 AND target_agent_id = @p1
              AND trigger_kind = 'Todo' AND status IN ('Pending', 'Running')
            LIMIT 1
            """, ownerUserId, agentId))
        {
            if (await outstanding.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) is not null)
                return null;
        }

        var todo = await ReadScheduledTodoAsync(connection, transaction, ownerUserId, agentId,
            AgentTodoStatus.Ready, cancellationToken, includeSources: false).ConfigureAwait(false);
        if (todo is null) return null;
        using (var snapshot = Command(connection, transaction, """
            INSERT OR IGNORE INTO agent_todo_asset_snapshots (
                owner_user_id, todo_id, asset_id, team_room_id, category, title,
                markdown, revision, captured_at_unix_ms)
            SELECT owner_user_id, @p0, id, room_id, category, title, markdown, revision, @p1
            FROM agent_team_assets
            WHERE owner_user_id = @p2 AND room_id = @p3 AND status = 'Active'
            """, todo.Id, now, ownerUserId, todo.Draft.TeamRoomId))
        {
            await snapshot.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }
        var nextRevision = todo.Revision + 1;
        using (var update = Command(connection, transaction, """
            UPDATE agent_todos
            SET status = 'InProgress', revision = @p0, updated_at_unix_ms = @p1
            WHERE owner_user_id = @p2 AND id = @p3 AND agent_id = @p4
              AND status = 'Ready' AND revision = @p5
            """, nextRevision, now, ownerUserId, todo.Id, agentId, todo.Revision))
        {
            if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
                throw Conflict("Todo changed before it could be started.");
        }

        return await PrepareTodoDeliveryAsync(connection, transaction, ownerUserId, agentId,
            todo, now, cancellationToken).ConfigureAwait(false);
    }

    private static async Task<AgentDelivery> PrepareTodoDeliveryAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string agentId,
        AgentTodo todo,
        long now,
        CancellationToken cancellationToken)
    {
        var deduplicationKey = $"todo:{todo.Id}";
        AgentDelivery? existing;
        using (var select = Command(connection, transaction,
            $"SELECT {DeliveryColumns} FROM agent_deliveries " +
            "WHERE owner_user_id = @p0 AND trigger_kind = 'Todo' " +
            "AND (deduplication_key = @p1 OR deduplication_key LIKE @p2) " +
            "ORDER BY CASE WHEN deduplication_key = @p1 THEN 0 ELSE 1 END, " +
            "created_at_unix_ms DESC, id DESC LIMIT 1",
            ownerUserId, deduplicationKey, $"{deduplicationKey}:revision:%"))
        await using (var reader = await select.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            existing = await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
                ? ReadDelivery(reader)
                : null;
        }

        var content = $"Todo：{todo.Draft.Title}\n{todo.Draft.Detail}";
        if (existing is not null)
        {
            if (existing.Status != AgentDeliveryStatus.Failed ||
                existing.Trigger != AgentDeliveryTrigger.Todo ||
                !string.Equals(existing.TargetAgentId, agentId, StringComparison.Ordinal) ||
                !string.Equals(existing.RoomId, todo.Draft.TeamRoomId, StringComparison.Ordinal))
            {
                throw Conflict("Only the failed delivery owned by this Todo can be retried.");
            }

            using (var updateMessage = Command(connection, transaction, """
                UPDATE agent_messages SET content = @p0
                WHERE owner_user_id = @p1 AND room_id = @p2 AND id = @p3
                """, content, ownerUserId, todo.Draft.TeamRoomId, existing.MessageId))
            {
                if (await updateMessage.ExecuteNonQueryAsync(cancellationToken)
                    .ConfigureAwait(false) != 1)
                    throw Conflict("Todo delivery trigger message is unavailable.");
            }

            using (var reactivate = Command(connection, transaction, """
                UPDATE agent_deliveries
                SET deduplication_key = @p0, status = 'Pending', response_message_id = NULL,
                    last_error = NULL, claimed_at_unix_ms = NULL,
                    completed_at_unix_ms = NULL
                WHERE owner_user_id = @p1 AND id = @p2 AND status = 'Failed'
                """, deduplicationKey, ownerUserId, existing.Id))
            {
                if (await reactivate.ExecuteNonQueryAsync(cancellationToken)
                    .ConfigureAwait(false) != 1)
                    throw Conflict("Failed Todo delivery changed before it could be retried.");
            }

            return existing with
            {
                Status = AgentDeliveryStatus.Pending,
                DeduplicationKey = deduplicationKey,
                ResponseMessageId = null,
                LastError = null,
                ClaimedAtUnixMs = null,
                CompletedAtUnixMs = null,
            };
        }

        var messageId = await InsertSystemMessageAsync(connection, transaction, ownerUserId,
            todo.Draft.TeamRoomId, content, now, cancellationToken).ConfigureAwait(false);
        return await InsertDeliveryAsync(connection, transaction, ownerUserId,
            todo.Draft.TeamRoomId, messageId, messageId, agentId, AgentDeliveryTrigger.Todo, 0,
            deduplicationKey, now, cancellationToken).ConfigureAwait(false)
            ?? throw Conflict("Todo delivery already exists.");
    }

    private static async Task<AgentTodo?> ReadScheduledTodoAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string agentId,
        AgentTodoStatus status,
        CancellationToken cancellationToken,
        bool includeSources = true)
    {
        AgentTodo? todo;
        using (var command = Command(connection, transaction,
            $"SELECT {TodoColumns} FROM agent_todos " +
            "WHERE owner_user_id = @p0 AND agent_id = @p1 AND status = @p2 " +
            "ORDER BY CASE priority WHEN 'Urgent' THEN 3 WHEN 'High' THEN 2 " +
            "WHEN 'Normal' THEN 1 ELSE 0 END DESC, " +
            "sort_order, created_at_unix_ms, id LIMIT 1",
            ownerUserId, agentId, status.ToString()))
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            todo = await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
                ? ReadTodo(reader) : null;
        }
        if (todo is null || !includeSources) return todo;
        return (await AttachTodoSourcesAsync(connection, transaction, ownerUserId, [todo],
            cancellationToken).ConfigureAwait(false))[0];
    }

    private static async Task ScheduleReadyAgentsAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        long now,
        CancellationToken cancellationToken)
    {
        var agentIds = new List<string>();
        using (var command = Command(connection, transaction, """
            SELECT DISTINCT agent_id FROM agent_todos
            WHERE owner_user_id = @p0 AND status = 'Ready'
            ORDER BY agent_id
            """, ownerUserId))
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                agentIds.Add(reader.GetString(0));
        }

        foreach (var agentId in agentIds)
            _ = await StartNextReadyTodoAsync(connection, transaction, ownerUserId, agentId,
                now, cancellationToken).ConfigureAwait(false);
    }

    private static async Task BlockUnfinishedTodoForDeliveryAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentDelivery delivery,
        AgentDeliveryStatus deliveryStatus,
        string? error,
        long now,
        CancellationToken cancellationToken)
    {
        var parts = delivery.DeduplicationKey.Split(':');
        if (parts.Length < 2 || parts[0] != "todo") return;
        var todo = await ReadTodoAsync(connection, transaction, delivery.OwnerUserId, parts[1],
            cancellationToken, includeSources: false).ConfigureAwait(false);
        if (todo?.Status != AgentTodoStatus.InProgress) return;
        var detail = deliveryStatus == AgentDeliveryStatus.Failed
            ? error ?? "The Todo executor failed without a recoverable error."
            : "The Todo executor ended without completing or explicitly blocking the task.";
        var blocked = todo with
        {
            Status = AgentTodoStatus.Blocked,
            Result = detail,
            Revision = todo.Revision + 1,
            UpdatedAtUnixMs = now,
        };
        using var update = Command(connection, transaction, """
            UPDATE agent_todos
            SET status = 'Blocked', result = @p0, revision = @p1, updated_at_unix_ms = @p2
            WHERE owner_user_id = @p3 AND id = @p4 AND status = 'InProgress'
              AND revision = @p5
            """, detail, blocked.Revision, now, delivery.OwnerUserId, todo.Id, todo.Revision);
        if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
            throw Conflict("Todo changed while its executor delivery was finishing.");
        await EnqueueManagerTodoStatusAsync(connection, transaction, blocked,
            $"Todo 执行周期未正常收尾，已转为 Blocked：{blocked.Draft.Title}\n{detail}",
            $"todo-status:{blocked.Id}:revision:{blocked.Revision}", cancellationToken)
            .ConfigureAwait(false);
    }
}
