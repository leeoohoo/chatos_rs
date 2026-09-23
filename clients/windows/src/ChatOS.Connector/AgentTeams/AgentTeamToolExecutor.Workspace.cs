using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    private async Task<AgentToolExecutionResult> WorkspaceSnapshotAsync(
        AgentProfile profile,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken)
    {
        var profilesTask = store.ListAgentsAsync(profile.OwnerUserId, false, cancellationToken);
        var roomsTask = store.ListRoomsAsync(profile.OwnerUserId, null, false, cancellationToken);
        await Task.WhenAll(profilesTask, roomsTask).ConfigureAwait(false);
        var profiles = profilesTask.Result.ToDictionary(value => value.Id, StringComparer.Ordinal);
        var teams = new List<object>();
        var teamNamesByAgent = new Dictionary<string, List<string>>(StringComparer.Ordinal);
        foreach (var room in roomsTask.Result.Where(value =>
                     value.Kind == AgentConversationKind.ProjectTeam))
        {
            var members = await store.ListMembersAsync(profile.OwnerUserId, room.Id, false,
                cancellationToken).ConfigureAwait(false);
            foreach (var member in members)
            {
                if (!teamNamesByAgent.TryGetValue(member.AgentId, out var names))
                    teamNamesByAgent[member.AgentId] = names = [];
                names.Add(room.Draft.Name);
            }
            teams.Add(new
            {
                team_ref = references.ConversationReference(room.Id),
                name = room.Draft.Name,
                goal = room.Draft.Goal,
                project_manager_ref = room.ProjectManagerAgentId is null ? null :
                    references.AgentReference(room.ProjectManagerAgentId),
                members = members.Select(member => new
                {
                    agent_ref = references.AgentReference(member.AgentId),
                    name = profiles.GetValueOrDefault(member.AgentId)?.Draft.Name ?? "Agent",
                    profession = profiles.GetValueOrDefault(member.AgentId)?.Draft.ProfessionKey ??
                        "general",
                    member.Draft.Role,
                    member.Draft.Responsibility,
                    is_project_manager = room.ProjectManagerAgentId == member.AgentId,
                }),
            });
        }
        return new AgentToolExecutionResult(Json(new
        {
            agents = profilesTask.Result.Select(value => new
            {
                agent_ref = references.AgentReference(value.Id),
                name = value.Draft.Name,
                profession = value.Draft.ProfessionKey,
                is_current_agent = value.Id == profile.Id,
                teams = teamNamesByAgent.GetValueOrDefault(value.Id) ?? [],
            }),
            teams,
        }));
    }
}
