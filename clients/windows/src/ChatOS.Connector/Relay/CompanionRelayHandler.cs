using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.Approval;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Relay;

internal sealed class CompanionRelayHandler(
    WindowsLocalAgentConversationClient conversations,
    WindowsLocalAgentConversationCommandService commands,
    WindowsLocalAgentMessageTaskGraphService messageTasks,
    WindowsLocalAgentAskUserPromptService askUser,
    WindowsLocalAgentWorkspaceService workspace,
    WindowsLocalAgentProjectConversationService projectConversations,
    IProjectRegistry projects,
    ILocalProjectsService localProjects,
    IAgentTeamStore agentStore,
    IAgentTeamService agentTeams,
    CommandApprovalCoordinator approvals) : IRelayRequestHandler
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DictionaryKeyPolicy = JsonNamingPolicy.SnakeCaseLower,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private static readonly HashSet<string> RequestTypes = new(StringComparer.Ordinal)
    {
        "companion_resources_request",
        "companion_resolve_resource_request",
        "companion_conversation_request",
        "companion_conversation_history_request",
        "companion_conversation_state_request",
        "companion_conversation_send_request",
        "companion_conversation_guidance_request",
        "companion_conversation_stop_request",
        "companion_message_tasks_request",
        "companion_ask_user_prompts_request",
        "companion_ask_user_submit_request",
        "companion_ask_user_cancel_request",
        "companion_agent_workspace_request",
        "companion_agent_conversation_request",
        "companion_agent_messages_request",
        "companion_agent_send_message_request",
        "companion_agent_open_direct_request",
        "companion_approvals_request",
        "companion_resolve_approval_request",
    };

    public bool CanHandle(string requestType) => RequestTypes.Contains(requestType);

    public string ResponseType(string requestType) => requestType.EndsWith("_request", StringComparison.Ordinal)
        ? requestType[..^"_request".Length] + "_response"
        : "companion_error_response";

    public async Task<RelayHandlerResult> HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var owner = Require(request.OwnerUserId, "owner_user_id");
        var body = request.Type switch
        {
            "companion_resources_request" => await ResourcesAsync(owner, cancellationToken),
            "companion_resolve_resource_request" => await ResolveResourceAsync(
                owner, Read<ResolveResourceRequest>(request), cancellationToken),
            "companion_conversation_request" => await ConversationAsync(
                owner, Read<ConversationRequest>(request), cancellationToken),
            "companion_conversation_history_request" => await HistoryAsync(
                owner, Read<ConversationHistoryRequest>(request), cancellationToken),
            "companion_conversation_state_request" => await StateAsync(
                owner, Read<ConversationRequest>(request), cancellationToken),
            "companion_conversation_send_request" => await SendAsync(
                Read<ConversationTurnRequest>(request), false, cancellationToken),
            "companion_conversation_guidance_request" => await SendAsync(
                Read<ConversationTurnRequest>(request), true, cancellationToken),
            "companion_conversation_stop_request" => await StopAsync(
                Read<ConversationStopRequest>(request), cancellationToken),
            "companion_message_tasks_request" => await TasksAsync(
                owner, Read<MessageTasksRequest>(request), cancellationToken),
            "companion_ask_user_prompts_request" => await PromptsAsync(
                Read<AskUserPromptsRequest>(request), cancellationToken),
            "companion_ask_user_submit_request" => await SubmitPromptAsync(
                Read<AskUserSubmitRequest>(request), cancellationToken),
            "companion_ask_user_cancel_request" => await CancelPromptAsync(
                Read<AskUserMutationRequest>(request), cancellationToken),
            "companion_agent_workspace_request" => await AgentWorkspaceAsync(
                owner, cancellationToken),
            "companion_agent_conversation_request" => await AgentConversationAsync(
                owner, Read<AgentConversationRequest>(request), cancellationToken),
            "companion_agent_messages_request" => await AgentMessagesAsync(
                owner, Read<AgentMessagesRequest>(request), cancellationToken),
            "companion_agent_send_message_request" => await SendAgentMessageAsync(
                owner, Read<AgentSendMessageRequest>(request), cancellationToken),
            "companion_agent_open_direct_request" => await OpenAgentDirectAsync(
                owner, Read<AgentOpenDirectRequest>(request), cancellationToken),
            "companion_approvals_request" => ApprovalList(),
            "companion_resolve_approval_request" => await ResolveApprovalAsync(
                Read<ResolveApprovalRequest>(request), cancellationToken),
            _ => throw new RelayRequestException(400, "Unsupported Companion request."),
        };
        return RelayHandlerResult.Ok(body);
    }

    private async Task<JsonElement> ResourcesAsync(string owner, CancellationToken cancellationToken)
    {
        _ = await workspace.FetchWorkspaceRelationsAsync(cancellationToken).ConfigureAwait(false);
        var records = await workspace.ListAllAsync(owner, cancellationToken).ConfigureAwait(false);
        var projectRecords = await projects.ListAsync(owner, false, cancellationToken).ConfigureAwait(false);
        var values = new List<object>();
        var contact = records.FirstOrDefault(value => value.Resource is
            { Kind: WindowsLocalAgentWorkspaceService.ContactResourceKind });
        values.Add(Resource(
            $"contact:{WindowsLocalAgentWorkspaceService.MainContactId}",
            "contact",
            WindowsLocalAgentWorkspaceService.MainContact.Name,
            null,
            contact));
        foreach (var project in projectRecords.Where(value => value.Status == LocalProjectStatus.Active))
        {
            var conversation = records.FirstOrDefault(value => value.Resource is
                { Kind: WindowsLocalAgentWorkspaceService.ProjectResourceKind } binding &&
                binding.ResourceId == project.Id);
            values.Add(Resource(
                $"project:{project.Id}", "project", project.Draft.Name,
                project.Draft.Description, conversation));
        }
        return Element(values);
    }

    private async Task<JsonElement> ResolveResourceAsync(
        string owner,
        ResolveResourceRequest request,
        CancellationToken cancellationToken)
    {
        var resourceId = Require(request.ResourceId, "resource_id");
        var parts = resourceId.Split(':', 2);
        if (parts.Length != 2 || parts[1].Length == 0)
            throw new RelayRequestException(404, "Companion resource was not found.");
        if (parts[0] == "contact")
        {
            _ = await workspace.FetchWorkspaceRelationsAsync(cancellationToken).ConfigureAwait(false);
            var record = (await workspace.ListAllAsync(owner, cancellationToken).ConfigureAwait(false))
                .FirstOrDefault(value => value.Resource is
                    { Kind: WindowsLocalAgentWorkspaceService.ContactResourceKind } binding &&
                    binding.ResourceId == parts[1])
                ?? throw new RelayRequestException(404, "Companion resource was not found.");
            return Element(Resource(resourceId, "contact", record.Title, null, record));
        }
        if (parts[0] != "project")
            throw new RelayRequestException(404, "Companion resource was not found.");
        var project = await projects.GetAsync(owner, parts[1], cancellationToken).ConfigureAwait(false);
        if (project is null || project.Status != LocalProjectStatus.Active)
            throw new RelayRequestException(404, "Companion resource was not found.");
        var context = await localProjects.ResolveContextAsync(owner, project.Id, cancellationToken)
            .ConfigureAwait(false);
        var conversationId = await projectConversations.EnsureConversationAsync(
            new WorkspaceProject(project.Id, project.Draft.Name, null, null, null, context),
            WindowsLocalAgentWorkspaceService.MainContact,
            cancellationToken).ConfigureAwait(false);
        var conversation = await conversations.GetAsync(owner, conversationId, cancellationToken)
            .ConfigureAwait(false);
        return Element(Resource(
            resourceId, "project", project.Draft.Name, project.Draft.Description,
            conversation.Conversation));
    }

    private async Task<JsonElement> ConversationAsync(
        string owner,
        ConversationRequest request,
        CancellationToken cancellationToken)
    {
        var detail = await conversations.GetAsync(
            owner, Require(request.ConversationId, "conversation_id"), cancellationToken)
            .ConfigureAwait(false);
        var active = detail.Turns.LastOrDefault(turn => Active(turn.Status));
        return Element(new
        {
            id = detail.Conversation.ConversationId,
            title = detail.Conversation.Title,
            status = active?.Status ?? "idle",
            message_count = detail.Messages.Count,
            updated_at = Date(detail.Conversation.UpdatedAtUnixMs),
        });
    }

    private async Task<JsonElement> HistoryAsync(
        string owner,
        ConversationHistoryRequest request,
        CancellationToken cancellationToken)
    {
        var page = await conversations.HistoryAsync(
            owner,
            Require(request.ConversationId, "conversation_id"),
            request.BeforeOrdinal,
            (uint)Math.Clamp(request.Limit ?? 40, 1, 100),
            cancellationToken).ConfigureAwait(false);
        var items = page.Messages.OrderBy(message => message.Ordinal).Select(message => new
        {
            id = message.MessageId,
            role = message.Role,
            content = Text(message.Content),
            revision = Clamp(page.Conversation.Version),
            sequence_no = Clamp(message.Ordinal),
            message_source = String(message.Metadata, "source"),
            message_mode = String(message.Metadata, "task_id") is null
                ? null : "local_task_callback",
            task_id = String(message.Metadata, "task_id"),
            created_at = Date(message.CreatedAtUnixMs),
        }).ToArray();
        return Element(new
        {
            items,
            has_more = page.NextBeforeOrdinal.HasValue,
            next_before = page.NextBeforeOrdinal?.ToString(),
            snapshot_revision = Clamp(page.Conversation.Version),
        });
    }

    private async Task<JsonElement> StateAsync(
        string owner,
        ConversationRequest request,
        CancellationToken cancellationToken)
    {
        var detail = await conversations.GetAsync(
            owner, Require(request.ConversationId, "conversation_id"), cancellationToken)
            .ConfigureAwait(false);
        var active = detail.Turns.LastOrDefault(turn => Active(turn.Status));
        return active is null ? Element<object?>(null) : Element(new
        {
            turn_id = active.TurnId,
            conversation_turn_id = active.TurnId,
            status = active.Status,
            active_in_runtime = true,
        });
    }

    private async Task<JsonElement> SendAsync(
        ConversationTurnRequest request,
        bool guidance,
        CancellationToken cancellationToken)
    {
        var command = new ConversationSendCommand(
            Require(request.ConversationId, "conversation_id"),
            Require(request.TurnId, "turn_id"),
            Require(request.Content, "content"),
            []);
        var result = guidance
            ? await commands.SendGuidanceAsync(command, cancellationToken).ConfigureAwait(false)
            : await commands.SendNewTurnAsync(command, cancellationToken).ConfigureAwait(false);
        return Element(new
        {
            accepted = result.Accepted,
            conversation_id = command.ConversationId,
            turn_id = result.TurnId,
            user_message_id = result.UserMessageId,
            message_id = result.UserMessageId,
        });
    }

    private async Task<JsonElement> StopAsync(
        ConversationStopRequest request,
        CancellationToken cancellationToken)
    {
        await commands.StopTurnAsync(
            Require(request.ConversationId, "conversation_id"),
            request.TurnId,
            cancellationToken).ConfigureAwait(false);
        return Element(new { success = true });
    }

    private async Task<JsonElement> TasksAsync(
        string owner,
        MessageTasksRequest request,
        CancellationToken cancellationToken)
    {
        var conversationId = Require(request.ConversationId, "conversation_id");
        var messageId = Require(request.MessageId, "message_id");
        string? turnId = null;
        ulong? before = null;
        for (var pageNumber = 0; pageNumber < 20 && turnId is null; pageNumber++)
        {
            var page = await conversations.HistoryAsync(
                owner, conversationId, before, 100, cancellationToken).ConfigureAwait(false);
            turnId = page.Messages.FirstOrDefault(value => value.MessageId == messageId)?.TurnId;
            if (turnId is not null || page.NextBeforeOrdinal is not { } next || next == before) break;
            before = next;
        }
        if (turnId is null) throw new RelayRequestException(404, "Conversation message was not found.");
        var lookup = new MessageTaskLookup(conversationId, turnId, messageId);
        var graph = await messageTasks.FetchGraphAsync(messageId, lookup, cancellationToken)
            .ConfigureAwait(false);
        var selected = graph.Nodes.Select(value => value.Task)
            .Where(value => request.TaskId is null || value.Id == request.TaskId).ToArray();
        var values = new List<object>();
        foreach (var task in selected)
        {
            MessageTask detail;
            try
            {
                detail = await messageTasks.FetchTaskAsync(
                    messageId, task.Id, lookup, cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                detail = task;
            }
            values.Add(TaskValue(detail));
        }
        return Element(new { items = values });
    }

    private async Task<JsonElement> PromptsAsync(
        AskUserPromptsRequest request,
        CancellationToken cancellationToken)
    {
        var prompts = await askUser.FetchPromptsAsync(
            Require(request.ConversationId, "conversation_id"),
            Math.Clamp(request.Limit ?? 100, 1, 100),
            cancellationToken).ConfigureAwait(false);
        return Element(new { success = true, prompts = prompts.Select(PromptValue).ToArray() });
    }

    private async Task<JsonElement> SubmitPromptAsync(
        AskUserSubmitRequest request,
        CancellationToken cancellationToken)
    {
        var prompt = await askUser.SubmitAsync(
            Require(request.PromptId, "prompt_id"),
            Require(request.ConversationId, "conversation_id"),
            new AskUserSubmission(request.Values ?? new Dictionary<string, string>(), Selection(request.Selection)),
            cancellationToken).ConfigureAwait(false);
        return Element(new { success = true, prompt = PromptValue(prompt) });
    }

    private async Task<JsonElement> CancelPromptAsync(
        AskUserMutationRequest request,
        CancellationToken cancellationToken)
    {
        var prompt = await askUser.CancelAsync(
            Require(request.PromptId, "prompt_id"),
            Require(request.ConversationId, "conversation_id"),
            cancellationToken).ConfigureAwait(false);
        return Element(new { success = true, prompt = PromptValue(prompt) });
    }

    private async Task<JsonElement> AgentWorkspaceAsync(
        string owner,
        CancellationToken cancellationToken)
    {
        var profiles = await agentStore.ListAgentsAsync(owner, true, cancellationToken)
            .ConfigureAwait(false);
        var rooms = await agentStore.ListRoomsAsync(owner, null, false, cancellationToken)
            .ConfigureAwait(false);
        var activeRooms = rooms.Where(value => value.Status == AgentRoomStatus.Active).ToArray();
        var summaries = new List<object>();
        foreach (var room in activeRooms)
        {
            summaries.Add(await AgentConversationSummaryAsync(
                owner, room, cancellationToken).ConfigureAwait(false));
        }
        return Element(new
        {
            teams = summaries.Where((_, index) =>
                activeRooms[index].Kind == AgentConversationKind.ProjectTeam)
                .ToArray(),
            direct_conversations = summaries.Where((_, index) =>
                activeRooms[index].Kind != AgentConversationKind.ProjectTeam).ToArray(),
            agents = profiles.Where(value => value.Status == AgentProfileStatus.Active)
                .Select(AgentSummary).ToArray(),
        });
    }

    private async Task<JsonElement> AgentConversationAsync(
        string owner,
        AgentConversationRequest request,
        CancellationToken cancellationToken)
    {
        var roomId = Require(request.RoomId, "room_id");
        var room = await agentStore.GetRoomAsync(owner, roomId, cancellationToken)
            .ConfigureAwait(false);
        if (room is null || room.Status != AgentRoomStatus.Active)
            throw new RelayRequestException(404, "Agent conversation was not found.");
        return Element(await AgentConversationDetailAsync(owner, room, cancellationToken)
            .ConfigureAwait(false));
    }

    private async Task<JsonElement> AgentMessagesAsync(
        string owner,
        AgentMessagesRequest request,
        CancellationToken cancellationToken)
    {
        var roomId = Require(request.RoomId, "room_id");
        if (request.BeforeMessageId is not null && request.AfterMessageId is not null)
            throw new RelayRequestException(400, "Message cursors are mutually exclusive.");
        var limit = Math.Clamp(request.Limit ?? 40, 1, 100);
        IReadOnlyList<AgentMessage> messages;
        var hasMore = false;
        string? next = null;
        if (request.BeforeMessageId is { } beforeId)
        {
            var before = await agentStore.GetMessageAsync(
                owner, roomId, beforeId, cancellationToken).ConfigureAwait(false)
                ?? throw new RelayRequestException(400, "Message cursor is invalid.");
            var page = await agentStore.ListMessagesAsync(
                owner, roomId, limit + 1, false, cancellationToken,
                before.CreatedAtUnixMs, before.Id).ConfigureAwait(false);
            hasMore = page.Count > limit;
            messages = page.TakeLast(limit).ToArray();
            next = hasMore ? messages.FirstOrDefault()?.Id : null;
        }
        else if (request.AfterMessageId is { } afterId)
        {
            var after = await agentStore.GetMessageAsync(
                owner, roomId, afterId, cancellationToken).ConfigureAwait(false)
                ?? throw new RelayRequestException(400, "Message cursor is invalid.");
            var recent = await agentStore.ListMessagesAsync(
                owner, roomId, 1000, false, cancellationToken).ConfigureAwait(false);
            messages = recent.Where(value =>
                value.CreatedAtUnixMs > after.CreatedAtUnixMs ||
                value.CreatedAtUnixMs == after.CreatedAtUnixMs &&
                string.CompareOrdinal(value.Id, after.Id) > 0).Take(limit).ToArray();
        }
        else
        {
            var page = await agentStore.ListMessagesAsync(
                owner, roomId, limit + 1, false, cancellationToken).ConfigureAwait(false);
            hasMore = page.Count > limit;
            messages = page.TakeLast(limit).ToArray();
            next = hasMore ? messages.FirstOrDefault()?.Id : null;
        }
        return Element(new
        {
            messages = messages.Select(AgentMessageValue).ToArray(),
            next_cursor_message_id = next,
            has_more = hasMore,
        });
    }

    private async Task<JsonElement> SendAgentMessageAsync(
        string owner,
        AgentSendMessageRequest request,
        CancellationToken cancellationToken)
    {
        _ = Require(request.ClientMessageId, "client_message_id");
        var roomId = Require(request.RoomId, "room_id");
        var room = await agentStore.GetRoomAsync(owner, roomId, cancellationToken)
            .ConfigureAwait(false);
        if (room is null || room.Status != AgentRoomStatus.Active)
            throw new RelayRequestException(404, "Agent conversation was not found.");
        if (room.Kind == AgentConversationKind.AgentAgentDirect)
            throw new RelayRequestException(403, "Agent-to-Agent conversations are read-only.");
        var result = await agentTeams.PostHumanMessageAsync(
            owner,
            roomId,
            Require(request.Content, "content"),
            request.MentionedAgentIds ?? [],
            cancellationToken: cancellationToken).ConfigureAwait(false);
        return Element(new
        {
            accepted = true,
            message = AgentMessageValue(result.Message),
            deduplicated = false,
        });
    }

    private async Task<JsonElement> OpenAgentDirectAsync(
        string owner,
        AgentOpenDirectRequest request,
        CancellationToken cancellationToken)
    {
        var room = await agentTeams.OpenDirectAsync(
            owner, Require(request.AgentId, "agent_id"), cancellationToken).ConfigureAwait(false);
        return Element(await AgentConversationDetailAsync(owner, room, cancellationToken)
            .ConfigureAwait(false));
    }

    private async Task<object> AgentConversationDetailAsync(
        string owner,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        var members = await agentStore.ListMembersAsync(
            owner, room.Id, false, cancellationToken).ConfigureAwait(false);
        var profiles = await agentStore.ListAgentsAsync(owner, true, cancellationToken)
            .ConfigureAwait(false);
        var byId = profiles.ToDictionary(value => value.Id, StringComparer.Ordinal);
        return new
        {
            conversation = await AgentConversationSummaryAsync(
                owner, room, cancellationToken).ConfigureAwait(false),
            members = members.Where(value => byId.ContainsKey(value.AgentId)).Select(value => new
            {
                agent = AgentSummary(byId[value.AgentId]),
                role = value.Draft.Role,
                responsibility = value.Draft.Responsibility,
            }).ToArray(),
        };
    }

    private async Task<object> AgentConversationSummaryAsync(
        string owner,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        var members = await agentStore.ListMembersAsync(
            owner, room.Id, false, cancellationToken).ConfigureAwait(false);
        var recent = await agentStore.ListMessagesAsync(
            owner, room.Id, 1, false, cancellationToken).ConfigureAwait(false);
        var last = recent.LastOrDefault();
        return new
        {
            id = room.Id,
            kind = room.Kind switch
            {
                AgentConversationKind.ProjectTeam => "project_team",
                AgentConversationKind.HumanAgentDirect => "human_agent_direct",
                _ => "agent_agent_direct",
            },
            title = room.Draft.Name,
            goal = room.Draft.Goal,
            project_id = room.ProjectId,
            default_agent_id = room.DefaultAgentId,
            member_count = members.Count,
            can_send = room.Kind != AgentConversationKind.AgentAgentDirect,
            updated_at_unix_ms = Math.Max(room.UpdatedAtUnixMs, last?.CreatedAtUnixMs ?? 0),
            last_message = last is null ? null : AgentMessageValue(last),
        };
    }

    private static object AgentSummary(AgentProfile profile) => new
    {
        id = profile.Id,
        name = profile.Draft.Name,
        description = profile.Draft.Description,
        profession_key = profile.Draft.ProfessionKey,
        status = profile.Status == AgentProfileStatus.Active ? "active" : "archived",
        heartbeat_enabled = profile.Draft.HeartbeatEnabled,
        last_heartbeat_at_unix_ms = profile.LastHeartbeatAtUnixMs,
        updated_at_unix_ms = profile.UpdatedAtUnixMs,
    };

    private static object AgentMessageValue(AgentMessage message) => new
    {
        id = message.Id,
        room_id = message.RoomId,
        sender_kind = message.SenderKind switch
        {
            AgentMessageSenderKind.Human => "human",
            AgentMessageSenderKind.Agent => "agent",
            _ => "system",
        },
        sender_id = message.SenderAgentId ?? message.OwnerUserId,
        content = message.Content,
        mentioned_agent_ids = message.MentionedAgentIds,
        reply_to_message_id = message.ReplyToMessageId,
        created_at_unix_ms = message.CreatedAtUnixMs,
        attachments = message.Attachments.Select(value => new
        {
            id = value.Id,
            name = value.Name,
            mime_type = value.MimeType,
            size = value.ByteCount,
            kind = value.Kind.ToString().ToLowerInvariant(),
        }).ToArray(),
    };

    private JsonElement ApprovalList() => Element(approvals.Snapshot().Select(ApprovalValue).ToArray());

    private async Task<JsonElement> ResolveApprovalAsync(
        ResolveApprovalRequest request,
        CancellationToken cancellationToken)
    {
        var action = request.Decision switch
        {
            "accept" => ConnectorApprovalAction.Accept,
            "acceptForSession" => ConnectorApprovalAction.AcceptForSession,
            "decline" => ConnectorApprovalAction.Decline,
            _ => throw new RelayRequestException(400, "Invalid approval decision."),
        };
        var id = Require(request.ApprovalId, "approval_id");
        if (!await approvals.ResolveAsync(id, action, cancellationToken).ConfigureAwait(false))
            throw new RelayRequestException(404, "Approval request was not found.");
        return Element(new { success = true, approval_id = id });
    }

    private static object Resource(
        string id,
        string kind,
        string title,
        string? subtitle,
        WindowsLocalConversationRecord? conversation) => new
    {
        id,
        kind,
        title,
        subtitle,
        conversation_id = conversation?.ConversationId,
        message_count = 0,
        updated_at = conversation is null ? null : Date(conversation.UpdatedAtUnixMs),
    };

    private static object TaskValue(MessageTask task) => new
    {
        id = task.Id,
        title = task.Title,
        description = task.Description,
        objective = task.Objective,
        status = task.Status,
        priority = task.Priority,
        tags = task.Tags,
        result_summary = task.ResultSummary,
        process_log = task.ProcessLog,
        last_run = task.LastRun is null ? null : new
        {
            id = task.LastRun.Id,
            status = task.LastRun.Status,
            model_phase_status = task.LastRun.ModelPhaseStatus,
            result_summary = task.LastRun.ResultSummary,
            report = task.LastRun.ReportContent is null ? null : new { content = task.LastRun.ReportContent },
            error_message = task.LastRun.ErrorMessage,
            started_at = task.LastRun.StartedAt,
            finished_at = task.LastRun.FinishedAt,
        },
        created_at = task.CreatedAt,
        updated_at = task.UpdatedAt,
    };

    private static object PromptValue(AskUserPrompt prompt) => new
    {
        id = prompt.Id,
        conversation_id = prompt.ConversationId,
        conversation_turn_id = prompt.TurnId,
        kind = prompt.Kind,
        status = prompt.Status.ToString().ToLowerInvariant(),
        prompt = new
        {
            title = prompt.Title,
            message = prompt.Message,
            allow_cancel = prompt.AllowsCancel,
            payload = new
            {
                fields = prompt.Fields.Select(field => new
                {
                    key = field.Key,
                    label = field.Label,
                    description = field.Description,
                    placeholder = field.Placeholder,
                    default_value = field.DefaultValue,
                    required = field.IsRequired,
                    multiline = field.IsMultiline,
                    secret = field.IsSecret,
                }).ToArray(),
                choice = prompt.Choice is null ? null : new
                {
                    multiple = prompt.Choice.AllowsMultiple,
                    options = prompt.Choice.Options.Select(option => new
                    {
                        value = option.Value,
                        label = option.Label,
                        description = option.Description,
                    }).ToArray(),
                    @default = prompt.Choice.DefaultSelection,
                    min_selections = prompt.Choice.MinimumSelectionCount,
                    max_selections = prompt.Choice.MaximumSelectionCount,
                },
            },
        },
        created_at = prompt.CreatedAt ?? DateTimeOffset.UtcNow,
        updated_at = prompt.UpdatedAt ?? prompt.CreatedAt ?? DateTimeOffset.UtcNow,
    };

    private static object ApprovalValue(ConnectorPendingApproval approval) => new
    {
        id = approval.Id,
        command = approval.Command,
        context = Path.GetFileName(Path.TrimEndingDirectorySeparator(approval.WorkingDirectory)),
        source = approval.Source,
        risk = approval.Risk.Level.ToString().ToLowerInvariant(),
        reason = approval.Reason,
        created_at = approval.CreatedAt,
        available_decisions = approval.AvailableActions.Select(action => action switch
        {
            ConnectorApprovalAction.Accept => "accept",
            ConnectorApprovalAction.AcceptForSession => "acceptForSession",
            _ => "decline",
        }).ToArray(),
    };

    private static AskUserSelection? Selection(JsonElement? value)
    {
        if (value is not { } selected || selected.ValueKind is JsonValueKind.Null or JsonValueKind.Undefined)
            return null;
        if (selected.ValueKind == JsonValueKind.String)
            return new AskUserSelection.Single(selected.GetString() ?? string.Empty);
        if (selected.ValueKind == JsonValueKind.Array)
            return new AskUserSelection.Multiple(selected.EnumerateArray()
                .Where(item => item.ValueKind == JsonValueKind.String)
                .Select(item => item.GetString()).OfType<string>().ToArray());
        throw new RelayRequestException(400, "Ask User selection is invalid.");
    }

    private static T Read<T>(RelayRequest request) where T : class =>
        request.Body.Deserialize<T>(JsonOptions)
        ?? throw new RelayRequestException(400, "Companion request body is invalid.");

    private static JsonElement Element<T>(T value) => JsonSerializer.SerializeToElement(value, JsonOptions);

    private static string Require(string? value, string field) =>
        string.IsNullOrWhiteSpace(value) ? throw new RelayRequestException(400, $"{field} is required.")
            : value.Trim();

    private static bool Active(string status) => status is not ("succeeded" or "failed" or "cancelled");

    private static string Date(long unixMilliseconds) =>
        DateTimeOffset.FromUnixTimeMilliseconds(unixMilliseconds).ToString("O");

    private static long Clamp(ulong value) => value > long.MaxValue ? long.MaxValue : (long)value;

    private static string? String(JsonElement value, string property) =>
        value.ValueKind == JsonValueKind.Object && value.TryGetProperty(property, out var child) &&
        child.ValueKind == JsonValueKind.String ? child.GetString() : null;

    private static string Text(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.String) return value.GetString() ?? string.Empty;
        if (value.ValueKind == JsonValueKind.Object)
        {
            if (value.TryGetProperty("text", out var text)) return Text(text);
            if (value.TryGetProperty("content", out var content)) return Text(content);
        }
        return value.ValueKind is JsonValueKind.Null or JsonValueKind.Undefined
            ? string.Empty : value.ToString();
    }

    private sealed record ResolveResourceRequest(string? ResourceId);
    private sealed record ConversationRequest(string? ConversationId);
    private sealed record ConversationHistoryRequest(string? ConversationId, ulong? BeforeOrdinal, int? Limit);
    private sealed record ConversationTurnRequest(string? ConversationId, string? TurnId, string? Content);
    private sealed record ConversationStopRequest(string? ConversationId, string? TurnId);
    private sealed record MessageTasksRequest(string? ConversationId, string? MessageId, string? TaskId);
    private sealed record AskUserPromptsRequest(string? ConversationId, int? Limit);
    private sealed record AskUserMutationRequest(string? ConversationId, string? PromptId);
    private sealed record AskUserSubmitRequest(
        string? ConversationId,
        string? PromptId,
        IReadOnlyDictionary<string, string>? Values,
        JsonElement? Selection);
    private sealed record ResolveApprovalRequest(string? ApprovalId, string? Decision);
    private sealed record AgentConversationRequest(string? RoomId);
    private sealed record AgentMessagesRequest(
        string? RoomId,
        string? BeforeMessageId,
        string? AfterMessageId,
        int? Limit);
    private sealed record AgentSendMessageRequest(
        string? RoomId,
        string? Content,
        IReadOnlyList<string>? MentionedAgentIds,
        string? ClientMessageId);
    private sealed record AgentOpenDirectRequest(string? AgentId);
}
