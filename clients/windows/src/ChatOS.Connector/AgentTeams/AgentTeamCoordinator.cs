using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed class AgentTeamCoordinator : IAgentTeamService
{
    private readonly IAgentTeamStore _store;
    private readonly IProjectRegistry _projects;
    private readonly AgentTeamScheduler _scheduler;

    public AgentTeamCoordinator(
        IAgentTeamStore store,
        IProjectRegistry projects,
        AgentTeamScheduler scheduler)
    {
        _store = store;
        _projects = projects;
        _scheduler = scheduler;
        _scheduler.Changed += (_, args) => Changed?.Invoke(this, args);
    }

    public event EventHandler<AgentTeamChangedEventArgs>? Changed;

    public Task<IReadOnlyList<AgentProfile>> ListAgentsAsync(
        string ownerUserId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default) =>
        _store.ListAgentsAsync(ownerUserId, includeArchived, cancellationToken);

    public async Task<AgentProfile> SaveAgentAsync(
        string ownerUserId,
        string? agentId,
        AgentProfileDraft draft,
        CancellationToken cancellationToken = default)
    {
        var profile = agentId is null
            ? await _store.CreateAgentAsync(ownerUserId, draft, cancellationToken).ConfigureAwait(false)
            : await _store.UpdateAgentAsync(ownerUserId, agentId, draft,
                AgentProfileStatus.Active, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, null, null, "agent_saved");
        return profile;
    }

    public async Task ArchiveAgentAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default)
    {
        var profile = await _store.GetAgentAsync(ownerUserId, agentId, cancellationToken)
            .ConfigureAwait(false) ?? throw new AgentTeamException(
                AgentTeamError.NotFound, "Agent was not found.");
        var rooms = await _store.ListRoomsAsync(
            ownerUserId, null, includeArchived: false, cancellationToken).ConfigureAwait(false);
        foreach (var room in rooms)
        {
            var member = (await _store.ListMembersAsync(ownerUserId, room.Id,
                    includeRemoved: false, cancellationToken).ConfigureAwait(false))
                .FirstOrDefault(value => string.Equals(value.AgentId, agentId, StringComparison.Ordinal));
            if (member is not null)
            {
                await _store.UpsertMemberAsync(ownerUserId, room.Id, agentId,
                    member.Draft, AgentMemberStatus.Removed, cancellationToken).ConfigureAwait(false);
            }
        }

        await _store.UpdateAgentAsync(ownerUserId, agentId, profile.Draft,
            AgentProfileStatus.Archived, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, null, null, "agent_archived");
    }

    public Task<IReadOnlyList<AgentRoom>> ListRoomsAsync(
        string ownerUserId,
        string projectId,
        CancellationToken cancellationToken = default) =>
        _store.ListRoomsAsync(ownerUserId, projectId, includeArchived: false, cancellationToken);

    public async Task<AgentRoom> CreateTeamAsync(
        string ownerUserId,
        string projectId,
        AgentRoomDraft draft,
        string projectManagerAgentId,
        CancellationToken cancellationToken = default)
    {
        var project = await _projects.GetAsync(ownerUserId, projectId, cancellationToken)
            .ConfigureAwait(false);
        if (project is null || project.Status != LocalProjectStatus.Active)
        {
            throw new AgentTeamException(AgentTeamError.NotFound,
                "The local project is unavailable for an Agent team.");
        }

        var room = await _store.CreateRoomAsync(ownerUserId, projectId, draft,
            projectManagerAgentId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, projectId, room.Id, "team_created");
        return room;
    }

    public async Task<AgentRoom> OpenDirectAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default)
    {
        var room = await _store.OpenHumanAgentDirectAsync(
            ownerUserId, agentId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, room.Id, "direct_opened");
        return room;
    }

    public async Task<AgentRoomMember> AddMemberAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        AgentRoomMemberDraft draft,
        CancellationToken cancellationToken = default)
    {
        var member = await _store.UpsertMemberAsync(ownerUserId, roomId, agentId, draft,
            AgentMemberStatus.Active, cancellationToken).ConfigureAwait(false);
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId, "member_saved");
        return member;
    }

    public async Task RemoveMemberAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        CancellationToken cancellationToken = default)
    {
        var member = (await _store.ListMembersAsync(ownerUserId, roomId,
                includeRemoved: true, cancellationToken).ConfigureAwait(false))
            .FirstOrDefault(value => string.Equals(value.AgentId, agentId, StringComparison.Ordinal))
            ?? throw new AgentTeamException(AgentTeamError.NotFound,
                "Team member was not found.");
        await _store.UpsertMemberAsync(ownerUserId, roomId, agentId, member.Draft,
            AgentMemberStatus.Removed, cancellationToken).ConfigureAwait(false);
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId, "member_removed");
    }

    public async Task<AgentRoom> ConfigureTeamAsync(
        string ownerUserId,
        string roomId,
        AgentRoomDraft draft,
        string? defaultAgentId,
        string projectManagerAgentId,
        CancellationToken cancellationToken = default)
    {
        var room = await _store.UpdateRoomAsync(ownerUserId, roomId, draft,
            defaultAgentId, projectManagerAgentId, AgentRoomStatus.Active, cancellationToken)
            .ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId, "team_configured");
        return room;
    }

    public async Task<AgentTeamSnapshot> LoadSnapshotAsync(
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken = default)
    {
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        var membersTask = _store.ListMembersAsync(ownerUserId, roomId,
            includeRemoved: false, cancellationToken);
        var profilesTask = _store.ListAgentsAsync(ownerUserId,
            includeArchived: false, cancellationToken);
        var messagesTask = _store.ListMessagesAsync(ownerUserId, roomId, 250,
            includeAttachmentPayloads: false, cancellationToken);
        var todosTask = _store.ListTodosAsync(ownerUserId, roomId,
            includeTerminal: true, cancellationToken);
        var assetsTask = _store.ListAssetsAsync(ownerUserId, roomId,
            includeArchived: false, cancellationToken);
        var surveysTask = room.Kind == AgentConversationKind.ProjectTeam
            ? _store.ListRequirementSurveysAsync(ownerUserId, room.ProjectId, null,
                cancellationToken: cancellationToken)
            : Task.FromResult<IReadOnlyList<AgentRequirementSurvey>>([]);
        var staffingTask = _store.ListStaffingProposalsAsync(ownerUserId, roomId, null,
            cancellationToken);
        var runsTask = _store.ListRunsAsync(ownerUserId, roomId, 100, cancellationToken);
        await Task.WhenAll(membersTask, profilesTask, messagesTask, todosTask, assetsTask,
                surveysTask, staffingTask, runsTask)
            .ConfigureAwait(false);
        var memberIds = membersTask.Result.Select(value => value.AgentId).ToHashSet(StringComparer.Ordinal);
        return new AgentTeamSnapshot(room, membersTask.Result,
            profilesTask.Result.Where(value => memberIds.Contains(value.Id)).ToArray(),
            messagesTask.Result, todosTask.Result, assetsTask.Result, surveysTask.Result,
            staffingTask.Result, runsTask.Result);
    }

    public async Task<AgentPostResult> PostHumanMessageAsync(
        string ownerUserId,
        string roomId,
        string content,
        IReadOnlyList<string>? mentionedAgentIds = null,
        IReadOnlyList<AgentMessageAttachment>? attachments = null,
        CancellationToken cancellationToken = default)
    {
        var result = await _store.PostMessageAsync(ownerUserId, roomId,
            new AgentMessageDraft(AgentMessageSenderKind.Human, null, content,
                mentionedAgentIds, attachments), cancellationToken).ConfigureAwait(false);
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId, "message_posted");
        QueueDrain(ownerUserId);
        return result;
    }

    public Task<AgentMessageAttachment?> GetMessageAttachmentAsync(
        string ownerUserId,
        string roomId,
        string attachmentId,
        CancellationToken cancellationToken = default) =>
        _store.GetMessageAttachmentAsync(ownerUserId, roomId, attachmentId, cancellationToken);

    public async Task<AgentTodo> CreateTodoAsync(
        string ownerUserId,
        string managerAgentId,
        AgentTodoDraft draft,
        CancellationToken cancellationToken = default)
    {
        var room = await RequireRoomAsync(
            ownerUserId, draft.TeamRoomId, cancellationToken).ConfigureAwait(false);
        RequireManager(room, managerAgentId);
        var todo = await _store.CreateTodoAsync(ownerUserId, draft, cancellationToken)
            .ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, room.Id, "todo_created");
        QueueDrain(ownerUserId);
        return todo;
    }

    public async Task<AgentTodo> UpdateTodoAsync(
        string ownerUserId,
        string actingAgentId,
        string todoId,
        long expectedRevision,
        AgentTodoStatus status,
        string result,
        string? assignedAgentId = null,
        CancellationToken cancellationToken = default)
    {
        var current = await _store.GetTodoAsync(ownerUserId, todoId, cancellationToken)
            .ConfigureAwait(false) ?? throw new AgentTeamException(
                AgentTeamError.NotFound, "Todo was not found.");
        var room = await RequireRoomAsync(ownerUserId, current.Draft.TeamRoomId, cancellationToken)
            .ConfigureAwait(false);
        var isManager = string.Equals(room.ProjectManagerAgentId, actingAgentId, StringComparison.Ordinal);
        if (!isManager && !string.Equals(current.Draft.AgentId, actingAgentId, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Only the project manager or assigned Agent can update this Todo.");
        }

        if (!isManager && assignedAgentId is not null &&
            !string.Equals(assignedAgentId, actingAgentId, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Only the project manager can reassign a Todo.");
        }

        var todo = await _store.UpdateTodoAsync(ownerUserId, todoId, expectedRevision,
            status, result, assignedAgentId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, room.Id, "todo_updated");
        QueueDrain(ownerUserId);
        return todo;
    }

    public async Task<IReadOnlyList<AgentTodo>> ReorderTodosAsync(
        string ownerUserId,
        string managerAgentId,
        string roomId,
        IReadOnlyList<string> todoIds,
        CancellationToken cancellationToken = default)
    {
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        RequireManager(room, managerAgentId);
        var todos = await _store.ReorderTodosAsync(ownerUserId, roomId, todoIds, cancellationToken)
            .ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, room.Id, "todos_reordered");
        return todos;
    }

    public Task<IReadOnlyList<AgentTodoProgress>> ListTodoProgressAsync(
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken = default) =>
        _store.ListTodoProgressAsync(ownerUserId, todoId, 200, cancellationToken);

    public async Task<AgentTeamAsset> SaveAssetAsync(
        string ownerUserId,
        string roomId,
        string? assetId,
        string? editorAgentId,
        AgentTeamAssetCategory category,
        string title,
        string markdown,
        int? expectedRevision,
        CancellationToken cancellationToken = default)
    {
        var asset = await _store.UpsertAssetAsync(ownerUserId, roomId, assetId,
            editorAgentId, category, title, markdown, expectedRevision, cancellationToken)
            .ConfigureAwait(false);
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId, "asset_saved");
        return asset;
    }

    public async Task ArchiveAssetAsync(
        string ownerUserId,
        string roomId,
        string assetId,
        string? editorAgentId,
        int expectedRevision,
        CancellationToken cancellationToken = default)
    {
        await _store.ArchiveAssetAsync(ownerUserId, roomId, assetId, editorAgentId,
            expectedRevision, cancellationToken).ConfigureAwait(false);
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId, "asset_archived");
    }

    public Task<IReadOnlyList<AgentRequirementSurvey>> ListProjectRequirementSurveysAsync(
        string ownerUserId,
        string projectId,
        CancellationToken cancellationToken = default) =>
        _store.ListRequirementSurveysAsync(ownerUserId, projectId, null,
            cancellationToken: cancellationToken);

    public async Task<AgentRequirementSurvey> SubmitProjectRequirementSurveyAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        AgentRequirementSubmission submission,
        CancellationToken cancellationToken = default)
    {
        var survey = await _store.SubmitRequirementSurveyAsync(ownerUserId, projectId,
            surveyId, submission, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, projectId, null, "requirement_survey_submitted");
        QueueDrain(ownerUserId);
        return survey;
    }

    public async Task<AgentRequirementSurvey> SubmitRequirementSurveyAsync(
        string ownerUserId,
        string roomId,
        string surveyId,
        AgentRequirementSubmission submission,
        CancellationToken cancellationToken = default)
    {
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false);
        var survey = await _store.SubmitRequirementSurveyAsync(ownerUserId, room.ProjectId,
            surveyId, submission, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId, "requirement_survey_submitted");
        QueueDrain(ownerUserId);
        return survey;
    }

    public async Task<AgentStaffingProposal> ResolveStaffingProposalAsync(
        string ownerUserId,
        string roomId,
        string proposalId,
        bool approve,
        CancellationToken cancellationToken = default)
    {
        var room = await RequireRoomAsync(ownerUserId, roomId, cancellationToken)
            .ConfigureAwait(false);
        var proposal = await _store.ResolveStaffingProposalAsync(ownerUserId, roomId,
            proposalId, approve, cancellationToken).ConfigureAwait(false);
        Raise(ownerUserId, room.ProjectId, roomId,
            approve ? "staffing_proposal_approved" : "staffing_proposal_rejected");
        QueueDrain(ownerUserId);
        return proposal;
    }

    public Task DrainAsync(string ownerUserId, CancellationToken cancellationToken = default) =>
        _scheduler.DrainAsync(ownerUserId, cancellationToken);

    private async Task<AgentRoom> RequireRoomAsync(
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken) =>
        await _store.GetRoomAsync(ownerUserId, roomId, cancellationToken).ConfigureAwait(false)
        ?? throw new AgentTeamException(AgentTeamError.NotFound, "Agent room was not found.");

    private static void RequireManager(AgentRoom room, string agentId)
    {
        if (!string.Equals(room.ProjectManagerAgentId, agentId, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Only the team's explicit project manager can perform this action.");
        }
    }

    private void QueueDrain(string ownerUserId) => _ = Task.Run(async () =>
    {
        try
        {
            await _scheduler.DrainAsync(ownerUserId, CancellationToken.None).ConfigureAwait(false);
        }
        catch
        {
            // Individual delivery failures are durable and visible through Agent runs.
        }
    });

    private void Raise(string ownerUserId, string? projectId, string? roomId, string kind) =>
        Changed?.Invoke(this, new AgentTeamChangedEventArgs(ownerUserId, projectId, roomId, kind));
}
