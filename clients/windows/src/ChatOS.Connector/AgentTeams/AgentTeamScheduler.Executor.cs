using System.Text;
using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamScheduler
{
    private static string? TodoIdForDelivery(AgentDelivery delivery)
    {
        var parts = delivery.DeduplicationKey.Split(':');
        if (parts.Length < 2 || parts[0] != "todo") return null;
        return parts[1];
    }

    private static async Task<IReadOnlyList<AgentMessage>> LoadTodoSourceMessagesAsync(
        IAgentTeamStore teamStore,
        AgentTodo todo,
        CancellationToken cancellationToken)
    {
        var sources = todo.Sources.Take(64).ToArray();
        var output = await teamStore.ListMessagesBySourcesAsync(
            todo.OwnerUserId, sources, cancellationToken).ConfigureAwait(false);
        if (output.Count != sources.Length)
            throw new AgentTeamException(AgentTeamError.NotFound,
                "A Todo source message is no longer available.");
        return output;
    }

    private static List<object> BuildExecutorInput(
        AgentProfile profile,
        AgentRoomMember member,
        AgentRoom room,
        AgentDelivery delivery,
        AgentTodo todo,
        IReadOnlyList<AgentMessage> sourceMessages,
        IReadOnlyList<AgentTodo> todos,
        IReadOnlyList<AgentTodoAssetSnapshot> assetSnapshots,
        IReadOnlyList<AgentTodoProgress> todoProgress,
        string? pluginInstructions,
        AgentRunReferenceVault references,
        IReadOnlyList<AgentMultimodalAttachment> multimodalAttachments)
    {
        var selectedAssets = SelectAssetSnapshots(assetSnapshots);
        var system = $"""
            你是 ChatOS Windows 本机 Agent 团队的隔离 Todo Executor。
            姓名：{profile.Draft.Name}
            职业：{profile.Draft.ProfessionKey}
            团队角色：{member.Draft.Role}
            职责：{member.Draft.Responsibility}

            {profile.Draft.RolePrompt}

            你只能执行下方冻结的 Todo 合同。经理聊天历史不会进入本执行通道。
            规则：
            1. 以 execution_contract、source_messages、dependencies、capability_snapshot 和 team_assets 为唯一工作上下文，不得猜测缺失要求。
            2. 持续用 todo_progress 写入可核验进展；完成时只能用 todo_complete，受阻时只能用 todo_block 并说明原因。
            3. 不得把 Ready 自行改成 InProgress，不得创建或改派其他 Todo。
            4. 只有 capability_snapshot 声明且客户端实际提供的能力才可使用；所有 *_ref 仅在本轮有效。
            5. executor 结束而未完成或明确阻塞任务时，客户端会自动把 Todo 标记为 Blocked。

            {pluginInstructions}
            """;
        var dependencies = todos.Where(value => todo.Draft.Dependencies.Contains(
            value.Id, StringComparer.Ordinal)).Select(value => new
        {
            todo_ref = references.TodoReference(value.Draft.TeamRoomId, value.Id,
                value.Draft.AgentId),
            value.Draft.Title,
            value.Status,
            value.Result,
            value.Revision,
        });
        var context = new StringBuilder()
            .AppendLine($"team: {room.Draft.Name}")
            .AppendLine($"conversation_ref: {references.ConversationReference(room.Id)}")
            .AppendLine($"todo_ref: {references.TodoReference(room.Id, todo.Id, todo.Draft.AgentId)}")
            .AppendLine($"revision: {todo.Revision}")
            .AppendLine("execution_contract:")
            .AppendLine(JsonSerializer.Serialize(new
            {
                objective = todo.Draft.ExecutionContract!.Objective,
                scope = todo.Draft.ExecutionContract.Scope,
                expected_outputs = todo.Draft.ExecutionContract.Outputs,
                acceptance_criteria = todo.Draft.ExecutionContract.Criteria,
                constraints = todo.Draft.ExecutionContract.Limits,
            }))
            .AppendLine("capability_snapshot:")
            .AppendLine(JsonSerializer.Serialize(new
            {
                requires_execution = todo.Draft.ExecutionPlan!.RequiresExecution,
                builtin_capabilities = todo.Draft.ExecutionPlan.Capabilities.Select(
                    value => value.ToString()),
                plugins = todo.Draft.ExecutionPlan.SelectedPlugins.Select(value => new
                    { display_name = value.DisplayName, value.Reason }),
                selection_revision = todo.Draft.ExecutionPlan.SelectionRevision,
            }))
            .AppendLine("dependencies:")
            .AppendLine(JsonSerializer.Serialize(dependencies))
            .AppendLine("source_messages:");
        foreach (var message in sourceMessages)
        {
            context.AppendLine(JsonSerializer.Serialize(new
            {
                conversation_ref = references.ConversationReference(message.RoomId),
                message_ref = references.MessageReference(message.RoomId, message.Id),
                sender = message.SenderKind.ToString(),
                sender_agent_ref = message.SenderAgentId is null ? null :
                    references.AgentReference(message.SenderAgentId),
                message.Content,
                attachments = message.Attachments.Select(attachment => new
                {
                    attachment_ref = references.AttachmentReference(
                        message.RoomId, message.Id, attachment.Id),
                    name = AgentTeamMultimodalInput.SafeFileName(
                        attachment.Name, attachment.MimeType),
                    attachment.MimeType,
                    attachment.ByteCount,
                }),
            }));
        }
        context.AppendLine("todo_progress:")
            .AppendLine(JsonSerializer.Serialize(todoProgress.Select(value => new
            {
                value.Sequence,
                value.Kind,
                value.Stage,
                value.Detail,
                value.AssetUpdateSuggestions,
                value.CreatedAtUnixMs,
            })))
            .AppendLine($"team_assets_truncated: {selectedAssets.Count != assetSnapshots.Count}")
            .AppendLine("team_assets:")
            .AppendLine(JsonSerializer.Serialize(selectedAssets.Select(value => new
            {
                asset_ref = references.AssetReference(
                    value.TeamRoomId, value.AssetId, value.Revision),
                value.Category,
                value.Title,
                value.Markdown,
                value.Revision,
                value.CapturedAtUnixMs,
            })));
        return
        [
            new Dictionary<string, object> { ["role"] = "system", ["content"] = system },
            AgentTeamMultimodalInput.UserMessage(context.ToString(), multimodalAttachments),
        ];
    }

    private static IReadOnlyList<AgentTodoAssetSnapshot> SelectAssetSnapshots(
        IReadOnlyList<AgentTodoAssetSnapshot> assets)
    {
        const int maximumMarkdownCharacters = 512_000;
        var output = new List<AgentTodoAssetSnapshot>();
        var characters = 0;
        foreach (var asset in assets.Take(64))
        {
            if (characters + asset.Markdown.Length > maximumMarkdownCharacters) continue;
            output.Add(asset);
            characters += asset.Markdown.Length;
        }
        return output;
    }
}
