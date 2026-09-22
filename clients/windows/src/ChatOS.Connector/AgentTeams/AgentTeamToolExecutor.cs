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
    AgentProjectToolExecutor projectTools)
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public static IReadOnlyList<AgentToolDefinition> Definitions { get; } =
    [
        Tool("team_members", "列出当前团队或私聊的有效成员、角色和 Agent ID。", ObjectSchema()),
        Tool("team_send", "向当前团队或私聊发送消息，可精确 @ 其他 Agent。", new
        {
            type = "object",
            properties = new
            {
                content = new { type = "string", maxLength = 64_000 },
                mention_agent_ids = new
                {
                    type = "array",
                    items = new { type = "string" },
                    maxItems = 64,
                },
            },
            required = new[] { "content" },
            additionalProperties = false,
        }),
        Tool("direct_send", "打开或复用与另一个 Agent 的私聊，并发送消息唤醒对方。", new
        {
            type = "object",
            properties = new
            {
                target_agent_id = new { type = "string" },
                content = new { type = "string", maxLength = 64_000 },
            },
            required = new[] { "target_agent_id", "content" },
            additionalProperties = false,
        }),
        Tool("chat_read_attachment",
            "按当前会话附件 ID 分段读取 UTF-8 文本附件；二进制附件不会作为文本返回。", new
        {
            type = "object",
            properties = new
            {
                attachment_id = new { type = "string" },
                offset = new { type = "integer", minimum = 0 },
                limit = new { type = "integer", minimum = 1, maximum = 12_000 },
            },
            required = new[] { "attachment_id" },
            additionalProperties = false,
        }),
        Tool("todo_list", "读取当前团队共享任务板。", ObjectSchema()),
        Tool("todo_create", "项目经理创建并分配一个团队 Todo，可声明前置依赖。", new
        {
            type = "object",
            properties = new
            {
                agent_id = new { type = "string" },
                title = new { type = "string", maxLength = 500 },
                detail = new { type = "string", maxLength = 16_000 },
                priority = new { type = "string", @enum = Enum.GetNames<AgentTodoPriority>() },
                dependency_ids = new
                {
                    type = "array",
                    items = new { type = "string" },
                    maxItems = 100,
                },
            },
            required = new[] { "agent_id", "title" },
            additionalProperties = false,
        }),
        Tool("todo_update", "更新 Todo 状态、结果或负责人。非项目经理只能更新分配给自己的 Todo。", new
        {
            type = "object",
            properties = new
            {
                todo_id = new { type = "string" },
                expected_revision = new { type = "integer", minimum = 1 },
                status = new { type = "string", @enum = Enum.GetNames<AgentTodoStatus>() },
                result = new { type = "string", maxLength = 16_000 },
                assigned_agent_id = new { type = "string" },
            },
            required = new[] { "todo_id", "expected_revision", "status" },
            additionalProperties = false,
        }),
        Tool("todo_progress", "记录分配给自己的 Todo 执行进展。", new
        {
            type = "object",
            properties = new
            {
                todo_id = new { type = "string" },
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
            required = new[] { "todo_id", "kind", "detail" },
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
                asset_id = new { type = "string" },
                expected_revision = new { type = "integer", minimum = 1 },
                category = new { type = "string", @enum = Enum.GetNames<AgentTeamAssetCategory>() },
                title = new { type = "string", maxLength = 240 },
                markdown = new { type = "string", maxLength = 256_000 },
            },
            required = new[]
                { "asset_id", "expected_revision", "category", "title", "markdown" },
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
        AgentDelivery delivery)
    {
        IEnumerable<AgentToolDefinition> definitions = Definitions;
        if (room.Kind == AgentConversationKind.ProjectTeam)
        {
            if (CanManageSurveys(profile, room)) definitions = definitions.Concat(SurveyDefinitions);
            definitions = definitions.Concat(AgentProjectToolExecutor.Definitions);
        }

        var result = definitions.ToArray();
        if (delivery.Trigger == AgentDeliveryTrigger.Todo)
        {
            var executorTools = new HashSet<string>(StringComparer.Ordinal)
            {
                "todo_list", "todo_update", "todo_progress", "asset_list",
                "chat_read_attachment", "cycle_complete",
                "skill_activate", "skill_list_resources", "skill_read_resource",
                "requirement_survey_list",
                "requirement_survey_get", "requirement_survey_project_tasks",
                "requirement_survey_create", "requirement_survey_resolve",
            };
            return result.Where(value => executorTools.Contains(value.Name) ||
                AgentProjectToolExecutor.Definitions.Any(project => project.Name == value.Name))
                .ToArray();
        }

        if (!string.Equals(room.ProjectManagerAgentId, profile.Id, StringComparison.Ordinal))
        {
            var managerOnly = new HashSet<string>(StringComparer.Ordinal)
            {
                "todo_create", "asset_create", "asset_update",
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
        CancellationToken cancellationToken)
    {
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
            return call.Name switch
            {
                "team_members" => await ListMembersAsync(
                    profile, room, cancellationToken).ConfigureAwait(false),
                "team_send" => await SendAsync(
                    profile, room, delivery, arguments, cancellationToken).ConfigureAwait(false),
                "direct_send" => await SendDirectAsync(
                    profile, arguments, cancellationToken).ConfigureAwait(false),
                "chat_read_attachment" => await ReadAttachmentAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
                "todo_list" => await ListTodosAsync(
                    profile, room, cancellationToken).ConfigureAwait(false),
                "todo_create" => await CreateTodoAsync(
                    profile, room, delivery, arguments, cancellationToken).ConfigureAwait(false),
                "todo_update" => await UpdateTodoAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
                "todo_progress" => await AppendProgressAsync(
                    profile, arguments, cancellationToken).ConfigureAwait(false),
                "asset_list" => await ListAssetsAsync(
                    profile, room, cancellationToken).ConfigureAwait(false),
                "asset_create" => await CreateAssetAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
                "asset_update" => await UpdateAssetAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
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
                    profile, room, delivery, arguments, cancellationToken).ConfigureAwait(false),
                "requirement_survey_list" => await ListRequirementSurveysAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
                "requirement_survey_get" => await GetRequirementSurveyAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
                "requirement_survey_project_tasks" =>
                    await ListRequirementSurveyProjectTasksAsync(
                        profile, room, cancellationToken).ConfigureAwait(false),
                "requirement_survey_resolve" => await ResolveRequirementSurveyAsync(
                    profile, room, arguments, cancellationToken).ConfigureAwait(false),
                "cycle_complete" => new AgentToolExecutionResult(
                    Json(new { completed = true, summary = OptionalString(arguments, "summary") }), true),
                _ when AgentProjectToolExecutor.Definitions.Any(value => value.Name == call.Name) =>
                    new AgentToolExecutionResult(await projectTools.ExecuteAsync(
                        profile.OwnerUserId, room, call.Name, arguments, cancellationToken)
                        .ConfigureAwait(false)),
                _ => throw new AgentTeamException(AgentTeamError.InvalidField,
                    $"Unknown Agent tool: {call.Name}"),
            };
        }
    }

    private async Task<AgentToolExecutionResult> ReadAttachmentAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var attachment = await store.GetMessageAttachmentAsync(profile.OwnerUserId, room.Id,
            RequiredString(arguments, "attachment_id"), cancellationToken).ConfigureAwait(false)
            ?? throw new AgentTeamException(AgentTeamError.NotFound,
                "Message attachment was not found in the current conversation.");
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
            attachment_id = attachment.Id,
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
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var result = await store.PostMessageAsync(profile.OwnerUserId, room.Id,
            new AgentMessageDraft(
                AgentMessageSenderKind.Agent,
                profile.Id,
                RequiredString(arguments, "content"),
                StringArray(arguments, "mention_agent_ids", 64),
                RootMessageId: delivery.RootMessageId,
                HopCount: delivery.HopCount + 1), cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            message_id = result.Message.Id,
            deliveries = result.Deliveries.Select(value => value.Id).ToArray(),
            result.RoutingStopReason,
        }), EndsCycle: result.Deliveries.Count == 0, ResponseMessageId: result.Message.Id);
    }

    private async Task<AgentToolExecutionResult> ListMembersAsync(
        AgentProfile profile,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        var membersTask = store.ListMembersAsync(profile.OwnerUserId, room.Id, false,
            cancellationToken);
        var profilesTask = store.ListAgentsAsync(profile.OwnerUserId, false, cancellationToken);
        await Task.WhenAll(membersTask, profilesTask).ConfigureAwait(false);
        var profiles = profilesTask.Result.ToDictionary(value => value.Id, StringComparer.Ordinal);
        return new AgentToolExecutionResult(Json(membersTask.Result.Select(member => new
        {
            agent_id = member.AgentId,
            name = profiles.GetValueOrDefault(member.AgentId)?.Draft.Name ?? member.AgentId,
            member.Draft.Role,
            member.Draft.Responsibility,
            is_project_manager = room.ProjectManagerAgentId == member.AgentId,
        })));
    }

    private async Task<AgentToolExecutionResult> SendDirectAsync(
        AgentProfile profile,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var targetId = RequiredString(arguments, "target_agent_id");
        var room = await store.OpenAgentDirectAsync(profile.OwnerUserId, profile.Id,
            targetId, cancellationToken).ConfigureAwait(false);
        var post = await store.PostMessageAsync(profile.OwnerUserId, room.Id,
            new AgentMessageDraft(AgentMessageSenderKind.Agent, profile.Id,
                RequiredString(arguments, "content"), [targetId]), cancellationToken)
            .ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            room_id = room.Id,
            message_id = post.Message.Id,
            deliveries = post.Deliveries.Select(value => value.Id).ToArray(),
            post.RoutingStopReason,
        }), EndsCycle: false, ResponseMessageId: post.Message.Id);
    }

    private async Task<AgentToolExecutionResult> ListTodosAsync(
        AgentProfile profile,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        var todos = await store.ListTodosAsync(
            profile.OwnerUserId, room.Id, includeTerminal: true, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(todos));
    }

    private async Task<AgentToolExecutionResult> CreateTodoAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireManager(profile, room);
        var priority = ParseEnum<AgentTodoPriority>(OptionalString(arguments, "priority") ?? "Normal");
        var todo = await store.CreateTodoAsync(profile.OwnerUserId, new AgentTodoDraft(
            room.Id,
            RequiredString(arguments, "agent_id"),
            RequiredString(arguments, "title"),
            OptionalString(arguments, "detail") ?? string.Empty,
            priority,
            StringArray(arguments, "dependency_ids", 100),
            delivery.MessageId), cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(todo));
    }

    private async Task<AgentToolExecutionResult> UpdateTodoAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var todoId = RequiredString(arguments, "todo_id");
        var todo = await store.GetTodoAsync(profile.OwnerUserId, todoId, cancellationToken)
            .ConfigureAwait(false) ?? throw new AgentTeamException(AgentTeamError.NotFound,
                "Todo was not found.");
        if (!string.Equals(todo.Draft.TeamRoomId, room.Id, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Todo does not belong to the current team.");
        }

        var isManager = string.Equals(room.ProjectManagerAgentId, profile.Id, StringComparison.Ordinal);
        if (!isManager && !string.Equals(todo.Draft.AgentId, profile.Id, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Only the project manager or assigned Agent can update this Todo.");
        }

        var assignedAgentId = OptionalString(arguments, "assigned_agent_id");
        if (!isManager && assignedAgentId is not null &&
            !string.Equals(assignedAgentId, profile.Id, StringComparison.Ordinal))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Only the project manager can reassign a Todo.");
        }

        var status = ParseEnum<AgentTodoStatus>(RequiredString(arguments, "status"));
        var updated = await store.UpdateTodoAsync(profile.OwnerUserId, todoId,
            RequiredLong(arguments, "expected_revision"), status,
            OptionalString(arguments, "result") ?? string.Empty, assignedAgentId,
            cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(updated), EndsCycle: updated.IsTerminal);
    }

    private async Task<AgentToolExecutionResult> AppendProgressAsync(
        AgentProfile profile,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var suggestions = OptionalObjectArray(arguments, "asset_update_suggestions", 8)
            .Select(value => new AgentTeamAssetUpdateSuggestion(
                ParseEnum<AgentTeamAssetCategory>(RequiredString(value, "category")),
                RequiredString(value, "title"), RequiredString(value, "markdown"),
                RequiredString(value, "rationale"))).ToArray();
        var progress = await store.AppendTodoProgressAsync(profile.OwnerUserId,
            RequiredString(arguments, "todo_id"), profile.Id,
            ParseEnum<AgentTodoProgressKind>(RequiredString(arguments, "kind")),
            OptionalString(arguments, "stage") ?? string.Empty,
            RequiredString(arguments, "detail"), suggestions, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(progress));
    }

    private async Task<AgentToolExecutionResult> ListAssetsAsync(
        AgentProfile profile,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        var assets = await store.ListAssetsAsync(
            profile.OwnerUserId, room.Id, includeArchived: false, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(assets));
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
        JsonElement arguments,
        CancellationToken cancellationToken) =>
        SaveAssetAsync(profile, room, arguments, null, null, cancellationToken);

    private Task<AgentToolExecutionResult> UpdateAssetAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        CancellationToken cancellationToken) =>
        SaveAssetAsync(profile, room, arguments,
            RequiredString(arguments, "asset_id"),
            RequiredInt(arguments, "expected_revision"), cancellationToken);

    private async Task<AgentToolExecutionResult> SaveAssetAsync(
        AgentProfile profile,
        AgentRoom room,
        JsonElement arguments,
        string? assetId,
        int? expectedRevision,
        CancellationToken cancellationToken)
    {
        RequireManager(profile, room);
        var asset = await store.UpsertAssetAsync(profile.OwnerUserId, room.Id,
            assetId, profile.Id,
            ParseEnum<AgentTeamAssetCategory>(RequiredString(arguments, "category")),
            RequiredString(arguments, "title"), RequiredString(arguments, "markdown"),
            expectedRevision, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(asset));
    }

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
