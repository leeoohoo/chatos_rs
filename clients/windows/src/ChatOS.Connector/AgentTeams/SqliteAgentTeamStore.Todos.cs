using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private const string TodoColumns = """
        owner_user_id, id, room_id, agent_id, title, detail, priority,
        dependency_ids_json, source_message_id, status, result, sort_order, revision,
        created_at_unix_ms, updated_at_unix_ms
        """;

    public async Task<IReadOnlyList<AgentTodo>> ListTodosAsync(
        string ownerUserId,
        string roomId,
        bool includeTerminal = true,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {TodoColumns} FROM agent_todos WHERE owner_user_id = @p0 AND room_id = @p1" +
            (includeTerminal ? string.Empty : " AND status NOT IN ('Completed', 'Cancelled')") +
            " ORDER BY sort_order, created_at_unix_ms, id", ownerUserId, roomId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentTodo>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(ReadTodo(reader));
        }

        return output;
    }

    public async Task<AgentTodo?> GetTodoAsync(
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        return await ReadTodoAsync(connection, null, ownerUserId, todoId, cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task<AgentTodo> CreateTodoAsync(
        string ownerUserId,
        AgentTodoDraft draft,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        draft.Validate();
        var now = Now();
        var todoId = NewId();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireRoomAsync(connection, transaction, ownerUserId, draft.TeamRoomId,
            requireActive: true, cancellationToken).ConfigureAwait(false);
        await RequireActiveMemberAsync(connection, transaction, ownerUserId, draft.TeamRoomId,
            draft.AgentId, cancellationToken).ConfigureAwait(false);
        foreach (var dependencyId in draft.Dependencies)
        {
            var dependency = await ReadTodoAsync(
                connection, transaction, ownerUserId, dependencyId, cancellationToken).ConfigureAwait(false)
                ?? throw NotFound("Todo dependency");
            if (!string.Equals(dependency.Draft.TeamRoomId, draft.TeamRoomId, StringComparison.Ordinal))
            {
                throw Conflict("Todo dependencies must belong to the same team.");
            }
        }

        var ready = await DependenciesCompleteAsync(
            connection, transaction, ownerUserId, draft.Dependencies, cancellationToken).ConfigureAwait(false);
        var status = ready ? AgentTodoStatus.Ready : AgentTodoStatus.Pending;
        var sortOrder = await NextTodoOrderAsync(
            connection, transaction, ownerUserId, draft.TeamRoomId, cancellationToken).ConfigureAwait(false);
        var todo = new AgentTodo(todoId, ownerUserId, draft, status, string.Empty,
            sortOrder, 1, now, now);
        todo.Validate();
        using (var command = Command(connection, transaction, $"""
            INSERT INTO agent_todos ({TodoColumns})
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9, @p10,
                @p11, @p12, @p13, @p14)
            """, ownerUserId, todoId, draft.TeamRoomId, draft.AgentId, draft.Title,
            draft.Detail, draft.Priority.ToString(), Serialize(draft.Dependencies),
            DbValue(draft.SourceMessageId), status.ToString(), string.Empty, sortOrder, 1, now, now))
        {
            await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        if (status == AgentTodoStatus.Ready)
        {
            await EnqueueTodoAsync(connection, transaction, todo, cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return todo;
    }

    public async Task<AgentTodo> UpdateTodoAsync(
        string ownerUserId,
        string todoId,
        long expectedRevision,
        AgentTodoStatus status,
        string result,
        string? assignedAgentId = null,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.OptionalText(result, nameof(result), 16_000);
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var current = await ReadTodoAsync(connection, transaction, ownerUserId, todoId, cancellationToken)
            .ConfigureAwait(false) ?? throw NotFound("Todo");
        if (current.Revision != expectedRevision)
        {
            throw Conflict("Todo changed before the update was applied.");
        }

        var agentId = assignedAgentId ?? current.Draft.AgentId;
        await RequireActiveMemberAsync(connection, transaction, ownerUserId,
            current.Draft.TeamRoomId, agentId, cancellationToken).ConfigureAwait(false);
        if ((status is AgentTodoStatus.Ready or AgentTodoStatus.InProgress) &&
            !await DependenciesCompleteAsync(connection, transaction, ownerUserId,
                current.Draft.Dependencies, cancellationToken).ConfigureAwait(false))
        {
            throw Conflict("Todo prerequisites are not completed.");
        }

        var now = Math.Max(Now(), current.UpdatedAtUnixMs);
        var next = current with
        {
            Draft = current.Draft with { AgentId = agentId },
            Status = status,
            Result = result,
            Revision = current.Revision + 1,
            UpdatedAtUnixMs = now,
        };
        next.Validate();
        using (var command = Command(connection, transaction, """
            UPDATE agent_todos SET agent_id = @p0, status = @p1, result = @p2,
                revision = @p3, updated_at_unix_ms = @p4
            WHERE owner_user_id = @p5 AND id = @p6 AND revision = @p7
            """, agentId, status.ToString(), result, next.Revision, now,
            ownerUserId, todoId, expectedRevision))
        {
            if (await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
            {
                throw Conflict("Todo changed before the update was applied.");
            }
        }

        if (status == AgentTodoStatus.Ready && current.Status != AgentTodoStatus.Ready)
        {
            await EnqueueTodoAsync(connection, transaction, next, cancellationToken).ConfigureAwait(false);
        }

        if (status == AgentTodoStatus.Completed)
        {
            await ReleaseDependentTodosAsync(
                connection, transaction, ownerUserId, todoId, now, cancellationToken).ConfigureAwait(false);
        }

        if (status is AgentTodoStatus.Cancelled or AgentTodoStatus.Completed)
        {
            using var cancel = Command(connection, transaction, """
                UPDATE agent_deliveries SET status = 'Cancelled', completed_at_unix_ms = @p0
                WHERE owner_user_id = @p1 AND status = 'Pending'
                  AND deduplication_key LIKE @p2
                """, now, ownerUserId, $"todo:{todoId}:%");
            await cancel.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        if (status is AgentTodoStatus.Blocked or AgentTodoStatus.Completed or AgentTodoStatus.Cancelled)
        {
            await EnqueueManagerTodoStatusAsync(connection, transaction, next,
                $"Todo 状态更新为 {status}：{next.Draft.Title}\n{result}",
                $"todo-status:{next.Id}:revision:{next.Revision}", cancellationToken)
                .ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return next;
    }

    public async Task<IReadOnlyList<AgentTodo>> ReorderTodosAsync(
        string ownerUserId,
        string roomId,
        IReadOnlyList<string> todoIds,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifiers(todoIds, nameof(todoIds), 1_000);
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var existing = await ReadTodosAsync(
            connection, transaction, ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        if (existing.Count != todoIds.Count ||
            !existing.Select(value => value.Id).ToHashSet(StringComparer.Ordinal)
                .SetEquals(todoIds))
        {
            throw Conflict("Todo order must include every team todo exactly once.");
        }

        var now = Now();
        for (var index = 0; index < todoIds.Count; index++)
        {
            using var command = Command(connection, transaction, """
                UPDATE agent_todos SET sort_order = @p0, revision = revision + 1,
                    updated_at_unix_ms = @p1
                WHERE owner_user_id = @p2 AND room_id = @p3 AND id = @p4
                """, index, now, ownerUserId, roomId, todoIds[index]);
            await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return await ListTodosAsync(ownerUserId, roomId, includeTerminal: true, cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task<AgentTodoProgress> AppendTodoProgressAsync(
        string ownerUserId,
        string todoId,
        string agentId,
        AgentTodoProgressKind kind,
        string stage,
        string detail,
        IReadOnlyList<AgentTeamAssetUpdateSuggestion>? assetUpdateSuggestions = null,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.OptionalText(stage, nameof(stage), 500);
        AgentTeamValidation.Text(detail, nameof(detail), 16_000);
        var suggestions = assetUpdateSuggestions ?? [];
        if (suggestions.Count > 8) throw AgentTeamValidation.Invalid(nameof(assetUpdateSuggestions));
        foreach (var suggestion in suggestions) suggestion.Validate();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var todo = await ReadTodoAsync(connection, transaction, ownerUserId, todoId, cancellationToken)
            .ConfigureAwait(false) ?? throw NotFound("Todo");
        if (!string.Equals(todo.Draft.AgentId, agentId, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Only the assigned Agent can append Todo progress.");
        }

        using var sequenceCommand = Command(connection, transaction, """
            SELECT COALESCE(MAX(sequence), 0) + 1 FROM agent_todo_progress
            WHERE owner_user_id = @p0 AND todo_id = @p1
            """, ownerUserId, todoId);
        var sequence = Convert.ToInt64(
            await sequenceCommand.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false));
        var progress = new AgentTodoProgress(NewId(), ownerUserId, agentId, todoId,
            sequence, kind, stage, detail, Now(), suggestions);
        using var command = Command(connection, transaction, """
            INSERT INTO agent_todo_progress (
                owner_user_id, id, todo_id, agent_id, sequence, kind, stage, detail,
                created_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8)
            """, ownerUserId, progress.Id, todoId, agentId, sequence, kind.ToString(),
            stage, detail, progress.CreatedAtUnixMs);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        for (var index = 0; index < suggestions.Count; index++)
        {
            var suggestion = suggestions[index];
            using var insertSuggestion = Command(connection, transaction, """
                INSERT INTO agent_todo_progress_suggestions (
                    owner_user_id, progress_id, position, category, title, markdown, rationale)
                VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6)
                """, ownerUserId, progress.Id, index, suggestion.Category.ToString(),
                suggestion.Title, suggestion.Markdown, suggestion.Rationale);
            await insertSuggestion.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }
        if (suggestions.Count > 0 || kind is AgentTodoProgressKind.Blocked or
            AgentTodoProgressKind.Completed or AgentTodoProgressKind.Failed)
        {
            var suggestionSummary = suggestions.Count == 0
                ? string.Empty
                : $"\n共享资产更新建议：{string.Join("、", suggestions.Select(value => value.Title))}";
            await EnqueueManagerTodoStatusAsync(connection, transaction, todo,
                $"Todo 进展 [{kind}] {stage}\n{detail}{suggestionSummary}",
                $"todo-status:{todo.Id}:progress:{progress.Id}", cancellationToken)
                .ConfigureAwait(false);
        }
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return progress;
    }

    public async Task<IReadOnlyList<AgentTodoProgress>> ListTodoProgressAsync(
        string ownerUserId,
        string todoId,
        int limit = 200,
        CancellationToken cancellationToken = default)
    {
        if (limit is < 1 or > 1_000)
        {
            throw AgentTeamValidation.Invalid(nameof(limit));
        }

        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, """
            SELECT id, agent_id, sequence, kind, stage, detail, created_at_unix_ms
            FROM agent_todo_progress
            WHERE owner_user_id = @p0 AND todo_id = @p1
            ORDER BY sequence DESC LIMIT @p2
            """, ownerUserId, todoId, limit);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentTodoProgress>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(new AgentTodoProgress(reader.GetString(0), ownerUserId,
                reader.GetString(1), todoId, reader.GetInt64(2),
                ParseEnum<AgentTodoProgressKind>(reader.GetString(3)), reader.GetString(4),
                reader.GetString(5), reader.GetInt64(6)));
        }

        var suggestions = new Dictionary<string, List<AgentTeamAssetUpdateSuggestion>>(StringComparer.Ordinal);
        using (var suggestionCommand = Command(connection, null, """
            SELECT s.progress_id, s.category, s.title, s.markdown, s.rationale
            FROM agent_todo_progress_suggestions s
            JOIN agent_todo_progress p ON p.owner_user_id = s.owner_user_id AND p.id = s.progress_id
            WHERE p.owner_user_id = @p0 AND p.todo_id = @p1
            ORDER BY p.sequence, s.position
            """, ownerUserId, todoId))
        await using (var suggestionReader = await suggestionCommand.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await suggestionReader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var progressId = suggestionReader.GetString(0);
                if (!suggestions.TryGetValue(progressId, out var values))
                    suggestions[progressId] = values = [];
                values.Add(new AgentTeamAssetUpdateSuggestion(
                    ParseEnum<AgentTeamAssetCategory>(suggestionReader.GetString(1)),
                    suggestionReader.GetString(2), suggestionReader.GetString(3),
                    suggestionReader.GetString(4)));
            }
        }

        for (var index = 0; index < output.Count; index++)
            output[index] = output[index] with
            {
                AssetUpdateSuggestions = suggestions.GetValueOrDefault(output[index].Id) ?? [],
            };

        output.Reverse();
        return output;
    }

    private static AgentTodo ReadTodo(SqliteDataReader reader)
    {
        var todo = new AgentTodo(
            reader.GetString(1),
            reader.GetString(0),
            new AgentTodoDraft(
                reader.GetString(2),
                reader.GetString(3),
                reader.GetString(4),
                reader.GetString(5),
                ParseEnum<AgentTodoPriority>(reader.GetString(6)),
                DeserializeStrings(reader.GetString(7)),
                reader.IsDBNull(8) ? null : reader.GetString(8)),
            ParseEnum<AgentTodoStatus>(reader.GetString(9)),
            reader.GetString(10),
            reader.GetInt32(11),
            reader.GetInt64(12),
            reader.GetInt64(13),
            reader.GetInt64(14));
        todo.Validate();
        return todo;
    }

    private static async Task<AgentTodo?> ReadTodoAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {TodoColumns} FROM agent_todos WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, todoId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadTodo(reader) : null;
    }

    private static async Task<IReadOnlyList<AgentTodo>> ReadTodosAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {TodoColumns} FROM agent_todos WHERE owner_user_id = @p0 AND room_id = @p1 " +
            "ORDER BY sort_order, created_at_unix_ms, id", ownerUserId, roomId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentTodo>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(ReadTodo(reader));
        }

        return output;
    }

    private static async Task<bool> DependenciesCompleteAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        IReadOnlyList<string> dependencyIds,
        CancellationToken cancellationToken)
    {
        foreach (var id in dependencyIds)
        {
            var dependency = await ReadTodoAsync(
                connection, transaction, ownerUserId, id, cancellationToken).ConfigureAwait(false);
            if (dependency?.Status != AgentTodoStatus.Completed)
            {
                return false;
            }
        }

        return true;
    }

    private static async Task<int> NextTodoOrderAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT COALESCE(MAX(sort_order), -1) + 1 FROM agent_todos
            WHERE owner_user_id = @p0 AND room_id = @p1
            """, ownerUserId, roomId);
        return Convert.ToInt32(await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false));
    }

    private static async Task EnqueueTodoAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentTodo todo,
        CancellationToken cancellationToken)
    {
        var now = Now();
        var messageId = await InsertSystemMessageAsync(connection, transaction, todo.OwnerUserId,
            todo.Draft.TeamRoomId, $"Todo：{todo.Draft.Title}\n{todo.Draft.Detail}", now,
            cancellationToken).ConfigureAwait(false);
        _ = await InsertDeliveryAsync(connection, transaction, todo.OwnerUserId,
            todo.Draft.TeamRoomId, messageId, messageId, todo.Draft.AgentId,
            AgentDeliveryTrigger.Todo, 0, $"todo:{todo.Id}:revision:{todo.Revision}", now,
            cancellationToken).ConfigureAwait(false);
    }

    private static async Task ReleaseDependentTodosAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string completedTodoId,
        long now,
        CancellationToken cancellationToken)
    {
        var candidates = new List<AgentTodo>();
        using (var command = Command(connection, transaction,
            $"SELECT {TodoColumns} FROM agent_todos WHERE owner_user_id = @p0 AND status = 'Pending'",
            ownerUserId))
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var todo = ReadTodo(reader);
                if (todo.Draft.Dependencies.Contains(completedTodoId, StringComparer.Ordinal))
                {
                    candidates.Add(todo);
                }
            }
        }

        foreach (var todo in candidates)
        {
            if (!await DependenciesCompleteAsync(connection, transaction, ownerUserId,
                todo.Draft.Dependencies, cancellationToken).ConfigureAwait(false))
            {
                continue;
            }

            var ready = todo with
            {
                Status = AgentTodoStatus.Ready,
                Revision = todo.Revision + 1,
                UpdatedAtUnixMs = now,
            };
            using var update = Command(connection, transaction, """
                UPDATE agent_todos SET status = 'Ready', revision = @p0, updated_at_unix_ms = @p1
                WHERE owner_user_id = @p2 AND id = @p3 AND revision = @p4
                """, ready.Revision, now, ownerUserId, todo.Id, todo.Revision);
            if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) == 1)
            {
                await EnqueueTodoAsync(connection, transaction, ready, cancellationToken).ConfigureAwait(false);
            }
        }
    }

    private static async Task EnqueueManagerTodoStatusAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentTodo todo,
        string content,
        string deduplicationKey,
        CancellationToken cancellationToken)
    {
        using var managerCommand = Command(connection, transaction, """
            SELECT project_manager_agent_id FROM agent_rooms
            WHERE owner_user_id = @p0 AND id = @p1 AND status = 'Active'
            """, todo.OwnerUserId, todo.Draft.TeamRoomId);
        var managerId = await managerCommand.ExecuteScalarAsync(cancellationToken)
            .ConfigureAwait(false) as string;
        if (managerId is null || managerId == todo.Draft.AgentId) return;
        var now = Now();
        var messageId = await InsertSystemMessageAsync(connection, transaction, todo.OwnerUserId,
            todo.Draft.TeamRoomId, content, now, cancellationToken).ConfigureAwait(false);
        _ = await InsertDeliveryAsync(connection, transaction, todo.OwnerUserId,
            todo.Draft.TeamRoomId, messageId, messageId, managerId,
            AgentDeliveryTrigger.TodoStatus, 0, deduplicationKey, now, cancellationToken)
            .ConfigureAwait(false);
    }
}
