using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private const string ProfileColumns = """
        owner_user_id, id, name, description, role_prompt, model_config_id, thinking_level,
        profession_key, default_plugin_ids_json, default_skill_ids_json, heartbeat_enabled,
        heartbeat_interval_seconds, heartbeat_prompt, status, created_at_unix_ms,
        updated_at_unix_ms, last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms
        """;

    private const string RoomColumns = """
        owner_user_id, id, project_id, name, goal, default_agent_id, project_manager_agent_id,
        conversation_kind, direct_key, status, created_at_unix_ms, updated_at_unix_ms
        """;

    private const string MemberColumns = """
        owner_user_id, room_id, agent_id, role, responsibility, plugin_allowlist_json,
        status, joined_at_unix_ms
        """;

    public async Task<IReadOnlyList<AgentProfile>> ListAgentsAsync(
        string ownerUserId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {ProfileColumns} FROM agent_profiles WHERE owner_user_id = @p0" +
            (includeArchived ? string.Empty : " AND status = 'Active'") +
            " ORDER BY name COLLATE NOCASE, id", ownerUserId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentProfile>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(ReadProfile(reader));
        }

        return output;
    }

    public async Task<AgentProfile?> GetAgentAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {ProfileColumns} FROM agent_profiles WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, agentId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadProfile(reader) : null;
    }

    public async Task<AgentProfile> CreateAgentAsync(
        string ownerUserId,
        AgentProfileDraft draft,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        draft.Validate();
        var now = Now();
        var record = new AgentProfile(
            NewId(), ownerUserId, draft, AgentProfileStatus.Active, now, now,
            null, draft.HeartbeatEnabled ? now + draft.HeartbeatIntervalSeconds * 1_000L : null);
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, $"""
            INSERT INTO agent_profiles ({ProfileColumns})
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9, @p10, @p11,
                @p12, @p13, @p14, @p15, @p16, @p17)
            """, ownerUserId, record.Id, draft.Name, draft.Description, draft.RolePrompt,
            draft.ModelConfigId, DbValue(draft.ThinkingLevel), draft.ProfessionKey,
            Serialize(draft.Plugins), Serialize(draft.Skills), draft.HeartbeatEnabled ? 1 : 0,
            draft.HeartbeatIntervalSeconds, draft.HeartbeatPrompt, record.Status.ToString(), now, now,
            DBNull.Value, DbValue(record.NextHeartbeatAtUnixMs));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        return record;
    }

    public async Task<AgentProfile> UpdateAgentAsync(
        string ownerUserId,
        string agentId,
        AgentProfileDraft draft,
        AgentProfileStatus status,
        CancellationToken cancellationToken = default)
    {
        draft.Validate();
        var current = await GetAgentAsync(ownerUserId, agentId, cancellationToken).ConfigureAwait(false)
            ?? throw NotFound("Agent");
        var now = Math.Max(Now(), current.UpdatedAtUnixMs);
        long? nextHeartbeat = draft.HeartbeatEnabled
            ? current.NextHeartbeatAtUnixMs ?? now + draft.HeartbeatIntervalSeconds * 1_000L
            : null;
        var record = current with
        {
            Draft = draft,
            Status = status,
            UpdatedAtUnixMs = now,
            NextHeartbeatAtUnixMs = nextHeartbeat,
        };
        record.Validate();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, """
            UPDATE agent_profiles SET name = @p0, description = @p1, role_prompt = @p2,
                model_config_id = @p3, thinking_level = @p4, profession_key = @p5,
                default_plugin_ids_json = @p6, default_skill_ids_json = @p7,
                heartbeat_enabled = @p8, heartbeat_interval_seconds = @p9,
                heartbeat_prompt = @p10, status = @p11, updated_at_unix_ms = @p12,
                next_heartbeat_at_unix_ms = @p13
            WHERE owner_user_id = @p14 AND id = @p15
            """, draft.Name, draft.Description, draft.RolePrompt, draft.ModelConfigId,
            DbValue(draft.ThinkingLevel), draft.ProfessionKey, Serialize(draft.Plugins),
            Serialize(draft.Skills), draft.HeartbeatEnabled ? 1 : 0,
            draft.HeartbeatIntervalSeconds, draft.HeartbeatPrompt, status.ToString(), now,
            DbValue(nextHeartbeat), ownerUserId, agentId);
        if (await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
        {
            throw NotFound("Agent");
        }

        return record;
    }

    public async Task<IReadOnlyList<AgentRoom>> ListRoomsAsync(
        string ownerUserId,
        string? projectId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var projectClause = projectId is null ? string.Empty : " AND project_id = @p1";
        object[] values = projectId is null ? [ownerUserId] : [ownerUserId, projectId];
        using var command = Command(connection, null,
            $"SELECT {RoomColumns} FROM agent_rooms WHERE owner_user_id = @p0{projectClause}" +
            (includeArchived ? string.Empty : " AND status = 'Active'") +
            " ORDER BY updated_at_unix_ms DESC, id", values);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentRoom>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(ReadRoom(reader));
        }

        return output;
    }

    public async Task<AgentRoom?> GetRoomAsync(
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {RoomColumns} FROM agent_rooms WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, roomId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadRoom(reader) : null;
    }

    public async Task<AgentRoom> CreateRoomAsync(
        string ownerUserId,
        string projectId,
        AgentRoomDraft draft,
        string? projectManagerAgentId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(projectId, nameof(projectId));
        draft.Validate();
        if (projectManagerAgentId is not null)
        {
            AgentTeamValidation.Identifier(projectManagerAgentId, nameof(projectManagerAgentId));
        }

        var now = Now();
        var room = new AgentRoom(NewId(), ownerUserId, projectId, draft, projectManagerAgentId,
            projectManagerAgentId, AgentConversationKind.ProjectTeam, null, AgentRoomStatus.Active,
            now, now);
        room.Validate();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        if (projectManagerAgentId is not null)
        {
            await RequireAgentAsync(connection, transaction, ownerUserId, projectManagerAgentId,
                requireActive: true, cancellationToken).ConfigureAwait(false);
        }

        await InsertRoomAsync(connection, transaction, room, cancellationToken).ConfigureAwait(false);
        if (projectManagerAgentId is not null)
        {
            await InsertMemberAsync(connection, transaction, ownerUserId, room.Id,
                projectManagerAgentId, new AgentRoomMemberDraft("project_manager", draft.Goal),
                AgentMemberStatus.Active, now, cancellationToken).ConfigureAwait(false);
            await EnqueueTeamAssetMaintenanceAsync(connection, transaction, room,
                projectManagerAgentId, now, cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return room;
    }

    public async Task<AgentRoom> OpenHumanAgentDirectAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(agentId, nameof(agentId));
        var directKey = $"human:{agentId}";
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireAgentAsync(connection, transaction, ownerUserId, agentId, requireActive: true,
            cancellationToken).ConfigureAwait(false);
        using (var existingCommand = Command(connection, transaction,
            $"SELECT {RoomColumns} FROM agent_rooms WHERE owner_user_id = @p0 AND direct_key = @p1",
            ownerUserId, directKey))
        await using (var reader = await existingCommand.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            if (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var existing = ReadRoom(reader);
                await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
                return existing;
            }
        }

        var profile = await ReadProfileAsync(connection, transaction, ownerUserId, agentId, cancellationToken)
            .ConfigureAwait(false) ?? throw NotFound("Agent");
        var now = Now();
        var room = new AgentRoom(NewId(), ownerUserId, "direct", new($"与 {profile.Draft.Name} 私聊"),
            agentId, null, AgentConversationKind.HumanAgentDirect, directKey, AgentRoomStatus.Active,
            now, now);
        await InsertRoomAsync(connection, transaction, room, cancellationToken).ConfigureAwait(false);
        await InsertMemberAsync(connection, transaction, ownerUserId, room.Id, agentId,
            new AgentRoomMemberDraft("direct", profile.Draft.Description), AgentMemberStatus.Active,
            now, cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return room;
    }

    public async Task<AgentRoom> OpenAgentDirectAsync(
        string ownerUserId,
        string sourceAgentId,
        string targetAgentId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(sourceAgentId, nameof(sourceAgentId));
        AgentTeamValidation.Identifier(targetAgentId, nameof(targetAgentId));
        if (sourceAgentId == targetAgentId) throw AgentTeamValidation.Invalid(nameof(targetAgentId));
        var ids = new[] { sourceAgentId, targetAgentId }.Order(StringComparer.Ordinal).ToArray();
        var directKey = $"agents:{ids[0]}:{ids[1]}";
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireAgentAsync(connection, transaction, ownerUserId, sourceAgentId, true,
            cancellationToken).ConfigureAwait(false);
        await RequireAgentAsync(connection, transaction, ownerUserId, targetAgentId, true,
            cancellationToken).ConfigureAwait(false);
        using (var existingCommand = Command(connection, transaction,
            $"SELECT {RoomColumns} FROM agent_rooms WHERE owner_user_id = @p0 AND direct_key = @p1",
            ownerUserId, directKey))
        await using (var reader = await existingCommand.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false))
        {
            if (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var existing = ReadRoom(reader);
                await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
                return existing;
            }
        }

        var source = await ReadProfileAsync(connection, transaction, ownerUserId, sourceAgentId,
            cancellationToken).ConfigureAwait(false) ?? throw NotFound("Agent");
        var target = await ReadProfileAsync(connection, transaction, ownerUserId, targetAgentId,
            cancellationToken).ConfigureAwait(false) ?? throw NotFound("Agent");
        var now = Now();
        var room = new AgentRoom(NewId(), ownerUserId, "direct",
            new($"{source.Draft.Name} ↔ {target.Draft.Name}"), targetAgentId, null,
            AgentConversationKind.AgentAgentDirect, directKey, AgentRoomStatus.Active, now, now);
        await InsertRoomAsync(connection, transaction, room, cancellationToken).ConfigureAwait(false);
        await InsertMemberAsync(connection, transaction, ownerUserId, room.Id, sourceAgentId,
            new("direct", source.Draft.Description), AgentMemberStatus.Active, now,
            cancellationToken).ConfigureAwait(false);
        await InsertMemberAsync(connection, transaction, ownerUserId, room.Id, targetAgentId,
            new("direct", target.Draft.Description), AgentMemberStatus.Active, now,
            cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return room;
    }

    public async Task<AgentRoom> UpdateRoomAsync(
        string ownerUserId,
        string roomId,
        AgentRoomDraft draft,
        string? defaultAgentId,
        string? projectManagerAgentId,
        AgentRoomStatus status,
        CancellationToken cancellationToken = default)
    {
        draft.Validate();
        var current = await GetRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false)
            ?? throw NotFound("Agent room");
        if (current.IsDirect && projectManagerAgentId is not null)
        {
            throw AgentTeamValidation.Invalid(nameof(projectManagerAgentId));
        }

        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        foreach (var agentId in new[] { defaultAgentId, projectManagerAgentId }.OfType<string>().Distinct())
        {
            await RequireActiveMemberAsync(connection, transaction, ownerUserId, roomId, agentId,
                cancellationToken).ConfigureAwait(false);
        }

        var now = Math.Max(Now(), current.UpdatedAtUnixMs);
        using var command = Command(connection, transaction, """
            UPDATE agent_rooms SET name = @p0, goal = @p1, default_agent_id = @p2,
                project_manager_agent_id = @p3, status = @p4, updated_at_unix_ms = @p5
            WHERE owner_user_id = @p6 AND id = @p7
            """, draft.Name, draft.Goal, DbValue(defaultAgentId), DbValue(projectManagerAgentId),
            status.ToString(), now, ownerUserId, roomId);
        if (await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
        {
            throw NotFound("Agent room");
        }

        var updated = current with
        {
            Draft = draft,
            DefaultAgentId = defaultAgentId,
            ProjectManagerAgentId = projectManagerAgentId,
            Status = status,
            UpdatedAtUnixMs = now,
        };
        if (status == AgentRoomStatus.Active && projectManagerAgentId is not null)
        {
            await EnqueueTeamAssetMaintenanceAsync(connection, transaction, updated,
                projectManagerAgentId, now, cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return updated;
    }

    public async Task<IReadOnlyList<AgentRoomMember>> ListMembersAsync(
        string ownerUserId,
        string roomId,
        bool includeRemoved = false,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {MemberColumns} FROM agent_room_members WHERE owner_user_id = @p0 AND room_id = @p1" +
            (includeRemoved ? string.Empty : " AND status = 'Active'") +
            " ORDER BY joined_at_unix_ms, agent_id", ownerUserId, roomId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentRoomMember>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(ReadMember(reader));
        }

        return output;
    }

    public async Task<AgentRoomMember> UpsertMemberAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        AgentRoomMemberDraft draft,
        AgentMemberStatus status = AgentMemberStatus.Active,
        CancellationToken cancellationToken = default)
    {
        draft.Validate();
        var now = Now();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireRoomAsync(connection, transaction, ownerUserId, roomId, requireActive: true,
            cancellationToken).ConfigureAwait(false);
        await RequireAgentAsync(connection, transaction, ownerUserId, agentId, requireActive: true,
            cancellationToken).ConfigureAwait(false);
        await InsertMemberAsync(connection, transaction, ownerUserId, roomId, agentId, draft,
            status, now, cancellationToken).ConfigureAwait(false);
        if (status == AgentMemberStatus.Removed)
        {
            using var clear = Command(connection, transaction, """
                UPDATE agent_rooms SET
                    default_agent_id = CASE WHEN default_agent_id = @p0 THEN NULL ELSE default_agent_id END,
                    project_manager_agent_id = CASE WHEN project_manager_agent_id = @p0 THEN NULL ELSE project_manager_agent_id END,
                    updated_at_unix_ms = @p1
                WHERE owner_user_id = @p2 AND id = @p3
                """, agentId, now, ownerUserId, roomId);
            await clear.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return new AgentRoomMember(ownerUserId, roomId, agentId, draft, status, now);
    }

    private static async Task InsertRoomAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, $"""
            INSERT INTO agent_rooms ({RoomColumns})
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9, @p10, @p11)
            """, room.OwnerUserId, room.Id, room.ProjectId, room.Draft.Name, room.Draft.Goal,
            DbValue(room.DefaultAgentId), DbValue(room.ProjectManagerAgentId), room.Kind.ToString(),
            DbValue(room.DirectKey), room.Status.ToString(), room.CreatedAtUnixMs, room.UpdatedAtUnixMs);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static async Task InsertMemberAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string agentId,
        AgentRoomMemberDraft draft,
        AgentMemberStatus status,
        long now,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            INSERT INTO agent_room_members (
                owner_user_id, room_id, agent_id, role, responsibility, plugin_allowlist_json,
                status, joined_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7)
            ON CONFLICT(owner_user_id, room_id, agent_id) DO UPDATE SET
                role = excluded.role, responsibility = excluded.responsibility,
                plugin_allowlist_json = excluded.plugin_allowlist_json, status = excluded.status
            """, ownerUserId, roomId, agentId, draft.Role, draft.Responsibility,
            Serialize(draft.Plugins), status.ToString(), now);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static async Task<AgentProfile?> ReadProfileAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {ProfileColumns} FROM agent_profiles WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, agentId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadProfile(reader) : null;
    }
}
