using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private const string TodoColumns = """
        owner_user_id, id, room_id, agent_id, title, detail, priority,
        dependency_ids_json, source_message_id, execution_contract_json,
        execution_plan_json, status, result, sort_order, revision,
        created_at_unix_ms, updated_at_unix_ms
        """;
    private const string QualifiedTodoColumns = """
        todo.owner_user_id, todo.id, todo.room_id, todo.agent_id, todo.title,
        todo.detail, todo.priority, todo.dependency_ids_json, todo.source_message_id,
        todo.execution_contract_json, todo.execution_plan_json, todo.status, todo.result,
        todo.sort_order, todo.revision,
        todo.created_at_unix_ms, todo.updated_at_unix_ms
        """;

    private static string TodoDisplayOrderSql(string prefix = "") => $"""
        CASE {prefix}status
            WHEN 'InProgress' THEN 0
            WHEN 'Ready' THEN 1
            WHEN 'Pending' THEN 2
            WHEN 'Blocked' THEN 3
            WHEN 'Completed' THEN 4
            WHEN 'Cancelled' THEN 5
            ELSE 6
        END,
        CASE {prefix}priority
            WHEN 'Urgent' THEN 3
            WHEN 'High' THEN 2
            WHEN 'Normal' THEN 1
            ELSE 0
        END DESC,
        {prefix}sort_order, {prefix}created_at_unix_ms, {prefix}id
        """;

    public async Task<IReadOnlyList<AgentTodo>> ListTodosAsync(
        string ownerUserId,
        string roomId,
        bool includeTerminal = true,
        int limit = 200,
        CancellationToken cancellationToken = default)
    {
        ValidateTodoListLimit(limit);
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {TodoColumns} FROM agent_todos WHERE owner_user_id = @p0 AND room_id = @p1" +
            (includeTerminal ? string.Empty : " AND status NOT IN ('Completed', 'Cancelled')") +
            $" ORDER BY {TodoDisplayOrderSql()} LIMIT @p2", ownerUserId, roomId, limit);
        var output = new List<AgentTodo>();
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                output.Add(ReadTodo(reader));
        }

        return await AttachTodoSourcesAsync(connection, null, ownerUserId, output,
            cancellationToken).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<AgentTodo>> ListProjectTodosAsync(
        string ownerUserId,
        string projectId,
        bool includeTerminal = true,
        int limit = 200,
        CancellationToken cancellationToken = default)
    {
        ValidateTodoListLimit(limit);
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {QualifiedTodoColumns} " +
            "FROM agent_todos todo JOIN agent_rooms room " +
            "ON room.owner_user_id = todo.owner_user_id AND room.id = todo.room_id " +
            "WHERE todo.owner_user_id = @p0 AND room.project_id = @p1" +
            (includeTerminal ? string.Empty :
                " AND todo.status NOT IN ('Completed', 'Cancelled')") +
            $" ORDER BY {TodoDisplayOrderSql("todo.")} LIMIT @p2",
            ownerUserId, projectId, limit);
        var output = new List<AgentTodo>();
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                output.Add(ReadTodo(reader));
        }
        return await AttachTodoSourcesAsync(connection, null, ownerUserId, output,
            cancellationToken).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<AgentTodo>> ListTodosByIdsAsync(
        string ownerUserId,
        IReadOnlyList<string> todoIds,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifiers(todoIds, nameof(todoIds), 100);
        if (todoIds.Count == 0) return [];

        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var placeholders = string.Join(", ", Enumerable.Range(1, todoIds.Count)
            .Select(index => $"@p{index}"));
        using var command = Command(connection, null,
            $"SELECT {TodoColumns} FROM agent_todos " +
            $"WHERE owner_user_id = @p0 AND id IN ({placeholders})",
            [ownerUserId, .. todoIds.Cast<object>()]);
        var found = new Dictionary<string, AgentTodo>(StringComparer.Ordinal);
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var todo = ReadTodo(reader);
                found[todo.Id] = todo;
            }
        }

        var ordered = todoIds.Where(found.ContainsKey).Select(id => found[id]).ToArray();
        return await AttachTodoSourcesAsync(connection, null, ownerUserId, ordered,
            cancellationToken).ConfigureAwait(false);
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
        var now = Now();
        var contract = (draft.ExecutionContract ?? new AgentTodoExecutionContract())
            .Normalized(draft.Title, draft.Detail);
        var plan = (draft.ExecutionPlan ?? new AgentTodoExecutionPlan()).Normalized(now);
        var sources = draft.Sources.ToList();
        if (draft.SourceMessageId is not null && !sources.Any(value =>
            value.ConversationId == draft.TeamRoomId && value.MessageId == draft.SourceMessageId))
        {
            sources.Insert(0, new AgentTodoSourceDraft(
                draft.TeamRoomId, draft.SourceMessageId, AgentTodoSourceRelation.Created));
        }
        draft = draft with
        {
            ExecutionContract = contract,
            ExecutionPlan = plan,
            SourceLinks = sources,
        };
        draft.Validate();
        var todoId = NewId();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireRoomAsync(connection, transaction, ownerUserId, draft.TeamRoomId,
            requireActive: true, cancellationToken).ConfigureAwait(false);
        await RequireActiveMemberAsync(connection, transaction, ownerUserId, draft.TeamRoomId,
            draft.AgentId, cancellationToken).ConfigureAwait(false);
        foreach (var source in draft.Sources)
        {
            await RequireMessageAsync(connection, transaction, ownerUserId,
                source.ConversationId, source.MessageId, cancellationToken).ConfigureAwait(false);
        }
        foreach (var dependencyId in draft.Dependencies)
        {
            var dependency = await ReadTodoAsync(
                connection, transaction, ownerUserId, dependencyId, cancellationToken,
                includeSources: false).ConfigureAwait(false)
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
        var sourceLinks = draft.Sources.Select(value => new AgentTodoSourceLink(todoId,
            value.ConversationId, value.MessageId, value.Relation, now)).ToArray();
        var todo = new AgentTodo(todoId, ownerUserId, draft, status, string.Empty,
            sortOrder, 1, now, now, sourceLinks);
        todo.Validate();
        using (var command = Command(connection, transaction, $"""
            INSERT INTO agent_todos ({TodoColumns})
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9, @p10,
                @p11, @p12, @p13, @p14, @p15, @p16)
            """, ownerUserId, todoId, draft.TeamRoomId, draft.AgentId, draft.Title,
            draft.Detail, draft.Priority.ToString(), Serialize(draft.Dependencies),
            DbValue(draft.SourceMessageId), Serialize(contract), Serialize(plan), status.ToString(),
            string.Empty, sortOrder, 1, now, now))
        {
            await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        foreach (var source in sourceLinks)
        {
            using var insertSource = Command(connection, transaction, """
                INSERT INTO agent_todo_sources (
                    owner_user_id, todo_id, conversation_id, message_id, relation,
                    created_at_unix_ms)
                VALUES (@p0, @p1, @p2, @p3, @p4, @p5)
                """, ownerUserId, todoId, source.ConversationId, source.MessageId,
                source.Relation.ToString(), source.CreatedAtUnixMs);
            await insertSource.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        if (status == AgentTodoStatus.Ready && await StartNextReadyTodoAsync(connection,
            transaction, ownerUserId, draft.AgentId, now, cancellationToken)
            .ConfigureAwait(false) is not null)
        {
            todo = todo with
            {
                Status = AgentTodoStatus.InProgress,
                Revision = todo.Revision + 1,
            };
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

        if (status == AgentTodoStatus.InProgress && current.Status != AgentTodoStatus.InProgress)
            throw Conflict("Todo execution can only be started by the local scheduler.");

        var agentId = assignedAgentId ?? current.Draft.AgentId;
        if (current.Status == AgentTodoStatus.InProgress &&
            (status == AgentTodoStatus.Ready || status == AgentTodoStatus.InProgress &&
             !string.Equals(agentId, current.Draft.AgentId, StringComparison.Ordinal)))
        {
            throw Conflict("A running Todo must complete, block, or cancel before rescheduling.");
        }
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
                  AND (deduplication_key = @p2 OR deduplication_key LIKE @p3)
                """, now, ownerUserId, $"todo:{todoId}", $"todo:{todoId}:revision:%");
            await cancel.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        if (status is AgentTodoStatus.Blocked or AgentTodoStatus.Completed or AgentTodoStatus.Cancelled)
        {
            await EnqueueManagerTodoStatusAsync(connection, transaction, next,
                $"Todo 状态更新为 {status}：{next.Draft.Title}\n{result}",
                $"todo-status:{next.Id}:revision:{next.Revision}", cancellationToken)
                .ConfigureAwait(false);
        }

        if (status is AgentTodoStatus.Pending or AgentTodoStatus.Ready or AgentTodoStatus.Blocked or
            AgentTodoStatus.Completed or AgentTodoStatus.Cancelled)
        {
            await ScheduleReadyAgentsAsync(connection, transaction, ownerUserId, now,
                cancellationToken).ConfigureAwait(false);
            next = await ReadTodoAsync(connection, transaction, ownerUserId, todoId,
                cancellationToken).ConfigureAwait(false) ?? next;
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
        if (todoIds.Count == 0) return [];
        return await ListTodosAsync(ownerUserId, roomId, includeTerminal: true,
                limit: todoIds.Count, cancellationToken)
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
        var contract = (System.Text.Json.JsonSerializer.Deserialize<AgentTodoExecutionContract>(
            reader.GetString(9), JsonOptions) ?? new AgentTodoExecutionContract())
            .Normalized(reader.GetString(4), reader.GetString(5));
        var plan = (System.Text.Json.JsonSerializer.Deserialize<AgentTodoExecutionPlan>(
            reader.GetString(10), JsonOptions) ?? new AgentTodoExecutionPlan())
            .Normalized(reader.GetInt64(15));
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
                reader.IsDBNull(8) ? null : reader.GetString(8),
                contract, plan),
            ParseEnum<AgentTodoStatus>(reader.GetString(11)),
            reader.GetString(12),
            reader.GetInt32(13),
            reader.GetInt64(14),
            reader.GetInt64(15),
            reader.GetInt64(16));
        todo.Validate();
        return todo;
    }

    private static async Task<AgentTodo?> ReadTodoAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken,
        bool includeSources = true)
    {
        using var command = Command(connection, transaction,
            $"SELECT {TodoColumns} FROM agent_todos WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, todoId);
        AgentTodo? todo;
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
            todo = await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
                ? ReadTodo(reader) : null;
        if (todo is null || !includeSources) return todo;
        return (await AttachTodoSourcesAsync(connection, transaction, ownerUserId, [todo],
            cancellationToken).ConfigureAwait(false))[0];
    }

    private static void ValidateTodoListLimit(int limit)
    {
        if (limit is < 1 or > 1_000)
        {
            throw AgentTeamValidation.Invalid(nameof(limit));
        }
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
        var output = new List<AgentTodo>();
        await using (var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
                output.Add(ReadTodo(reader));
        }

        return await AttachTodoSourcesAsync(connection, transaction, ownerUserId, output,
            cancellationToken).ConfigureAwait(false);
    }

    private static async Task<IReadOnlyList<AgentTodo>> AttachTodoSourcesAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        IReadOnlyList<AgentTodo> todos,
        CancellationToken cancellationToken)
    {
        if (todos.Count == 0) return todos;
        var sources = todos.ToDictionary(value => value.Id,
            _ => new List<AgentTodoSourceLink>(), StringComparer.Ordinal);
        foreach (var batch in todos.Chunk(400))
        {
            var ids = batch.Select(value => value.Id).ToArray();
            var placeholders = string.Join(", ", Enumerable.Range(1, ids.Length)
                .Select(index => $"@p{index}"));
            using var command = Command(connection, transaction, $"""
                SELECT todo_id, conversation_id, message_id, relation, created_at_unix_ms
                FROM agent_todo_sources
                WHERE owner_user_id = @p0 AND todo_id IN ({placeholders})
                ORDER BY created_at_unix_ms, conversation_id, message_id
                """, [ownerUserId, .. ids.Cast<object>()]);
            await using var reader = await command.ExecuteReaderAsync(cancellationToken)
                .ConfigureAwait(false);
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var todoId = reader.GetString(0);
                sources[todoId].Add(new AgentTodoSourceLink(todoId, reader.GetString(1),
                    reader.GetString(2), ParseEnum<AgentTodoSourceRelation>(reader.GetString(3)),
                    reader.GetInt64(4)));
            }
        }

        return todos.Select(value => value with { SourceLinks = sources[value.Id] }).ToArray();
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
