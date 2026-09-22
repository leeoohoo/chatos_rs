using System.Collections.Concurrent;
using System.Text;
using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed class AgentTeamScheduler(
    IAgentTeamStore store,
    AgentTeamModelGateway models,
    AgentTeamToolExecutor tools,
    AgentPluginToolRuntime? pluginTools = null)
{
    private readonly ConcurrentDictionary<string, SemaphoreSlim> _ownerGates =
        new(StringComparer.Ordinal);

    public event EventHandler<AgentTeamChangedEventArgs>? Changed;

    public async Task DrainAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Identifier(ownerUserId, nameof(ownerUserId));
        var gate = _ownerGates.GetOrAdd(ownerUserId, static _ => new SemaphoreSlim(1, 1));
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            for (var handled = 0; handled < 100; handled++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var delivery = await store.ClaimNextDeliveryAsync(ownerUserId, cancellationToken)
                    .ConfigureAwait(false);
                if (delivery is null)
                {
                    break;
                }

                await RunDeliverySafelyAsync(delivery, cancellationToken).ConfigureAwait(false);
            }
        }
        finally
        {
            gate.Release();
        }
    }

    private async Task RunDeliverySafelyAsync(
        AgentDelivery delivery,
        CancellationToken cancellationToken)
    {
        var started = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var run = new AgentRunSummary(Guid.NewGuid().ToString("D").ToLowerInvariant(),
            delivery.OwnerUserId, delivery.Id, delivery.TargetAgentId, delivery.RoomId,
            AgentRunStatus.Running, 0, null, started, started);
        await store.SaveRunAsync(run, cancellationToken).ConfigureAwait(false);
        RaiseChanged(delivery, "run_started");
        try
        {
            await RunDeliveryAsync(delivery, run, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            await store.SaveRunAsync(run with
            {
                Status = AgentRunStatus.Cancelled,
                LastError = "Agent run was cancelled.",
                UpdatedAtUnixMs = now,
            }, CancellationToken.None).ConfigureAwait(false);
            try
            {
                await store.FailDeliveryAsync(delivery.OwnerUserId, delivery.Id,
                    "Agent run was cancelled.", CancellationToken.None).ConfigureAwait(false);
            }
            catch
            {
            }

            RaiseChanged(delivery, "run_cancelled");
            throw;
        }
        catch (Exception exception)
        {
            var detail = SafeError(exception);
            var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            await store.SaveRunAsync(run with
            {
                Status = AgentRunStatus.Failed,
                LastError = detail,
                UpdatedAtUnixMs = now,
            }, CancellationToken.None).ConfigureAwait(false);
            try
            {
                await store.FailDeliveryAsync(delivery.OwnerUserId, delivery.Id, detail,
                    CancellationToken.None).ConfigureAwait(false);
            }
            catch
            {
            }

            await RecordTodoFailureAsync(delivery, detail).ConfigureAwait(false);
            RaiseChanged(delivery, "run_failed");
        }
    }

    private async Task RunDeliveryAsync(
        AgentDelivery delivery,
        AgentRunSummary initialRun,
        CancellationToken cancellationToken)
    {
        var room = await store.GetRoomAsync(delivery.OwnerUserId, delivery.RoomId, cancellationToken)
            .ConfigureAwait(false) ?? throw new AgentTeamException(AgentTeamError.NotFound,
                "Agent room was not found.");
        var profile = await store.GetAgentAsync(
            delivery.OwnerUserId, delivery.TargetAgentId, cancellationToken).ConfigureAwait(false);
        if (profile?.Status != AgentProfileStatus.Active)
        {
            throw new AgentTeamException(AgentTeamError.NotFound, "Target Agent is unavailable.");
        }

        var member = (await store.ListMembersAsync(delivery.OwnerUserId, room.Id,
                includeRemoved: false, cancellationToken).ConfigureAwait(false))
            .FirstOrDefault(value => string.Equals(
                value.AgentId, profile.Id, StringComparison.Ordinal))
            ?? throw new AgentTeamException(AgentTeamError.NotMember,
                "Target Agent is no longer a team member.");
        await BeginTodoExecutionAsync(delivery, cancellationToken).ConfigureAwait(false);
        var recentMessages = await store.ListMessagesAsync(
            delivery.OwnerUserId, room.Id, 120, includeAttachmentPayloads: false,
            cancellationToken).ConfigureAwait(false);
        var todos = await store.ListTodosAsync(
            delivery.OwnerUserId, room.Id, includeTerminal: true, cancellationToken).ConfigureAwait(false);
        var assets = await store.ListAssetsAsync(
            delivery.OwnerUserId, room.Id, includeArchived: false, cancellationToken).ConfigureAwait(false);
        var todoProgress = await TriggerTodoProgressAsync(delivery, cancellationToken)
            .ConfigureAwait(false);
        var references = new AgentRunReferenceVault();
        await using var pluginSession = pluginTools is null
            ? null
            : await pluginTools.PrepareAsync(profile, member, room, initialRun.Id,
                cancellationToken).ConfigureAwait(false);
        var input = BuildInput(profile, member, room, delivery, recentMessages, todos, assets,
            todoProgress, pluginSession?.Instructions, references);
        var definitions = tools.AllDefinitions(profile, room, delivery)
            .Concat(pluginSession?.Definitions ?? []).ToArray();
        var run = initialRun;
        string? responseMessageId = null;
        var ended = false;
        for (var modelCall = 1; modelCall <= 16 && !ended; modelCall++)
        {
            var turn = await models.CompleteAsync(profile, input, definitions, cancellationToken)
                .ConfigureAwait(false);
            run = run with
            {
                ModelCalls = modelCall,
                UpdatedAtUnixMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            };
            await store.SaveRunAsync(run, cancellationToken).ConfigureAwait(false);
            input.AddRange(turn.OutputItems.Cast<object>());

            if (turn.ToolCalls.Count == 0)
            {
                if (string.IsNullOrWhiteSpace(turn.Content))
                {
                    throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                        "Agent model returned neither content nor a tool call.");
                }

                var post = await store.PostMessageAsync(delivery.OwnerUserId, room.Id,
                    new AgentMessageDraft(AgentMessageSenderKind.Agent, profile.Id,
                        turn.Content.Trim(), RootMessageId: delivery.RootMessageId,
                        HopCount: delivery.HopCount + 1), cancellationToken).ConfigureAwait(false);
                responseMessageId = post.Message.Id;
                ended = true;
                continue;
            }

            foreach (var call in turn.ToolCalls)
            {
                AgentToolExecutionResult result;
                try
                {
                    result = pluginSession?.CanExecute(call.Name) == true
                        ? new AgentToolExecutionResult(await pluginSession.ExecuteAsync(
                            call, cancellationToken).ConfigureAwait(false))
                        : await tools.ExecuteAsync(profile, member, room, delivery, call,
                            cancellationToken, references).ConfigureAwait(false);
                }
                catch (Exception exception) when (exception is not OperationCanceledException)
                {
                    result = new AgentToolExecutionResult(JsonSerializer.Serialize(new
                    {
                        success = false,
                        error = SafeError(exception),
                    }));
                }

                input.Add(new Dictionary<string, object>
                {
                    ["type"] = "function_call_output",
                    ["call_id"] = call.Id,
                    ["output"] = result.Content,
                });
                responseMessageId ??= result.ResponseMessageId;
                if (result.EndsCycle)
                {
                    ended = true;
                    break;
                }
            }
        }

        if (!ended)
        {
            throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                "Agent exceeded the bounded model-call budget without completing its cycle.");
        }

        await store.CompleteDeliveryAsync(delivery.OwnerUserId, delivery.Id,
            responseMessageId, cancellationToken).ConfigureAwait(false);
        var completedAt = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        await store.SaveRunAsync(run with
        {
            Status = AgentRunStatus.Completed,
            LastError = null,
            UpdatedAtUnixMs = completedAt,
        }, cancellationToken).ConfigureAwait(false);
        RaiseChanged(delivery, "run_completed");
    }

    private static List<object> BuildInput(
        AgentProfile profile,
        AgentRoomMember member,
        AgentRoom room,
        AgentDelivery delivery,
        IReadOnlyList<AgentMessage> messages,
        IReadOnlyList<AgentTodo> todos,
        IReadOnlyList<AgentTeamAsset> assets,
        IReadOnlyList<AgentTodoProgress> todoProgress,
        string? pluginInstructions,
        AgentRunReferenceVault references)
    {
        var lane = delivery.Trigger == AgentDeliveryTrigger.Todo ? "executor" : "manager";
        var authority = string.Equals(room.ProjectManagerAgentId, profile.Id, StringComparison.Ordinal)
            ? "你是这个团队明确指定的项目经理，可以创建、分配和调整共享 Todo。"
            : "你不是这个团队的项目经理，不得创建、分配或重排团队 Todo；需要新任务时请 @ 项目经理。";
        var staffingAuthority = AgentProfilePermissions.CanManageStaff(profile)
            ? "你可用 agent_propose_* 工具发起成员变更提案；只有 Human 批准后才会生效。"
            : "你没有人员管理权限，不得发起成员新增、入队或移出提案。";
        var system = $"""
            你是 ChatOS Windows 本机 Agent 团队中的持久 Agent。
            姓名：{profile.Draft.Name}
            职业：{profile.Draft.ProfessionKey}
            团队角色：{member.Draft.Role}
            职责：{member.Draft.Responsibility}
            当前周期：{lane}
            {authority}
            {staffingAuthority}

            {profile.Draft.RolePrompt}

            规则：
            1. 团队协作使用 team_send，并用 mention_agent_refs 精确唤醒责任人。
            2. 团队任务以 todo_list 为权威状态；执行者持续记录 todo_progress，完成时用 todo_update。
            3. 只有项目经理可维护版本化共享资产：首次创建用 asset_create，已有资产先 asset_list 再用 asset_update 和当前 revision；执行者只能用 todo_progress 的 asset_update_suggestions 提交完整替换建议。
            4. 项目文件和命令只通过提供的 project_* 与 terminal_exec 工具访问，不能编造结果。
            5. 需求调研 Skill Catalog：SKreq-router=requirement-survey(router)，SKreq-create=create，SKreq-read=read-results，SKreq-resolve=resolve，SKreq-review=review-execution。需要调研能力时先用 skill_activate 激活 SKreq-router，再只激活当前目标对应的专业 Skill；需要示例时才用 skill_list_resources/skill_read_resource。创建后立即结束，不能代替 Human 提交。
            6. 用 chat_read_all_unread 检查账号内其他团队和私聊的新消息；返回即已读，当前会话用 chat_read_unread 后按需 chat_mark_read。
            7. 人员变更只能创建待审批提案，不能声称已经创建 Agent、加入团队或移出成员。
            8. 完成本轮且无需发送消息时调用 cycle_complete；不要发送无意义的在线通知。
            所有 *_ref 都只在本轮有效，不得猜测或输出真实数据库 ID；引用失效时重新调用相应读取工具。

            {pluginInstructions}
            """;
        var context = new StringBuilder()
            .AppendLine($"团队：{room.Draft.Name}")
            .AppendLine($"目标：{room.Draft.Goal}")
            .AppendLine($"会话引用：{references.ConversationReference(room.Id)}")
            .AppendLine($"触发：{delivery.Trigger}；消息引用：{references.MessageReference(room.Id, delivery.MessageId)}")
            .AppendLine()
            .AppendLine("最近消息：");
        foreach (var message in messages.TakeLast(80))
        {
            var sender = message.SenderKind switch
            {
                AgentMessageSenderKind.Human => "Human",
                AgentMessageSenderKind.System => "System",
                _ => message.SenderAgentId is null ? "Agent" :
                    $"Agent({references.AgentReference(message.SenderAgentId)})",
            };
            context.Append('[').Append(sender).Append("] ").AppendLine(message.Content);
            foreach (var attachment in message.Attachments)
            {
                context.Append("  [附件 ref=").Append(references.AttachmentReference(
                        room.Id, attachment.Id)).Append(" name=")
                    .Append(attachment.Name).Append(" mime=").Append(attachment.MimeType)
                    .Append(" bytes=").Append(attachment.ByteCount)
                    .AppendLine("；文本内容可用 chat_read_attachment 按需读取]");
            }
        }

        context.AppendLine().AppendLine("共享 Todo：")
            .AppendLine(JsonSerializer.Serialize(todos.Select(value => new
            {
                todo_ref = references.TodoReference(room.Id, value.Id, value.Draft.AgentId),
                assignee_ref = references.AgentReference(value.Draft.AgentId),
                value.Draft.Title,
                value.Draft.Detail,
                value.Draft.Priority,
                dependency_refs = value.Draft.Dependencies.Select(id =>
                    references.TodoReference(room.Id, id, string.Empty)),
                value.Status,
                value.Result,
                value.Revision,
            })));
        if (todoProgress.Count > 0)
        {
            context.AppendLine().AppendLine("本次触发 Todo 的执行进展与共享资产建议：")
                .AppendLine(JsonSerializer.Serialize(todoProgress.Select(value => new
                {
                    todo_ref = references.TodoReference(room.Id, value.TodoId, value.AgentId),
                    agent_ref = references.AgentReference(value.AgentId),
                    value.Sequence,
                    value.Kind,
                    value.Stage,
                    value.Detail,
                    value.AssetUpdateSuggestions,
                    value.CreatedAtUnixMs,
                })));
        }
        context.AppendLine().AppendLine("团队资产：")
            .AppendLine(JsonSerializer.Serialize(assets.Select(value => new
            {
                asset_ref = references.AssetReference(room.Id, value.Id, value.Revision),
                value.Category,
                value.Title,
                value.Markdown,
                value.Revision,
            })));
        return
        [
            new Dictionary<string, object> { ["role"] = "system", ["content"] = system },
            new Dictionary<string, object> { ["role"] = "user", ["content"] = context.ToString() },
        ];
    }

    private async Task<IReadOnlyList<AgentTodoProgress>> TriggerTodoProgressAsync(
        AgentDelivery delivery,
        CancellationToken cancellationToken)
    {
        if (delivery.Trigger is not (AgentDeliveryTrigger.Todo or AgentDeliveryTrigger.TodoStatus))
            return [];
        var parts = delivery.DeduplicationKey.Split(':');
        var todoId = parts.Length >= 2 && (parts[0] == "todo" || parts[0] == "todo-status")
            ? parts[1]
            : null;
        if (todoId is null && delivery.Trigger == AgentDeliveryTrigger.TodoStatus)
        {
            var message = await store.GetMessageAsync(delivery.OwnerUserId, delivery.RoomId,
                delivery.MessageId, cancellationToken).ConfigureAwait(false);
            todoId = message?.Content.Split('\n')[0]
                .Split(':', 2).ElementAtOrDefault(1)?.Trim();
        }
        return todoId is null
            ? []
            : await store.ListTodoProgressAsync(delivery.OwnerUserId, todoId, 50,
                cancellationToken).ConfigureAwait(false);
    }

    private async Task BeginTodoExecutionAsync(
        AgentDelivery delivery,
        CancellationToken cancellationToken)
    {
        if (delivery.Trigger != AgentDeliveryTrigger.Todo ||
            !delivery.DeduplicationKey.StartsWith("todo:", StringComparison.Ordinal))
        {
            return;
        }

        var parts = delivery.DeduplicationKey.Split(':');
        if (parts.Length < 2)
        {
            return;
        }

        var todo = await store.GetTodoAsync(delivery.OwnerUserId, parts[1], cancellationToken)
            .ConfigureAwait(false);
        if (todo?.Status != AgentTodoStatus.Ready)
        {
            return;
        }

        var updated = await store.UpdateTodoAsync(delivery.OwnerUserId, todo.Id, todo.Revision,
            AgentTodoStatus.InProgress, todo.Result, null, cancellationToken).ConfigureAwait(false);
        await store.AppendTodoProgressAsync(delivery.OwnerUserId, updated.Id,
            delivery.TargetAgentId, AgentTodoProgressKind.Started, "started",
            "Windows Agent executor claimed the ready Todo.", cancellationToken: cancellationToken)
            .ConfigureAwait(false);
    }

    private async Task RecordTodoFailureAsync(AgentDelivery delivery, string detail)
    {
        if (delivery.Trigger != AgentDeliveryTrigger.Todo ||
            !delivery.DeduplicationKey.StartsWith("todo:", StringComparison.Ordinal))
        {
            return;
        }

        var parts = delivery.DeduplicationKey.Split(':');
        if (parts.Length < 2)
        {
            return;
        }

        try
        {
            await store.AppendTodoProgressAsync(delivery.OwnerUserId, parts[1],
                delivery.TargetAgentId, AgentTodoProgressKind.Failed, "failed", detail,
                cancellationToken: CancellationToken.None).ConfigureAwait(false);
        }
        catch
        {
        }
    }

    private void RaiseChanged(AgentDelivery delivery, string kind) => Changed?.Invoke(
        this, new AgentTeamChangedEventArgs(
            delivery.OwnerUserId, null, delivery.RoomId, kind));

    private static string SafeError(Exception exception)
    {
        var message = exception switch
        {
            AgentTeamException => exception.Message,
            _ => "The local Agent run failed before completion.",
        };
        return message.Length <= 2_000 ? message : message[..2_000];
    }
}
