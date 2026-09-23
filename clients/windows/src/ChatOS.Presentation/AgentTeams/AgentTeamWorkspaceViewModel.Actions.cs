using ChatOS.Core.Domain;

namespace ChatOS.Presentation.AgentTeams;

public sealed partial class AgentTeamWorkspaceViewModel
{
    public Task SaveAgentAsync(string? agentId, AgentProfileDraft draft) =>
        MutateAsync("正在保存 Agent…", async context =>
        {
            await _service.SaveAgentAsync(context.Owner, agentId, draft, context.Token)
                .ConfigureAwait(false);
        });

    public Task ArchiveAgentAsync(AgentProfile profile) =>
        MutateAsync("正在归档 Agent…", async context =>
        {
            await _service.ArchiveAgentAsync(context.Owner, profile.Id, context.Token)
                .ConfigureAwait(false);
        });

    public Task CreateTeamAsync(string name, string goal, string managerAgentId) =>
        MutateAsync("正在创建团队…", async context =>
        {
            var room = await _service.CreateTeamAsync(context.Owner, context.Project,
                new AgentRoomDraft(name.Trim(), goal.Trim()), managerAgentId, context.Token)
                .ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => SelectedRoom = room, context.Token)
                .ConfigureAwait(false);
        });

    public Task OpenDirectAsync(string agentId) =>
        MutateAsync("正在打开私聊…", async context =>
        {
            var room = await _service.OpenDirectAsync(context.Owner, agentId, context.Token)
                .ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => SelectedRoom = room, context.Token)
                .ConfigureAwait(false);
        });

    public Task AddMemberAsync(string agentId, string role, string responsibility) =>
        MutateRoomAsync("正在添加成员…", async (context, room) =>
        {
            await _service.AddMemberAsync(context.Owner, room.Id, agentId,
                new AgentRoomMemberDraft(role.Trim(), responsibility.Trim()), context.Token)
                .ConfigureAwait(false);
        });

    public Task RemoveMemberAsync(AgentRoomMember member) =>
        MutateRoomAsync("正在移除成员…", async (context, room) =>
        {
            await _service.RemoveMemberAsync(context.Owner, room.Id, member.AgentId, context.Token)
                .ConfigureAwait(false);
        });

    public Task ConfigureTeamAsync(
        string name,
        string goal,
        string? defaultAgentId,
        string managerAgentId) =>
        MutateRoomAsync("正在更新团队…", async (context, room) =>
        {
            await _service.ConfigureTeamAsync(context.Owner, room.Id,
                new AgentRoomDraft(name.Trim(), goal.Trim()), defaultAgentId,
                managerAgentId, context.Token).ConfigureAwait(false);
        });

    public Task SendMessageAsync(IReadOnlyList<string>? mentionedAgentIds = null) =>
        MutateRoomAsync("正在发送…", async (context, room) =>
        {
            var content = MessageText.Trim();
            await _service.PostHumanMessageAsync(context.Owner, room.Id, content,
                mentionedAgentIds, PendingAttachments.ToArray(), context.Token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                MessageText = string.Empty;
                PendingAttachments.Clear();
                OnPropertyChanged(nameof(HasPendingAttachments));
            }, context.Token)
                .ConfigureAwait(false);
        });

    public Task CreateTodoAsync(
        string agentId,
        string title,
        string detail,
        AgentTodoPriority priority,
        IReadOnlyList<string>? dependencyIds = null) =>
        MutateRoomAsync("正在创建 Todo…", async (context, room) =>
        {
            var manager = room.ProjectManagerAgentId ?? throw new InvalidOperationException(
                "当前团队没有项目经理。");
            await _service.CreateTodoAsync(context.Owner, manager,
                new AgentTodoDraft(room.Id, agentId, title.Trim(), detail.Trim(), priority,
                    dependencyIds), context.Token).ConfigureAwait(false);
        });

    public Task UpdateTodoAsync(
        AgentTodo todo,
        AgentTodoStatus status,
        string result,
        string? assignedAgentId = null) =>
        MutateRoomAsync("正在更新 Todo…", async (context, room) =>
        {
            var actor = room.ProjectManagerAgentId ?? todo.Draft.AgentId;
            await _service.UpdateTodoAsync(context.Owner, actor, todo.Id, todo.Revision,
                status, result.Trim(), assignedAgentId, context.Token).ConfigureAwait(false);
        });

    public Task MoveTodoAsync(AgentTodo todo, int offset) =>
        MutateRoomAsync("正在调整 Todo 顺序…", async (context, room) =>
        {
            var manager = room.ProjectManagerAgentId ?? throw new InvalidOperationException(
                "当前团队没有项目经理。");
            var ordered = Todos.OrderBy(value => value.SortOrder).Select(value => value.Id).ToList();
            var current = ordered.IndexOf(todo.Id);
            var destination = Math.Clamp(current + offset, 0, ordered.Count - 1);
            if (current < 0 || current == destination) return;
            ordered.RemoveAt(current);
            ordered.Insert(destination, todo.Id);
            await _service.ReorderTodosAsync(context.Owner, manager, room.Id, ordered, context.Token)
                .ConfigureAwait(false);
        });

    public Task SaveAssetAsync(
        AgentTeamAsset? asset,
        AgentTeamAssetCategory category,
        string title,
        string markdown) =>
        MutateRoomAsync("正在保存团队资产…", async (context, room) =>
        {
            await _service.SaveAssetAsync(context.Owner, room.Id, asset?.Id, null, category,
                title.Trim(), markdown, asset?.Revision, context.Token).ConfigureAwait(false);
        });

    public Task ArchiveAssetAsync(AgentTeamAsset asset) =>
        MutateRoomAsync("正在归档团队资产…", async (context, room) =>
        {
            await _service.ArchiveAssetAsync(context.Owner, room.Id, asset.Id, null,
                asset.Revision, context.Token).ConfigureAwait(false);
        });

    public Task SubmitRequirementSurveyAsync(
        AgentRequirementSurvey survey,
        AgentRequirementSubmission submission) =>
        MutateRoomAsync("正在提交需求调研…", async (context, room) =>
        {
            await _service.SubmitRequirementSurveyAsync(context.Owner, room.Id,
                survey.Id, submission, context.Token).ConfigureAwait(false);
        });

    public Task ResolveStaffingProposalAsync(AgentStaffingProposal proposal, bool approve) =>
        MutateRoomAsync(approve ? "正在批准成员提案…" : "正在拒绝成员提案…",
            async (context, room) =>
            {
                await _service.ResolveStaffingProposalAsync(context.Owner, room.Id,
                    proposal.Id, approve, context.Token).ConfigureAwait(false);
            });

    private Task MutateRoomAsync(
        string status,
        Func<SessionContext, AgentRoom, Task> action) =>
        MutateAsync(status, context => action(context,
            SelectedRoom ?? throw new InvalidOperationException("请先选择一个团队或私聊。")));

    private async Task MutateAsync(string status, Func<SessionContext, Task> action)
    {
        using var context = RequireContext(CancellationToken.None);
        await ExecuteAsync(status, async () =>
        {
            Interlocked.Increment(ref _localMutationCount);
            try
            {
                await action(context).ConfigureAwait(false);
            }
            finally
            {
                Interlocked.Decrement(ref _localMutationCount);
            }
            await RefreshAsync(context.Token).ConfigureAwait(false);
        }).ConfigureAwait(false);
    }
}
