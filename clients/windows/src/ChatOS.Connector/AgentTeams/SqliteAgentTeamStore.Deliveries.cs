using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private const string DeliveryColumns = """
        owner_user_id, id, room_id, message_id, root_message_id, target_agent_id,
        trigger_kind, status, attempt, hop_count, deduplication_key, response_message_id,
        last_error, claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
        """;

    public async Task<IReadOnlyList<string>> ListOwnersWithPendingDeliveriesAsync(
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = connection.CreateCommand();
        command.CommandText = """
            SELECT DISTINCT owner_user_id FROM agent_deliveries
            WHERE status = 'Pending' ORDER BY owner_user_id
            """;
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var owners = new List<string>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            owners.Add(reader.GetString(0));
        }

        return owners;
    }

    public async Task<AgentDelivery?> ClaimNextDeliveryAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var now = Now();
        using (var recover = Command(connection, transaction, """
            UPDATE agent_deliveries SET status = 'Pending', claimed_at_unix_ms = NULL,
                last_error = 'Recovered after an interrupted local Agent run.'
            WHERE owner_user_id = @p0 AND status = 'Running'
              AND claimed_at_unix_ms < @p1
            """, ownerUserId, now - 10 * 60_000L))
        {
            await recover.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        AgentDelivery? pending;
        using (var select = Command(connection, transaction,
            $"SELECT {DeliveryColumns} FROM agent_deliveries " +
            "WHERE owner_user_id = @p0 AND status = 'Pending' " +
            "ORDER BY created_at_unix_ms, id LIMIT 1", ownerUserId))
        await using (var reader = await select.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            pending = await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
                ? ReadDelivery(reader)
                : null;
        }

        if (pending is null)
        {
            await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
            return null;
        }

        using var update = Command(connection, transaction, """
            UPDATE agent_deliveries SET status = 'Running', attempt = attempt + 1,
                claimed_at_unix_ms = @p0, last_error = NULL
            WHERE owner_user_id = @p1 AND id = @p2 AND status = 'Pending'
            """, now, ownerUserId, pending.Id);
        if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
        {
            throw Conflict("Agent delivery changed before it could be claimed.");
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return pending with
        {
            Status = AgentDeliveryStatus.Running,
            Attempt = pending.Attempt + 1,
            ClaimedAtUnixMs = now,
            LastError = null,
        };
    }

    public Task<AgentDelivery> CompleteDeliveryAsync(
        string ownerUserId,
        string deliveryId,
        string? responseMessageId,
        CancellationToken cancellationToken = default) =>
        FinishDeliveryAsync(ownerUserId, deliveryId, AgentDeliveryStatus.Completed,
            responseMessageId, null, cancellationToken);

    public Task<AgentDelivery> FailDeliveryAsync(
        string ownerUserId,
        string deliveryId,
        string error,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Text(error, nameof(error), 2_000);
        return FinishDeliveryAsync(ownerUserId, deliveryId, AgentDeliveryStatus.Failed,
            null, error, cancellationToken);
    }

    public async Task<IReadOnlyList<AgentDelivery>> EnqueueDueHeartbeatsAsync(
        long nowUnixMs,
        CancellationToken cancellationToken = default)
    {
        if (nowUnixMs < 0)
        {
            throw AgentTeamValidation.Invalid(nameof(nowUnixMs));
        }

        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var due = new List<(string Owner, string Agent, int Interval, string Prompt)>();
        using (var command = Command(connection, transaction, """
            SELECT owner_user_id, id, heartbeat_interval_seconds, heartbeat_prompt
            FROM agent_profiles
            WHERE status = 'Active' AND heartbeat_enabled = 1
              AND next_heartbeat_at_unix_ms IS NOT NULL
              AND next_heartbeat_at_unix_ms <= @p0
            ORDER BY next_heartbeat_at_unix_ms, owner_user_id, id
            """, nowUnixMs))
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                due.Add((reader.GetString(0), reader.GetString(1), reader.GetInt32(2),
                    reader.GetString(3)));
            }
        }

        var output = new List<AgentDelivery>();
        foreach (var item in due)
        {
            var rooms = new List<string>();
            using (var roomCommand = Command(connection, transaction, """
                SELECT m.room_id FROM agent_room_members m
                JOIN agent_rooms r ON r.owner_user_id = m.owner_user_id AND r.id = m.room_id
                WHERE m.owner_user_id = @p0 AND m.agent_id = @p1
                  AND m.status = 'Active' AND r.status = 'Active'
                ORDER BY CASE r.conversation_kind WHEN 'ProjectTeam' THEN 0 ELSE 1 END,
                    r.updated_at_unix_ms DESC
                """, item.Owner, item.Agent))
            await using (var reader = await roomCommand.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
            {
                while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                {
                    rooms.Add(reader.GetString(0));
                }
            }

            foreach (var roomId in rooms)
            {
                var content = string.IsNullOrWhiteSpace(item.Prompt)
                    ? "执行定期心跳：读取未读消息、检查团队任务和阻塞，只在有行动时回复。"
                    : item.Prompt;
                var messageId = await InsertSystemMessageAsync(connection, transaction,
                    item.Owner, roomId, content, nowUnixMs, cancellationToken).ConfigureAwait(false);
                var bucket = nowUnixMs / Math.Max(60_000L, item.Interval * 1_000L);
                var delivery = await InsertDeliveryAsync(connection, transaction, item.Owner,
                    roomId, messageId, messageId, item.Agent, AgentDeliveryTrigger.Heartbeat, 0,
                    $"heartbeat:{item.Agent}:{roomId}:{bucket}", nowUnixMs,
                    cancellationToken).ConfigureAwait(false);
                if (delivery is not null)
                {
                    output.Add(delivery);
                }
            }

            using var update = Command(connection, transaction, """
                UPDATE agent_profiles SET last_heartbeat_at_unix_ms = @p0,
                    next_heartbeat_at_unix_ms = @p1, updated_at_unix_ms = MAX(updated_at_unix_ms, @p0)
                WHERE owner_user_id = @p2 AND id = @p3
                """, nowUnixMs, nowUnixMs + item.Interval * 1_000L, item.Owner, item.Agent);
            await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return output;
    }

    public async Task<AgentRunSummary> SaveRunAsync(
        AgentRunSummary run,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, """
            INSERT INTO agent_runs (
                owner_user_id, id, delivery_id, agent_id, room_id, status, model_calls,
                last_error, created_at_unix_ms, updated_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9)
            ON CONFLICT(owner_user_id, delivery_id) DO UPDATE SET
                status = excluded.status, model_calls = excluded.model_calls,
                last_error = excluded.last_error, updated_at_unix_ms = excluded.updated_at_unix_ms
            """, run.OwnerUserId, run.Id, run.DeliveryId, run.AgentId, run.RoomId,
            run.Status.ToString(), run.ModelCalls, DbValue(run.LastError), run.CreatedAtUnixMs,
            run.UpdatedAtUnixMs);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        return run;
    }

    public async Task<IReadOnlyList<AgentRunSummary>> ListRunsAsync(
        string ownerUserId,
        string roomId,
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        if (limit is < 1 or > 1_000)
        {
            throw AgentTeamValidation.Invalid(nameof(limit));
        }

        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, """
            SELECT id, delivery_id, agent_id, status, model_calls, last_error,
                created_at_unix_ms, updated_at_unix_ms
            FROM agent_runs WHERE owner_user_id = @p0 AND room_id = @p1
            ORDER BY updated_at_unix_ms DESC LIMIT @p2
            """, ownerUserId, roomId, limit);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentRunSummary>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(new AgentRunSummary(reader.GetString(0), ownerUserId,
                reader.GetString(1), reader.GetString(2), roomId,
                ParseEnum<AgentRunStatus>(reader.GetString(3)), reader.GetInt32(4),
                reader.IsDBNull(5) ? null : reader.GetString(5), reader.GetInt64(6),
                reader.GetInt64(7)));
        }

        return output;
    }

    private async Task<AgentDelivery> FinishDeliveryAsync(
        string ownerUserId,
        string deliveryId,
        AgentDeliveryStatus status,
        string? responseMessageId,
        string? error,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var current = await ReadDeliveryAsync(
            connection, transaction, ownerUserId, deliveryId, cancellationToken).ConfigureAwait(false)
            ?? throw NotFound("Agent delivery");
        if (current.Status != AgentDeliveryStatus.Running)
        {
            throw Conflict("Only a running Agent delivery can be finished.");
        }

        var now = Now();
        using var command = Command(connection, transaction, """
            UPDATE agent_deliveries SET status = @p0, response_message_id = @p1,
                last_error = @p2, completed_at_unix_ms = @p3
            WHERE owner_user_id = @p4 AND id = @p5 AND status = 'Running'
            """, status.ToString(), DbValue(responseMessageId), DbValue(error), now,
            ownerUserId, deliveryId);
        if (await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
        {
            throw Conflict("Agent delivery changed before it could be finished.");
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return current with
        {
            Status = status,
            ResponseMessageId = responseMessageId,
            LastError = error,
            CompletedAtUnixMs = now,
        };
    }

    private static async Task<string> InsertSystemMessageAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string content,
        long now,
        CancellationToken cancellationToken)
    {
        var messageId = NewId();
        using var command = Command(connection, transaction, """
            INSERT INTO agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_agent_id, content,
                reply_to_message_id, root_message_id, hop_count, created_at_unix_ms)
            VALUES (@p0, @p1, @p2, 'System', NULL, @p3, NULL, @p1, 0, @p4)
            """, ownerUserId, messageId, roomId, content, now);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        return messageId;
    }

    private static async Task<AgentDelivery?> InsertDeliveryAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string messageId,
        string rootMessageId,
        string targetAgentId,
        AgentDeliveryTrigger trigger,
        int hopCount,
        string deduplicationKey,
        long now,
        CancellationToken cancellationToken)
    {
        var id = NewId();
        using var command = Command(connection, transaction, """
            INSERT OR IGNORE INTO agent_deliveries (
                owner_user_id, id, room_id, message_id, root_message_id, target_agent_id,
                trigger_kind, status, attempt, hop_count, deduplication_key,
                response_message_id, last_error, claimed_at_unix_ms, completed_at_unix_ms,
                created_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, 'Pending', 0, @p7, @p8,
                NULL, NULL, NULL, NULL, @p9)
            """, ownerUserId, id, roomId, messageId, rootMessageId, targetAgentId,
            trigger.ToString(), hopCount, deduplicationKey, now);
        if (await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
        {
            return null;
        }

        return new AgentDelivery(id, ownerUserId, roomId, messageId, rootMessageId,
            targetAgentId, trigger, AgentDeliveryStatus.Pending, 0, hopCount,
            deduplicationKey, null, null, null, null, now);
    }

    private static AgentDelivery ReadDelivery(SqliteDataReader reader) => new(
        reader.GetString(1), reader.GetString(0), reader.GetString(2), reader.GetString(3),
        reader.GetString(4), reader.GetString(5),
        ParseEnum<AgentDeliveryTrigger>(reader.GetString(6)),
        ParseEnum<AgentDeliveryStatus>(reader.GetString(7)), reader.GetInt32(8),
        reader.GetInt32(9), reader.GetString(10),
        reader.IsDBNull(11) ? null : reader.GetString(11),
        reader.IsDBNull(12) ? null : reader.GetString(12),
        reader.IsDBNull(13) ? null : reader.GetInt64(13),
        reader.IsDBNull(14) ? null : reader.GetInt64(14), reader.GetInt64(15));

    private static async Task<AgentDelivery?> ReadDeliveryAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string deliveryId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {DeliveryColumns} FROM agent_deliveries WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, deliveryId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
            ? ReadDelivery(reader)
            : null;
    }
}
