using System.Text.Json;
using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private const string SurveyColumns = """
        owner_user_id, id, team_room_id, creator_agent_id, source_delivery_id,
        request_key, draft_json, status, submission_json, resolution_json,
        created_at_unix_ms, submitted_at_unix_ms, resolved_at_unix_ms
        """;

    public async Task<AgentRequirementSurvey> CreateRequirementSurveyAsync(
        string ownerUserId,
        string teamRoomId,
        string creatorAgentId,
        string sourceDeliveryId,
        string requestKey,
        AgentRequirementSurveyDraft draft,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(requestKey, nameof(requestKey));
        draft.Validate();
        var draftJson = Serialize(draft);
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await EnsureSurveyManagerAsync(connection, transaction, ownerUserId, teamRoomId,
            creatorAgentId, cancellationToken).ConfigureAwait(false);
        var delivery = await ReadDeliveryAsync(connection, transaction, ownerUserId,
            sourceDeliveryId, cancellationToken).ConfigureAwait(false);
        if (delivery is null || delivery.TargetAgentId != creatorAgentId ||
            delivery.RoomId != teamRoomId || delivery.Status != AgentDeliveryStatus.Running)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "A requirement survey can only be created by the authorized active Agent run.");

        using (var existingCommand = Command(connection, transaction, $"""
            SELECT {SurveyColumns} FROM agent_requirement_surveys
            WHERE owner_user_id = @p0 AND team_room_id = @p1 AND creator_agent_id = @p2
              AND source_delivery_id = @p3 AND request_key = @p4
            """, ownerUserId, teamRoomId, creatorAgentId, sourceDeliveryId, requestKey))
        await using (var reader = await existingCommand.ExecuteReaderAsync(cancellationToken)
            .ConfigureAwait(false))
        {
            if (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                var existing = ReadSurvey(reader);
                if (Serialize(existing.Draft) != draftJson)
                    throw Conflict("Requirement survey request key was reused with different content.");
                await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
                return existing;
            }
        }

        var now = Now();
        var survey = new AgentRequirementSurvey(NewId(), ownerUserId, teamRoomId,
            creatorAgentId, sourceDeliveryId, requestKey, draft,
            AgentRequirementSurveyStatus.Pending, null, null, now, null, null);
        survey.Validate();
        using var insert = Command(connection, transaction, $"""
            INSERT INTO agent_requirement_surveys ({SurveyColumns})
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, 'Pending', NULL, NULL,
                @p7, NULL, NULL)
            """, ownerUserId, survey.Id, teamRoomId, creatorAgentId,
            sourceDeliveryId, requestKey, draftJson, now);
        await insert.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return survey;
    }

    public async Task<IReadOnlyList<AgentRequirementSurvey>> ListRequirementSurveysAsync(
        string ownerUserId,
        string teamRoomId,
        AgentRequirementSurveyStatus? status = null,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        await RequireRoomAsync(connection, null, ownerUserId, teamRoomId, requireActive: false,
            cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {SurveyColumns} FROM agent_requirement_surveys " +
            "WHERE owner_user_id = @p0 AND team_room_id = @p1" +
            (status is null ? string.Empty : " AND status = @p2") +
            " ORDER BY created_at_unix_ms DESC, id",
            status is null
                ? [ownerUserId, teamRoomId]
                : [ownerUserId, teamRoomId, status.Value.ToString()]);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentRequirementSurvey>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            output.Add(ReadSurvey(reader));
        return output;
    }

    public async Task<AgentRequirementSurvey?> GetRequirementSurveyAsync(
        string ownerUserId,
        string teamRoomId,
        string surveyId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        return await ReadSurveyAsync(connection, null, ownerUserId, teamRoomId, surveyId,
            cancellationToken).ConfigureAwait(false);
    }

    public async Task<AgentRequirementSurvey> SubmitRequirementSurveyAsync(
        string ownerUserId,
        string teamRoomId,
        string surveyId,
        AgentRequirementSubmission submission,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var existing = await ReadSurveyAsync(connection, transaction, ownerUserId, teamRoomId,
            surveyId, cancellationToken).ConfigureAwait(false) ?? throw NotFound("Requirement survey");
        AgentRequirementSurvey.ValidateSubmission(submission, existing.Draft.Questions);
        var submissionJson = Serialize(submission);
        if (existing.Status == AgentRequirementSurveyStatus.Submitted)
        {
            if (Serialize(existing.Submission) != submissionJson)
                throw Conflict("Requirement survey was already submitted with different answers.");
            await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
            return existing;
        }

        var now = Now();
        using (var update = Command(connection, transaction, """
            UPDATE agent_requirement_surveys SET status = 'Submitted', submission_json = @p0,
                submitted_at_unix_ms = @p1
            WHERE owner_user_id = @p2 AND team_room_id = @p3 AND id = @p4 AND status = 'Pending'
            """, submissionJson, now, ownerUserId, teamRoomId, surveyId))
        {
            if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
                throw Conflict("Requirement survey changed before submission.");
        }

        var recipient = await ResolveSurveyRecipientAsync(connection, transaction, existing,
            cancellationToken).ConfigureAwait(false);
        var messageId = await InsertSystemMessageAsync(connection, transaction, ownerUserId,
            teamRoomId, $"Human 已提交需求调研“{existing.Draft.Title}”（survey_id: {surveyId}）。请读取真实答案并形成方案。",
            now, cancellationToken).ConfigureAwait(false);
        _ = await InsertDeliveryAsync(connection, transaction, ownerUserId, teamRoomId,
            messageId, messageId, recipient, AgentDeliveryTrigger.RequirementSurvey, 0,
            $"requirement-survey:{surveyId}:submitted", now, cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return existing with
        {
            Status = AgentRequirementSurveyStatus.Submitted,
            Submission = submission,
            SubmittedAtUnixMs = now,
        };
    }

    public async Task<AgentRequirementSurvey> ResolveRequirementSurveyAsync(
        string ownerUserId,
        string teamRoomId,
        string surveyId,
        string resolverAgentId,
        AgentRequirementResolution resolution,
        CancellationToken cancellationToken = default)
    {
        resolution.Validate();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await EnsureSurveyManagerAsync(connection, transaction, ownerUserId, teamRoomId,
            resolverAgentId, cancellationToken).ConfigureAwait(false);
        var existing = await ReadSurveyAsync(connection, transaction, ownerUserId, teamRoomId,
            surveyId, cancellationToken).ConfigureAwait(false) ?? throw NotFound("Requirement survey");
        if (existing.Status != AgentRequirementSurveyStatus.Submitted)
            throw Conflict("Requirement survey must be submitted before it can be resolved.");
        if (existing.Resolution is not null)
        {
            if (Serialize(existing.Resolution) != Serialize(resolution))
                throw Conflict("Requirement survey already has a different resolution.");
            await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
            return existing;
        }

        var now = Now();
        using var update = Command(connection, transaction, """
            UPDATE agent_requirement_surveys SET resolution_json = @p0, resolved_at_unix_ms = @p1
            WHERE owner_user_id = @p2 AND team_room_id = @p3 AND id = @p4
              AND status = 'Submitted' AND resolution_json IS NULL
            """, Serialize(resolution), now, ownerUserId, teamRoomId, surveyId);
        if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
            throw Conflict("Requirement survey changed before resolution.");
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return existing with { Resolution = resolution, ResolvedAtUnixMs = now };
    }

    private static AgentRequirementSurvey ReadSurvey(SqliteDataReader reader)
    {
        var survey = new AgentRequirementSurvey(reader.GetString(1), reader.GetString(0),
            reader.GetString(2), reader.GetString(3), reader.GetString(4), reader.GetString(5),
            JsonSerializer.Deserialize<AgentRequirementSurveyDraft>(reader.GetString(6), JsonOptions)!,
            ParseEnum<AgentRequirementSurveyStatus>(reader.GetString(7)),
            reader.IsDBNull(8) ? null : JsonSerializer.Deserialize<AgentRequirementSubmission>(reader.GetString(8), JsonOptions),
            reader.IsDBNull(9) ? null : JsonSerializer.Deserialize<AgentRequirementResolution>(reader.GetString(9), JsonOptions),
            reader.GetInt64(10), reader.IsDBNull(11) ? null : reader.GetInt64(11),
            reader.IsDBNull(12) ? null : reader.GetInt64(12));
        survey.Validate();
        return survey;
    }

    private static async Task<AgentRequirementSurvey?> ReadSurveyAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string teamRoomId,
        string surveyId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, $"""
            SELECT {SurveyColumns} FROM agent_requirement_surveys
            WHERE owner_user_id = @p0 AND team_room_id = @p1 AND id = @p2
            """, ownerUserId, teamRoomId, surveyId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadSurvey(reader) : null;
    }

    private static async Task EnsureSurveyManagerAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string agentId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT r.conversation_kind, r.project_manager_agent_id,
                p.profession_key, p.default_skill_ids_json
            FROM agent_rooms r
            JOIN agent_room_members m ON m.owner_user_id = r.owner_user_id
                AND m.room_id = r.id AND m.agent_id = @p2 AND m.status = 'Active'
            JOIN agent_profiles p ON p.owner_user_id = r.owner_user_id
                AND p.id = @p2 AND p.status = 'Active'
            WHERE r.owner_user_id = @p0 AND r.id = @p1 AND r.status = 'Active'
            """, ownerUserId, roomId, agentId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        if (!await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ||
            !reader.GetString(0).Equals(nameof(AgentConversationKind.ProjectTeam), StringComparison.Ordinal))
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Requirement surveys are available only to project team members.");
        var isManager = !reader.IsDBNull(1) && reader.GetString(1) == agentId;
        var profession = reader.GetString(2);
        var skills = DeserializeStrings(reader.GetString(3));
        if (!isManager && profession != "project_manager" &&
            !skills.Contains("requirement.survey.manage", StringComparer.Ordinal))
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "The Agent cannot manage requirement surveys.");
    }

    private static async Task<string> ResolveSurveyRecipientAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentRequirementSurvey survey,
        CancellationToken cancellationToken)
    {
        try
        {
            await EnsureSurveyManagerAsync(connection, transaction, survey.OwnerUserId,
                survey.TeamRoomId, survey.CreatorAgentId, cancellationToken).ConfigureAwait(false);
            return survey.CreatorAgentId;
        }
        catch (AgentTeamException)
        {
            using var command = Command(connection, transaction, """
                SELECT project_manager_agent_id FROM agent_rooms
                WHERE owner_user_id = @p0 AND id = @p1 AND status = 'Active'
                """, survey.OwnerUserId, survey.TeamRoomId);
            return await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) as string
                ?? throw Conflict("The team has no active project manager for survey resolution.");
        }
    }
}
