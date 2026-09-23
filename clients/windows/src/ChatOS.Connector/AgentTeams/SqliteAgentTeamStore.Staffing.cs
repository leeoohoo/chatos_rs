using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private const string StaffingProposalColumns = """
        owner_user_id, id, source_room_id, proposer_agent_id, source_delivery_id,
        request_key, draft_json, status, created_agent_id, created_at_unix_ms,
        resolved_at_unix_ms
        """;

    public async Task<AgentStaffingProposal> CreateStaffingProposalAsync(
        string ownerUserId,
        string sourceRoomId,
        string proposerAgentId,
        string sourceDeliveryId,
        string requestKey,
        AgentStaffingProposalDraft draft,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(sourceRoomId, nameof(sourceRoomId));
        AgentTeamValidation.Identifier(proposerAgentId, nameof(proposerAgentId));
        AgentTeamValidation.Identifier(sourceDeliveryId, nameof(sourceDeliveryId));
        AgentTeamValidation.Identifier(requestKey, nameof(requestKey));
        draft.Validate();
        var draftJson = Serialize(draft);
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var room = await ReadRoomAsync(connection, transaction, ownerUserId, sourceRoomId,
            cancellationToken).ConfigureAwait(false) ?? throw NotFound("Agent room");
        var proposer = await ReadProfileAsync(connection, transaction, ownerUserId,
            proposerAgentId, cancellationToken).ConfigureAwait(false) ?? throw NotFound("Agent");
        if (room.Status != AgentRoomStatus.Active || proposer.Status != AgentProfileStatus.Active ||
            !AgentProfilePermissions.CanManageStaff(proposer))
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "The Agent is not allowed to manage team members.");
        await RequireActiveMemberAsync(connection, transaction, ownerUserId, sourceRoomId,
            proposerAgentId, cancellationToken).ConfigureAwait(false);
        await RequireRunningDeliveryAsync(connection, transaction, ownerUserId, sourceRoomId,
            proposerAgentId, sourceDeliveryId, cancellationToken).ConfigureAwait(false);

        var existing = await ReadProposalByRequestAsync(connection, transaction, ownerUserId,
            sourceRoomId, proposerAgentId, sourceDeliveryId, requestKey, cancellationToken)
            .ConfigureAwait(false);
        if (existing is not null)
        {
            if (existing.Draft != draft)
                throw Conflict("The staffing proposal request key was reused with different content.");
            await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
            return existing;
        }

        await ValidateStaffingTargetAsync(connection, transaction, ownerUserId, room,
            proposerAgentId, draft, cancellationToken).ConfigureAwait(false);
        var now = Now();
        var proposal = new AgentStaffingProposal(NewId(), ownerUserId, sourceRoomId,
            proposerAgentId, sourceDeliveryId, requestKey, draft,
            AgentStaffingProposalStatus.Pending, null, now, null);
        proposal.Validate();
        using var insert = Command(connection, transaction, """
            INSERT INTO agent_staffing_proposals (
                owner_user_id, id, kind, source_room_id, proposer_agent_id,
                source_delivery_id, request_key, draft_json, status, created_agent_id,
                created_at_unix_ms, resolved_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, 'Pending', NULL, @p8, NULL)
            """, ownerUserId, proposal.Id, draft.Kind.ToString(), sourceRoomId,
            proposerAgentId, sourceDeliveryId, requestKey, draftJson, now);
        await insert.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return proposal;
    }

    public async Task<IReadOnlyList<AgentStaffingProposal>> ListStaffingProposalsAsync(
        string ownerUserId,
        string sourceRoomId,
        AgentStaffingProposalStatus? status = null,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(sourceRoomId, nameof(sourceRoomId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        await RequireRoomAsync(connection, null, ownerUserId, sourceRoomId, false,
            cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {StaffingProposalColumns} FROM agent_staffing_proposals " +
            "WHERE owner_user_id = @p0 AND source_room_id = @p1" +
            (status is null ? string.Empty : " AND status = @p2") +
            " ORDER BY created_at_unix_ms DESC, id",
            status is null ? [ownerUserId, sourceRoomId] :
                [ownerUserId, sourceRoomId, status.Value.ToString()]);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        var output = new List<AgentStaffingProposal>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            output.Add(ReadStaffingProposal(reader));
        return output;
    }

    public async Task<AgentStaffingProposal> ResolveStaffingProposalAsync(
        string ownerUserId,
        string sourceRoomId,
        string proposalId,
        bool approve,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        AgentTeamValidation.Identifier(sourceRoomId, nameof(sourceRoomId));
        AgentTeamValidation.Identifier(proposalId, nameof(proposalId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var proposal = await ReadProposalAsync(connection, transaction, ownerUserId,
            sourceRoomId, proposalId, cancellationToken).ConfigureAwait(false)
            ?? throw NotFound("Staffing proposal");
        var desired = approve ? AgentStaffingProposalStatus.Approved :
            AgentStaffingProposalStatus.Rejected;
        if (proposal.Status == desired)
        {
            await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
            return proposal;
        }
        if (proposal.Status != AgentStaffingProposalStatus.Pending)
            throw Conflict("The staffing proposal was already resolved differently.");

        string? createdAgentId = null;
        if (approve)
            createdAgentId = await ApplyApprovedProposalAsync(connection, transaction, proposal,
                cancellationToken).ConfigureAwait(false);
        var now = Now();
        using var update = Command(connection, transaction, """
            UPDATE agent_staffing_proposals
            SET status = @p0, created_agent_id = @p1, resolved_at_unix_ms = @p2
            WHERE owner_user_id = @p3 AND id = @p4 AND source_room_id = @p5
              AND status = 'Pending'
            """, desired.ToString(), DbValue(createdAgentId), now, ownerUserId, proposalId,
            sourceRoomId);
        if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
            throw Conflict("The staffing proposal changed while it was being resolved.");
        await EnqueueStaffingResolutionNotificationAsync(connection, transaction, proposal,
            desired, now, cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return proposal with
        {
            Status = desired,
            CreatedAgentId = createdAgentId,
            ResolvedAtUnixMs = now,
        };
    }

    private static async Task ValidateStaffingTargetAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        AgentRoom sourceRoom,
        string proposerAgentId,
        AgentStaffingProposalDraft draft,
        CancellationToken cancellationToken)
    {
        if (draft.Kind == AgentStaffingProposalKind.CreateAgent) return;
        if (draft.Kind == AgentStaffingProposalKind.RemoveMember)
        {
            if (sourceRoom.Kind != AgentConversationKind.ProjectTeam ||
                draft.TargetAgentId == proposerAgentId)
                throw new AgentTeamException(AgentTeamError.PermissionDenied,
                    "An Agent cannot remove itself or remove a member from a direct chat.");
            await RequireActiveMemberAsync(connection, transaction, ownerUserId, sourceRoom.Id,
                draft.TargetAgentId!, cancellationToken).ConfigureAwait(false);
            return;
        }

        var targetRoom = await ReadRoomAsync(connection, transaction, ownerUserId,
            draft.TargetRoomId!, cancellationToken).ConfigureAwait(false);
        if (targetRoom is null || targetRoom.Status != AgentRoomStatus.Active ||
            targetRoom.Kind != AgentConversationKind.ProjectTeam)
            throw NotFound("Target Agent team");
        await RequireAgentAsync(connection, transaction, ownerUserId, draft.TargetAgentId!, true,
            cancellationToken).ConfigureAwait(false);
        if (await IsActiveMemberAsync(connection, transaction, ownerUserId, targetRoom.Id,
                draft.TargetAgentId!, cancellationToken).ConfigureAwait(false))
            throw Conflict("The Agent is already an active member of the target team.");
    }

    private static async Task<string?> ApplyApprovedProposalAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentStaffingProposal proposal,
        CancellationToken cancellationToken)
    {
        var draft = proposal.Draft;
        var now = Now();
        switch (draft.Kind)
        {
            case AgentStaffingProposalKind.CreateAgent:
            {
                var proposer = await ReadProfileAsync(connection, transaction,
                    proposal.OwnerUserId, proposal.ProposerAgentId, cancellationToken)
                    .ConfigureAwait(false) ?? throw NotFound("Proposer Agent");
                var profileDraft = new AgentProfileDraft(draft.Name, draft.Rationale,
                    draft.RolePrompt, proposer.Draft.ModelConfigId,
                    draft.ThinkingLevel ?? proposer.Draft.ThinkingLevel, draft.ProfessionKey);
                profileDraft.Validate();
                var agentId = NewId();
                using (var insert = Command(connection, transaction, $"""
                    INSERT INTO agent_profiles ({ProfileColumns})
                    VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9,
                        0, 900, '', 'Active', @p10, @p10, NULL, NULL)
                    """, proposal.OwnerUserId, agentId, profileDraft.Name,
                    profileDraft.Description, profileDraft.RolePrompt, profileDraft.ModelConfigId,
                    DbValue(profileDraft.ThinkingLevel), profileDraft.ProfessionKey, "[]", "[]", now))
                    await insert.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
                var sourceRoom = await ReadRoomAsync(connection, transaction,
                    proposal.OwnerUserId, proposal.SourceRoomId, cancellationToken)
                    .ConfigureAwait(false) ?? throw NotFound("Agent room");
                if (sourceRoom.Kind == AgentConversationKind.ProjectTeam)
                {
                    await InsertMemberAsync(connection, transaction, proposal.OwnerUserId,
                        sourceRoom.Id, agentId,
                        new AgentRoomMemberDraft(draft.Role, draft.Responsibility),
                        AgentMemberStatus.Active, now, cancellationToken).ConfigureAwait(false);
                }
                return agentId;
            }
            case AgentStaffingProposalKind.AddExistingAgent:
            {
                await RequireAgentAsync(connection, transaction, proposal.OwnerUserId,
                    draft.TargetAgentId!, true, cancellationToken).ConfigureAwait(false);
                var room = await ReadRoomAsync(connection, transaction, proposal.OwnerUserId,
                    draft.TargetRoomId!, cancellationToken).ConfigureAwait(false);
                if (room is null || room.Status != AgentRoomStatus.Active ||
                    room.Kind != AgentConversationKind.ProjectTeam ||
                    await IsActiveMemberAsync(connection, transaction, proposal.OwnerUserId,
                        room.Id, draft.TargetAgentId!, cancellationToken).ConfigureAwait(false))
                    throw Conflict("The target Agent or team is no longer eligible.");
                await InsertMemberAsync(connection, transaction, proposal.OwnerUserId, room.Id,
                    draft.TargetAgentId!, new AgentRoomMemberDraft(draft.Role, draft.Responsibility),
                    AgentMemberStatus.Active, now, cancellationToken).ConfigureAwait(false);
                using var updateRoom = Command(connection, transaction, """
                    UPDATE agent_rooms SET default_agent_id = COALESCE(default_agent_id, @p0),
                        updated_at_unix_ms = @p1
                    WHERE owner_user_id = @p2 AND id = @p3 AND status = 'Active'
                    """, draft.TargetAgentId!, now, proposal.OwnerUserId, room.Id);
                await updateRoom.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
                return null;
            }
            case AgentStaffingProposalKind.RemoveMember:
            {
                var room = await ReadRoomAsync(connection, transaction, proposal.OwnerUserId,
                    proposal.SourceRoomId, cancellationToken).ConfigureAwait(false);
                if (room is null || room.Status != AgentRoomStatus.Active ||
                    room.Kind != AgentConversationKind.ProjectTeam ||
                    room.ProjectManagerAgentId == draft.TargetAgentId)
                    throw Conflict("The project manager must be handed over before removal.");
                using (var cancel = Command(connection, transaction, """
                    UPDATE agent_deliveries
                    SET status = CASE WHEN status = 'Pending' THEN 'Cancelled' ELSE 'Failed' END,
                        last_error = @p0, completed_at_unix_ms = @p1
                    WHERE owner_user_id = @p2 AND room_id = @p3 AND target_agent_id = @p4
                      AND status IN ('Pending', 'Running')
                    """, "成员已由 Human 确认移出当前团队。", now, proposal.OwnerUserId,
                    room.Id, draft.TargetAgentId!))
                    await cancel.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
                using (var remove = Command(connection, transaction, """
                    UPDATE agent_room_members SET status = 'Removed'
                    WHERE owner_user_id = @p0 AND room_id = @p1 AND agent_id = @p2
                      AND status = 'Active'
                    """, proposal.OwnerUserId, room.Id, draft.TargetAgentId!))
                {
                    if (await remove.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
                        throw Conflict("The target Agent is no longer an active team member.");
                }
                using var roomUpdate = Command(connection, transaction, """
                    UPDATE agent_rooms SET default_agent_id = CASE
                        WHEN default_agent_id = @p0 THEN (
                            SELECT agent_id FROM agent_room_members
                            WHERE owner_user_id = @p1 AND room_id = @p2 AND status = 'Active'
                            ORDER BY joined_at_unix_ms, agent_id LIMIT 1)
                        ELSE default_agent_id END,
                        updated_at_unix_ms = @p3
                    WHERE owner_user_id = @p1 AND id = @p2
                    """, draft.TargetAgentId!, proposal.OwnerUserId, room.Id, now);
                await roomUpdate.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
                return null;
            }
            default:
                throw AgentTeamValidation.Invalid(nameof(draft.Kind));
        }
    }

    private static async Task RequireRunningDeliveryAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string agentId,
        string deliveryId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT 1 FROM agent_deliveries
            WHERE owner_user_id = @p0 AND id = @p1 AND room_id = @p2
              AND target_agent_id = @p3 AND status = 'Running'
            """, ownerUserId, deliveryId, roomId, agentId);
        if (await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) is null)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "The proposal is not bound to the current Agent run.");
    }

    private static async Task EnqueueStaffingResolutionNotificationAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentStaffingProposal proposal,
        AgentStaffingProposalStatus status,
        long now,
        CancellationToken cancellationToken)
    {
        var messageId = NewId();
        var decision = status == AgentStaffingProposalStatus.Approved ? "已批准" : "已拒绝";
        var content = $"Human {decision}成员变更提案 {proposal.Id}（{proposal.Draft.Kind}）。";
        using (var message = Command(connection, transaction, """
            INSERT INTO agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_agent_id, content,
                reply_to_message_id, root_message_id, hop_count, created_at_unix_ms)
            VALUES (@p0, @p1, @p2, 'System', NULL, @p3, NULL, @p1, 0, @p4)
            """, proposal.OwnerUserId, messageId, proposal.SourceRoomId, content, now))
            await message.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        using (var mention = Command(connection, transaction, """
            INSERT INTO agent_message_mentions(owner_user_id, message_id, agent_id)
            VALUES (@p0, @p1, @p2)
            """, proposal.OwnerUserId, messageId, proposal.ProposerAgentId))
            await mention.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        _ = await InsertDeliveryAsync(connection, transaction, proposal.OwnerUserId,
            proposal.SourceRoomId, messageId, messageId, proposal.ProposerAgentId,
            AgentDeliveryTrigger.StaffingProposal, 0,
            $"staffing-proposal:{proposal.Id}:{status}", now, cancellationToken)
            .ConfigureAwait(false);
        using var touch = Command(connection, transaction, """
            UPDATE agent_rooms SET updated_at_unix_ms = @p0
            WHERE owner_user_id = @p1 AND id = @p2
            """, now, proposal.OwnerUserId, proposal.SourceRoomId);
        await touch.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static async Task<bool> IsActiveMemberAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string agentId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT 1 FROM agent_room_members
            WHERE owner_user_id = @p0 AND room_id = @p1 AND agent_id = @p2
              AND status = 'Active'
            """, ownerUserId, roomId, agentId);
        return await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) is not null;
    }

    private static async Task<AgentRoom?> ReadRoomAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {RoomColumns} FROM agent_rooms WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, roomId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadRoom(reader) : null;
    }

    private static async Task<AgentStaffingProposal?> ReadProposalAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string sourceRoomId,
        string proposalId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {StaffingProposalColumns} FROM agent_staffing_proposals " +
            "WHERE owner_user_id = @p0 AND source_room_id = @p1 AND id = @p2",
            ownerUserId, sourceRoomId, proposalId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
            ? ReadStaffingProposal(reader) : null;
    }

    private static async Task<AgentStaffingProposal?> ReadProposalByRequestAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string sourceRoomId,
        string proposerAgentId,
        string sourceDeliveryId,
        string requestKey,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {StaffingProposalColumns} FROM agent_staffing_proposals " +
            "WHERE owner_user_id = @p0 AND source_room_id = @p1 AND proposer_agent_id = @p2 " +
            "AND source_delivery_id = @p3 AND request_key = @p4",
            ownerUserId, sourceRoomId, proposerAgentId, sourceDeliveryId, requestKey);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
            ? ReadStaffingProposal(reader) : null;
    }

    private static AgentStaffingProposal ReadStaffingProposal(SqliteDataReader reader)
    {
        var draft = System.Text.Json.JsonSerializer.Deserialize<AgentStaffingProposalDraft>(
            reader.GetString(6), JsonOptions) ?? throw new InvalidDataException(
                "Invalid persisted staffing proposal draft.");
        var proposal = new AgentStaffingProposal(reader.GetString(1), reader.GetString(0),
            reader.GetString(2), reader.GetString(3), reader.GetString(4), reader.GetString(5),
            draft, ParseEnum<AgentStaffingProposalStatus>(reader.GetString(7)),
            reader.IsDBNull(8) ? null : reader.GetString(8), reader.GetInt64(9),
            reader.IsDBNull(10) ? null : reader.GetInt64(10));
        proposal.Validate();
        return proposal;
    }
}
