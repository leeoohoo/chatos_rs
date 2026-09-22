using System.Text;
using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed record AgentToolExecutionResult(
    string Content,
    bool EndsCycle = false,
    string? ResponseMessageId = null);

internal sealed partial class AgentTeamToolExecutor(
    IAgentTeamStore store,
    AgentProjectToolExecutor projectTools,
    AgentPluginToolRuntime? pluginTools = null)
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public static IReadOnlyList<AgentToolDefinition> Definitions { get; } =
    [
        Tool("team_members", "列出当前团队或私聊的有效成员、角色和本轮临时 Agent 引用。", ObjectSchema()),
        Tool("agent_workspace_snapshot", "读取当前账户的活跃 Agent、项目团队、成员关系和本轮临时引用。", ObjectSchema()),
        Tool("team_send", "向当前团队或私聊发送消息，可精确 @ 其他 Agent。", new
        {
            type = "object",
            properties = new
            {
                content = new { type = "string", maxLength = 64_000 },
                mention_agent_refs = new
                {
                    type = "array",
                    items = new { type = "string" },
                    maxItems = 64,
                },
                document_refs = DocumentReferenceSchema(),
            },
            required = new[] { "content" },
            additionalProperties = false,
        }),
        Tool("direct_send", "打开或复用与另一个 Agent 的私聊，并发送消息唤醒对方。", new
        {
            type = "object",
            properties = new
            {
                target_agent_ref = new { type = "string" },
                content = new { type = "string", maxLength = 64_000 },
                document_refs = DocumentReferenceSchema(),
            },
            required = new[] { "target_agent_ref", "content" },
            additionalProperties = false,
        }),
        Tool("chat_read_attachment",
            "按当前会话附件 ID 分段读取 UTF-8 文本附件；二进制附件不会作为文本返回。", new
        {
            type = "object",
            properties = new
            {
                attachment_ref = new { type = "string" },
                offset = new { type = "integer", minimum = 0 },
                limit = new { type = "integer", minimum = 1, maximum = 12_000 },
            },
            required = new[] { "attachment_ref" },
            additionalProperties = false,
        }),
        Tool("chat_read_unread", "读取当前 Agent 在当前会话中的未读消息；读取后用 chat_mark_read 单调推进已读游标。", new
        {
            type = "object",
            properties = new { limit = new { type = "integer", minimum = 1, maximum = 100 } },
            additionalProperties = false,
        }),
        Tool("chat_read_all_unread", "读取当前 Agent 在账号内全部团队和私聊的未读消息，并原子推进各会话已读游标。", new
        {
            type = "object",
            properties = new { limit = new { type = "integer", minimum = 1, maximum = 500 } },
            additionalProperties = false,
        }),
        Tool("chat_mark_read", "把当前 Agent 的当前会话已读游标单调推进到指定消息；旧调用不会回退游标。", new
        {
            type = "object",
            properties = new { through_message_ref = new { type = "string", maxLength = 600 } },
            required = new[] { "through_message_ref" },
            additionalProperties = false,
        }),
        Tool("todo_list", "读取当前团队共享任务板。", ObjectSchema()),
        Tool("todo_execution_options", "读取当前 Agent 可用于新 Todo 的 builtin 能力与本轮临时 Plugin 选项。", ObjectSchema()),
        Tool("todo_create", "项目经理用不可变执行合同创建并分配团队 Todo，可声明来源消息和前置依赖。", new
        {
            type = "object",
            properties = new
            {
                assignee_ref = new { type = "string" },
                title = new { type = "string", maxLength = 500 },
                detail = new { type = "string", maxLength = 16_000 },
                objective = new { type = "string", maxLength = 8_000 },
                scope = new { type = "string", maxLength = 16_000 },
                expected_outputs = TextArraySchema(64, 4_000),
                acceptance_criteria = TextArraySchema(64, 4_000),
                constraints = TextArraySchema(64, 4_000),
                priority = new { type = "string", @enum = Enum.GetNames<AgentTodoPriority>() },
                requires_execution = new { type = "boolean" },
                builtin_capabilities = new
                {
                    type = "array",
                    items = new
                    {
                        type = "string",
                        @enum = new[] { "project_read", "project_write", "terminal",
                            "requirement_survey_read", "requirement_survey_write" },
                    },
                    maxItems = 5,
                    uniqueItems = true,
                },
                plugin_hints = new
                {
                    type = "array",
                    maxItems = 20,
                    uniqueItems = true,
                    items = new
                    {
                        type = "object",
                        properties = new
                        {
                            plugin_ref = new { type = "string" },
                            reason = new { type = "string", maxLength = 1_000 },
                        },
                        required = new[] { "plugin_ref" },
                        additionalProperties = false,
                    },
                },
                dependency_refs = new
                {
                    type = "array",
                    items = new { type = "string" },
                    maxItems = 100,
                },
                source_message_refs = new
                {
                    type = "array",
                    items = new { type = "string" },
                    minItems = 1,
                    maxItems = 64,
                    uniqueItems = true,
                },
            },
            required = new[] { "assignee_ref", "title", "objective", "scope",
                "expected_outputs", "acceptance_criteria", "source_message_refs" },
            additionalProperties = false,
        }),
        Tool("todo_schedule_state", "读取当前 Agent 的程序计算调度状态：busy、ready 或 idle。", ObjectSchema()),
        Tool("todo_start_next", "为当前 Agent 原子启动最高优先级的下一项 Ready Todo；忙碌时不会重复启动。", ObjectSchema()),
        Tool("todo_update", "仅供项目经理重新排队、取消或改派 Todo；完成和阻塞由独立执行通道写入。", new
        {
            type = "object",
            properties = new
            {
                todo_ref = new { type = "string" },
                expected_revision = new { type = "integer", minimum = 1 },
                status = new
                {
                    type = "string",
                    @enum = new[] { nameof(AgentTodoStatus.Pending),
                        nameof(AgentTodoStatus.Cancelled) },
                },
                result = new { type = "string", maxLength = 16_000 },
                assigned_agent_ref = new { type = "string" },
            },
            required = new[] { "todo_ref", "expected_revision", "status" },
            additionalProperties = false,
        }),
        Tool("todo_complete", "仅供当前 Todo 执行通道提交完成结果。", new
        {
            type = "object",
            properties = new
            {
                todo_ref = new { type = "string" },
                expected_revision = new { type = "integer", minimum = 1 },
                summary = new { type = "string", maxLength = 16_000 },
            },
            required = new[] { "todo_ref", "expected_revision", "summary" },
            additionalProperties = false,
        }),
        Tool("todo_block", "仅供当前 Todo 执行通道提交阻塞原因。", new
        {
            type = "object",
            properties = new
            {
                todo_ref = new { type = "string" },
                expected_revision = new { type = "integer", minimum = 1 },
                reason = new { type = "string", maxLength = 16_000 },
            },
            required = new[] { "todo_ref", "expected_revision", "reason" },
            additionalProperties = false,
        }),
        Tool("todo_progress", "记录分配给自己的 Todo 执行进展。", new
        {
            type = "object",
            properties = new
            {
                todo_ref = new { type = "string" },
                kind = new { type = "string", @enum = Enum.GetNames<AgentTodoProgressKind>() },
                stage = new { type = "string", maxLength = 500 },
                detail = new { type = "string", maxLength = 16_000 },
                asset_update_suggestions = new
                {
                    type = "array",
                    maxItems = 8,
                    items = new
                    {
                        type = "object",
                        properties = new
                        {
                            category = new { type = "string", @enum = Enum.GetNames<AgentTeamAssetCategory>() },
                            title = new { type = "string", maxLength = 240 },
                            markdown = new { type = "string", maxLength = 128_000 },
                            rationale = new { type = "string", maxLength = 4_000 },
                        },
                        required = new[] { "category", "title", "markdown", "rationale" },
                        additionalProperties = false,
                    },
                },
            },
            required = new[] { "todo_ref", "kind", "detail" },
            additionalProperties = false,
        }),
        Tool("asset_list", "读取当前团队的共享文档和交付资产。", ObjectSchema()),
        Tool("asset_create", "项目经理首次创建一项团队共享 Markdown 资产；请先用 asset_list 排除重复资产。", new
        {
            type = "object",
            properties = new
            {
                category = new { type = "string", @enum = Enum.GetNames<AgentTeamAssetCategory>() },
                title = new { type = "string", maxLength = 240 },
                markdown = new { type = "string", maxLength = 256_000 },
            },
            required = new[] { "category", "title", "markdown" },
            additionalProperties = false,
        }),
        Tool("asset_update", "项目经理按 asset_list 返回的当前 revision 更新已有团队共享资产。", new
        {
            type = "object",
            properties = new
            {
                asset_ref = new { type = "string" },
                expected_revision = new { type = "integer", minimum = 1 },
                category = new { type = "string", @enum = Enum.GetNames<AgentTeamAssetCategory>() },
                title = new { type = "string", maxLength = 240 },
                markdown = new { type = "string", maxLength = 256_000 },
            },
            required = new[]
                { "asset_ref", "expected_revision", "category", "title", "markdown" },
            additionalProperties = false,
        }),
        Tool("cycle_complete", "确认本轮通讯或执行周期已经完成且无需再发送消息。", new
        {
            type = "object",
            properties = new { summary = new { type = "string", maxLength = 4_000 } },
            additionalProperties = false,
        }),
    ];

    public IReadOnlyList<AgentToolDefinition> AllDefinitions(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        AgentTodo? executionTodo = null)
    {
        IEnumerable<AgentToolDefinition> definitions = Definitions.Concat(DocumentDefinitions);
        if (AgentProfilePermissions.CanManageStaff(profile))
            definitions = definitions.Concat(StaffingDefinitions);
        if (room.Kind == AgentConversationKind.ProjectTeam)
        {
            if (CanManageSurveys(profile, room)) definitions = definitions.Concat(SurveyDefinitions);
            definitions = definitions.Concat(AgentProjectToolExecutor.Definitions);
        }

        var result = definitions.ToArray();
        if (delivery.Trigger == AgentDeliveryTrigger.Todo)
        {
            var executorTools = ExecutorToolNames(executionTodo?.Draft.ExecutionPlan);
            return result.Where(value => executorTools.Contains(value.Name)).ToArray();
        }

        result = result.Where(value => !IsExecutorOnlyTool(value.Name)).ToArray();

        if (!string.Equals(room.ProjectManagerAgentId, profile.Id, StringComparison.Ordinal))
        {
            var managerOnly = new HashSet<string>(StringComparer.Ordinal)
            {
                "todo_create", "todo_update", "asset_create", "asset_update",
            };
            result = result.Where(value => !managerOnly.Contains(value.Name)).ToArray();
        }
        return result;
    }

    public async Task<AgentToolExecutionResult> ExecuteAsync(
        AgentProfile profile,
        AgentRoomMember member,
        AgentRoom room,
        AgentDelivery delivery,
        AgentToolCall call,
        CancellationToken cancellationToken,
        AgentRunReferenceVault? references = null,
        AgentTodo? executionTodo = null)
    {
        var vault = references ?? new AgentRunReferenceVault(allowLegacyIds: true);
        if (delivery.Trigger == AgentDeliveryTrigger.Todo &&
            (executionTodo is null ||
             !ExecutorToolNames(executionTodo.Draft.ExecutionPlan).Contains(call.Name)))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Tool is outside this Todo's frozen capability snapshot.");
        }
        if (delivery.Trigger != AgentDeliveryTrigger.Todo && IsExecutorOnlyTool(call.Name))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Executor state tools require a Todo execution delivery.");
        }
        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(call.Arguments);
        }
        catch (JsonException exception)
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "Agent tool arguments are not valid JSON.", exception);
        }

        using (document)
        {
            var arguments = document.RootElement;
            var signature = $"{call.Name}\0{call.Arguments}";
            if (IsDurableSend(call.Name) && vault.ReplayedSend(call.Id, signature) is { } replayed)
                return replayed;
            var result = call.Name switch
            {
                "team_members" => await ListMembersAsync(
                    profile, room, vault, cancellationToken).ConfigureAwait(false),
                "agent_workspace_snapshot" => await WorkspaceSnapshotAsync(
                    profile, vault, cancellationToken).ConfigureAwait(false),
                "chat_create_document" => CreateDocument(vault, arguments),
                "chat_inbox_send" => await SendInboxAsync(
                    profile, vault, arguments, cancellationToken).ConfigureAwait(false),
                "team_send" => await SendAsync(
                    profile, room, delivery, vault, arguments, cancellationToken).ConfigureAwait(false),
                "direct_send" => await SendDirectAsync(
                    profile, vault, arguments, cancellationToken).ConfigureAwait(false),
                "chat_read_attachment" => await ReadAttachmentAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "chat_read_unread" => await ReadUnreadAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "chat_read_all_unread" => await ReadAllUnreadAsync(
                    profile, vault, arguments, cancellationToken).ConfigureAwait(false),
                "chat_mark_read" => await MarkReadAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "todo_list" => await ListTodosAsync(
                    profile, room, vault, cancellationToken).ConfigureAwait(false),
                "todo_execution_options" => await TodoExecutionOptionsAsync(
                    profile, member, vault, cancellationToken).ConfigureAwait(false),
                "todo_schedule_state" => await TodoScheduleStateAsync(
                    profile, vault, cancellationToken).ConfigureAwait(false),
                "todo_start_next" => await StartNextTodoAsync(
                    profile, vault, cancellationToken).ConfigureAwait(false),
                "todo_create" => await CreateTodoAsync(
                    profile, member, room, delivery, vault, arguments, cancellationToken)
                    .ConfigureAwait(false),
                "todo_update" => await UpdateTodoAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "todo_complete" => await FinishTodoAsync(
                    profile, room, delivery, vault, executionTodo, arguments,
                    AgentTodoStatus.Completed, cancellationToken).ConfigureAwait(false),
                "todo_block" => await FinishTodoAsync(
                    profile, room, delivery, vault, executionTodo, arguments,
                    AgentTodoStatus.Blocked, cancellationToken).ConfigureAwait(false),
                "todo_progress" => await AppendProgressAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "asset_list" => await ListAssetsAsync(
                    profile, room, vault, cancellationToken).ConfigureAwait(false),
                "asset_create" => await CreateAssetAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "asset_update" => await UpdateAssetAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                // Compatibility for a model call already in flight while upgrading from 3.0.4.
                "asset_upsert" => await UpsertAssetAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
                "skill_activate" => ActivateSkill(arguments),
                "skill_list_resources" => ListSkillResources(arguments),
                "skill_read_resource" => ReadSkillResource(arguments),
                // Compatibility for an Agent call already in flight during the 3.0.5 upgrade.
                "requirement_survey_skill_get" => ActivateSkill(
                    JsonSerializer.SerializeToElement(new
                    {
                        skill_ref = RequiredString(arguments, "scenario") switch
                        {
                            "create_survey" => "SKreq-create",
                            "read_results" => "SKreq-read",
                            "resolve_survey" => "SKreq-resolve",
                            "review_execution" => "SKreq-review",
                            _ => throw AgentTeamValidation.Invalid("scenario"),
                        },
                    })),
                "requirement_survey_create" => await CreateRequirementSurveyAsync(
                    profile, room, delivery, vault, arguments, cancellationToken).ConfigureAwait(false),
                "requirement_survey_list" => await ListRequirementSurveysAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "requirement_survey_get" => await GetRequirementSurveyAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "requirement_survey_project_tasks" =>
                    await ListRequirementSurveyProjectTasksAsync(
                        profile, room, vault, cancellationToken).ConfigureAwait(false),
                "requirement_survey_resolve" => await ResolveRequirementSurveyAsync(
                    profile, room, vault, arguments, cancellationToken).ConfigureAwait(false),
                "agent_propose_member" => await ProposeMemberAsync(
                    profile, room, delivery, call.Id, arguments, cancellationToken)
                    .ConfigureAwait(false),
                "agent_propose_existing_member" => await ProposeExistingMemberAsync(
                    profile, room, delivery, call.Id, vault, arguments, cancellationToken)
                    .ConfigureAwait(false),
                "agent_propose_member_removal" => await ProposeMemberRemovalAsync(
                    profile, room, delivery, call.Id, vault, arguments, cancellationToken)
                    .ConfigureAwait(false),
                "cycle_complete" => new AgentToolExecutionResult(
                    Json(new { completed = true, summary = OptionalString(arguments, "summary") }), true),
                _ when AgentProjectToolExecutor.Definitions.Any(value => value.Name == call.Name) =>
                    new AgentToolExecutionResult(await projectTools.ExecuteAsync(
                        profile.OwnerUserId, room, call.Name, arguments, cancellationToken)
                        .ConfigureAwait(false)),
                _ => throw new AgentTeamException(AgentTeamError.InvalidField,
                    $"Unknown Agent tool: {call.Name}"),
            };
            if (IsDurableSend(call.Name)) vault.RecordSend(call.Id, signature, result);
            return result;
        }
    }

    private async Task<AgentToolExecutionResult> ReadAttachmentAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var attachmentReference = RequiredString(arguments, "attachment_ref");
        var authority = references.Attachment(attachmentReference);
        var attachmentId = authority?.AttachmentId ?? (references.AllowsLegacyIds
            ? attachmentReference : throw AgentTeamValidation.Invalid("attachment_ref"));
        var attachmentRoomId = authority?.RoomId ?? room.Id;
        var attachment = await store.GetMessageAttachmentAsync(profile.OwnerUserId, attachmentRoomId,
            attachmentId, cancellationToken).ConfigureAwait(false)
            ?? throw new AgentTeamException(AgentTeamError.NotFound,
                "Message attachment was not found in an authorized conversation.");
        var mimeType = attachment.MimeType.Split(';', 2)[0].Trim().ToLowerInvariant();
        if (!mimeType.StartsWith("text/", StringComparison.Ordinal) && mimeType is not
            ("application/json" or "application/xml" or "application/javascript" or
             "application/yaml" or "application/toml" or "application/sql"))
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "Message attachment is not a supported text format.");
        }
        string text;
        try
        {
            text = new UTF8Encoding(false, true).GetString(attachment.Data);
        }
        catch (DecoderFallbackException exception)
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "Message attachment is not UTF-8 text.", exception);
        }
        var offset = OptionalInt(arguments, "offset") ?? 0;
        var limit = OptionalInt(arguments, "limit") ?? 12_000;
        if (offset < 0 || offset > text.Length || limit is < 1 or > 12_000)
            throw AgentTeamValidation.Invalid("attachment range");
        if (offset < text.Length && char.IsLowSurrogate(text[offset]))
            throw AgentTeamValidation.Invalid("attachment offset");
        var length = Math.Min(limit, text.Length - offset);
        if (length > 0 && offset + length < text.Length &&
            char.IsHighSurrogate(text[offset + length - 1]) &&
            char.IsLowSurrogate(text[offset + length]))
        {
            if (length == 1) length++;
            else length--;
        }
        var nextOffset = offset + length < text.Length ? offset + length : (int?)null;
        return new AgentToolExecutionResult(Json(new
        {
            attachment_ref = references.AttachmentReference(attachmentRoomId, attachment.Id),
            attachment.Name,
            attachment.MimeType,
            attachment.ByteCount,
            content = text.Substring(offset, length),
            offset,
            next_offset = nextOffset,
            truncated = nextOffset is not null,
        }));
    }

    private async Task<AgentToolExecutionResult> SendAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var mentions = StringArray(arguments, "mention_agent_refs", 64)
            .Select(value => references.AgentId(value) ?? throw AgentTeamValidation.Invalid(
                "mention_agent_refs"))
            .ToArray();
        var documentReferences = StringArray(arguments, "document_refs", 8);
        var documents = references.ReserveDocuments(documentReferences);
        var result = await store.PostMessageAsync(profile.OwnerUserId, room.Id,
            new AgentMessageDraft(
                AgentMessageSenderKind.Agent,
                profile.Id,
                RequiredString(arguments, "content"),
                mentions, documents,
                RootMessageId: delivery.RootMessageId,
            HopCount: delivery.HopCount + 1), cancellationToken).ConfigureAwait(false);
        references.ConsumeDocuments(documentReferences);
        return new AgentToolExecutionResult(Json(new
        {
            message_ref = references.MessageReference(room.Id, result.Message.Id),
            delivery_count = result.Deliveries.Count,
            result.RoutingStopReason,
        }), EndsCycle: result.Deliveries.Count == 0, ResponseMessageId: result.Message.Id);
    }

    private async Task<AgentToolExecutionResult> ListMembersAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken)
    {
        var membersTask = store.ListMembersAsync(profile.OwnerUserId, room.Id, false,
            cancellationToken);
        var profilesTask = store.ListAgentsAsync(profile.OwnerUserId, false, cancellationToken);
        await Task.WhenAll(membersTask, profilesTask).ConfigureAwait(false);
        var profiles = profilesTask.Result.ToDictionary(value => value.Id, StringComparer.Ordinal);
        return new AgentToolExecutionResult(Json(membersTask.Result.Select(member => new
        {
            agent_ref = references.AgentReference(member.AgentId),
            name = profiles.GetValueOrDefault(member.AgentId)?.Draft.Name ?? member.AgentId,
            member.Draft.Role,
            member.Draft.Responsibility,
            is_project_manager = room.ProjectManagerAgentId == member.AgentId,
        })));
    }

    private async Task<AgentToolExecutionResult> SendDirectAsync(
        AgentProfile profile,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var targetId = references.AgentId(RequiredString(arguments, "target_agent_ref"))
            ?? throw AgentTeamValidation.Invalid("target_agent_ref");
        var documentReferences = StringArray(arguments, "document_refs", 8);
        var documents = references.ReserveDocuments(documentReferences);
        var room = await store.OpenAgentDirectAsync(profile.OwnerUserId, profile.Id,
            targetId, cancellationToken).ConfigureAwait(false);
        var post = await store.PostMessageAsync(profile.OwnerUserId, room.Id,
            new AgentMessageDraft(AgentMessageSenderKind.Agent, profile.Id,
                RequiredString(arguments, "content"), [targetId], documents), cancellationToken)
            .ConfigureAwait(false);
        references.ConsumeDocuments(documentReferences);
        return new AgentToolExecutionResult(Json(new
        {
            conversation_ref = references.ConversationReference(room.Id),
            message_ref = references.MessageReference(room.Id, post.Message.Id),
            delivery_count = post.Deliveries.Count,
            post.RoutingStopReason,
        }), EndsCycle: false, ResponseMessageId: post.Message.Id);
    }

    private async Task<AgentToolExecutionResult> ListAssetsAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken)
    {
        var assets = await store.ListAssetsAsync(
            profile.OwnerUserId, room.Id, includeArchived: false, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(assets.Select(value => AssetResponse(
            value, references))));
    }

    private async Task<AgentToolExecutionResult> UpsertAssetAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireManager(profile, room);
        var asset = await store.UpsertAssetAsync(profile.OwnerUserId, room.Id,
            OptionalString(arguments, "asset_id"), profile.Id,
            ParseEnum<AgentTeamAssetCategory>(RequiredString(arguments, "category")),
            RequiredString(arguments, "title"), RequiredString(arguments, "markdown"),
            OptionalInt(arguments, "expected_revision"), cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(asset));
    }

    private Task<AgentToolExecutionResult> CreateAssetAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken) =>
        SaveAssetAsync(profile, room, arguments, null, null, cancellationToken, references);

    private Task<AgentToolExecutionResult> UpdateAssetAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var assetReference = RequiredString(arguments, "asset_ref");
        var authority = references.Asset(assetReference);
        var assetId = authority?.AssetId ?? (references.AllowsLegacyIds
            ? assetReference : throw AgentTeamValidation.Invalid("asset_ref"));
        var expectedRevision = RequiredInt(arguments, "expected_revision");
        if (authority is not null && (authority.RoomId != room.Id ||
            authority.Revision != expectedRevision))
            throw new AgentTeamException(AgentTeamError.Conflict,
                "Asset reference is stale or belongs to another team.");
        return SaveAssetAsync(profile, room, arguments, assetId, expectedRevision,
            cancellationToken, references);
    }

    private async Task<AgentToolExecutionResult> SaveAssetAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        string? assetId,
        int? expectedRevision,
        CancellationToken cancellationToken,
        AgentRunReferenceVault? references = null)
    {
        RequireManager(profile, room);
        var asset = await store.UpsertAssetAsync(profile.OwnerUserId, room.Id,
            assetId, profile.Id,
            ParseEnum<AgentTeamAssetCategory>(RequiredString(arguments, "category")),
            RequiredString(arguments, "title"), RequiredString(arguments, "markdown"),
            expectedRevision, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(references is null ? asset :
            AssetResponse(asset, references)));
    }

    private static object AssetResponse(AgentTeamAsset value,
        AgentRunReferenceVault references) => new
    {
        asset_ref = references.AssetReference(value.TeamRoomId, value.Id, value.Revision),
        value.Category,
        value.Title,
        value.Markdown,
        value.Revision,
        value.Status,
    };

    private static void RequireManager(AgentProfile profile, AgentRoom room)
    {
        if (!string.Equals(room.ProjectManagerAgentId, profile.Id, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Only the team's explicit project manager can perform this action.");
        }
    }

    private static AgentToolDefinition Tool(string name, string description, object schema) =>
        new(name, description, schema);

    private static object ObjectSchema() => new
    {
        type = "object",
        properties = new { },
        additionalProperties = false,
    };

    private static object DocumentReferenceSchema() => new
    {
        type = "array",
        items = new { type = "string", maxLength = 600 },
        maxItems = 8,
        uniqueItems = true,
    };

    private static object TextArraySchema(int maximumItems, int maximumLength) => new
    {
        type = "array",
        items = new { type = "string", maxLength = maximumLength },
        maxItems = maximumItems,
    };

    private static HashSet<string> ExecutorToolNames(AgentTodoExecutionPlan? plan)
    {
        var capabilities = plan?.Capabilities ?? [AgentTodoBuiltinCapability.ProjectRead];
        var result = new HashSet<string>(StringComparer.Ordinal)
        {
            "todo_complete", "todo_block", "todo_progress", "chat_read_attachment",
            "cycle_complete", "skill_activate", "skill_list_resources",
            "skill_read_resource",
        };
        if (capabilities.Contains(AgentTodoBuiltinCapability.ProjectRead))
            result.UnionWith(["project_list", "project_read", "project_search"]);
        if (capabilities.Contains(AgentTodoBuiltinCapability.ProjectWrite))
            result.Add("project_write");
        if (capabilities.Contains(AgentTodoBuiltinCapability.Terminal))
            result.Add("terminal_exec");
        if (capabilities.Contains(AgentTodoBuiltinCapability.RequirementSurveyRead))
            result.UnionWith(["requirement_survey_list", "requirement_survey_get",
                "requirement_survey_project_tasks"]);
        if (capabilities.Contains(AgentTodoBuiltinCapability.RequirementSurveyWrite))
            result.UnionWith(["requirement_survey_create", "requirement_survey_resolve"]);
        return result;
    }

    private static bool IsExecutorOnlyTool(string name) =>
        name is "todo_complete" or "todo_block" or "todo_progress";

    private static string Json(object value) => JsonSerializer.Serialize(value, JsonOptions);

    private static string RequiredString(JsonElement value, string name) =>
        OptionalString(value, name) is { Length: > 0 } text
            ? text
            : throw AgentTeamValidation.Invalid(name);

    private static string? OptionalString(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object && value.TryGetProperty(name, out var property) &&
        property.ValueKind == JsonValueKind.String
            ? property.GetString()
            : null;

    private static int? OptionalInt(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object && value.TryGetProperty(name, out var property) &&
        property.TryGetInt32(out var result)
            ? result
            : null;

    private static int RequiredInt(JsonElement value, string name) =>
        OptionalInt(value, name) is { } result
            ? result
            : throw AgentTeamValidation.Invalid(name);

    private static long RequiredLong(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object && value.TryGetProperty(name, out var property) &&
        property.TryGetInt64(out var result) && result > 0
            ? result
            : throw AgentTeamValidation.Invalid(name);

    private static IReadOnlyList<string> StringArray(
        JsonElement value,
        string name,
        int maximumCount)
    {
        if (!value.TryGetProperty(name, out var property) || property.ValueKind == JsonValueKind.Null)
        {
            return [];
        }

        if (property.ValueKind != JsonValueKind.Array)
        {
            throw AgentTeamValidation.Invalid(name);
        }

        var output = property.EnumerateArray().Select(item =>
            item.ValueKind == JsonValueKind.String && item.GetString() is { Length: > 0 } text
                ? text
                : throw AgentTeamValidation.Invalid(name)).ToArray();
        AgentTeamValidation.Identifiers(output, name, maximumCount);
        return output;
    }

    private static TEnum ParseEnum<TEnum>(string value) where TEnum : struct, Enum =>
        Enum.TryParse<TEnum>(value, ignoreCase: true, out var result)
            ? result
            : throw AgentTeamValidation.Invalid(typeof(TEnum).Name);
}
