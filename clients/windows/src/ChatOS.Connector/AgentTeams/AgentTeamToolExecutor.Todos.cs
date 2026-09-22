using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    private static object TodoListSchema() => new
    {
        type = "object",
        properties = new { limit = new { type = "integer", minimum = 1, maximum = 200 } },
        additionalProperties = false,
    };

    private async Task<AgentToolExecutionResult> ListTodosAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var limit = OptionalInt(arguments, "limit") ?? 100;
        if (limit is < 1 or > 200) throw AgentTeamValidation.Invalid("limit");
        var todos = await store.ListTodosAsync(profile.OwnerUserId, room.Id,
            includeTerminal: true, limit, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(todos.Select(value => TodoResponse(
            value, references))));
    }

    private async Task<AgentToolExecutionResult> CreateTodoAsync(
        AgentProfile profile,
        AgentRoomMember member,
        AgentRoom room,
        AgentDelivery delivery,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireManager(profile, room);
        var assigneeId = references.AgentId(RequiredString(arguments, "assignee_ref"))
            ?? throw AgentTeamValidation.Invalid("assignee_ref");
        var dependencies = StringArray(arguments, "dependency_refs", 100).Select(value =>
            references.Todo(value)?.TodoId ?? (references.AllowsLegacyIds
                ? value : throw AgentTeamValidation.Invalid("dependency_refs"))).ToArray();
        var sourceReferences = StringArray(arguments, "source_message_refs", 64);
        if (sourceReferences.Count == 0)
            throw AgentTeamValidation.Invalid("source_message_refs");
        var sources = sourceReferences.Select(value =>
        {
            var authority = references.Message(value);
            if (authority is not null)
                return new AgentTodoSourceDraft(authority.RoomId, authority.MessageId);
            if (references.AllowsLegacyIds)
                return new AgentTodoSourceDraft(room.Id, value);
            throw AgentTeamValidation.Invalid("source_message_refs");
        }).Distinct().ToArray();
        if (sources.Length != sourceReferences.Count)
            throw AgentTeamValidation.Invalid("source_message_refs");

        var capabilities = StringArray(arguments, "builtin_capabilities", 5)
            .Select(ParseBuiltinCapability).ToArray();
        var pluginHints = OptionalObjectArray(arguments, "plugin_hints", 20);
        var selectablePlugins = pluginHints.Count == 0 ? [] : pluginTools is null
            ? throw AgentTeamValidation.Invalid("plugin_hints")
            : await pluginTools.ListSelectableTodoPluginsAsync(profile, member,
                cancellationToken).ConfigureAwait(false);
        var selectablePluginIds = selectablePlugins.Select(value => value.PluginId)
            .ToHashSet(StringComparer.Ordinal);
        var selectedPlugins = pluginHints.Select(value =>
        {
            var authority = references.Plugin(RequiredString(value, "plugin_ref"))
                ?? throw AgentTeamValidation.Invalid("plugin_hints");
            if (!selectablePluginIds.Contains(authority.PluginId))
                throw AgentTeamValidation.Invalid("plugin_hints");
            return new AgentTodoPluginSelection(authority.PluginId, authority.DisplayName,
                OptionalString(value, "reason") ?? string.Empty);
        }).ToArray();
        if (selectedPlugins.Select(value => value.PluginId).Distinct(StringComparer.Ordinal).Count() !=
            selectedPlugins.Length)
            throw AgentTeamValidation.Invalid("plugin_hints");
        var todo = await store.CreateTodoAsync(profile.OwnerUserId, new AgentTodoDraft(
            room.Id,
            assigneeId,
            RequiredString(arguments, "title"),
            OptionalString(arguments, "detail") ?? string.Empty,
            ParseEnum<AgentTodoPriority>(OptionalString(arguments, "priority") ?? "Normal"),
            dependencies,
            sources.FirstOrDefault(value => value.ConversationId == room.Id)?.MessageId,
            new AgentTodoExecutionContract(
                RequiredString(arguments, "objective"),
                RequiredString(arguments, "scope"),
                StringArray(arguments, "expected_outputs", 64),
                StringArray(arguments, "acceptance_criteria", 64),
                StringArray(arguments, "constraints", 64)),
            new AgentTodoExecutionPlan(
                OptionalBoolean(arguments, "requires_execution") ?? true,
                capabilities.Length == 0 ? [AgentTodoBuiltinCapability.ProjectRead] : capabilities,
                selectedPlugins),
            sources), cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(TodoResponse(todo, references)));
    }

    private async Task<AgentToolExecutionResult> TodoExecutionOptionsAsync(
        AgentProfile profile,
        AgentRoomMember member,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken)
    {
        var plugins = pluginTools is null
            ? []
            : await pluginTools.ListSelectableTodoPluginsAsync(profile, member, cancellationToken)
                .ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            builtin_capabilities = new[] { "project_read", "project_write", "terminal",
                "requirement_survey_read", "requirement_survey_write" },
            plugins = plugins.Select(value => new
            {
                plugin_ref = references.PluginReference(value.PluginId, value.DisplayName),
                display_name = value.DisplayName,
                value.Description,
            }),
        }));
    }

    private async Task<AgentToolExecutionResult> TodoScheduleStateAsync(
        AgentProfile profile,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken)
    {
        var state = await store.GetTodoScheduleStateAsync(profile.OwnerUserId, profile.Id,
            cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            state = state.State,
            running_todo = state.RunningTodo is null ? null :
                TodoResponse(state.RunningTodo, references),
            ready_todo = state.ReadyTodo is null ? null : TodoResponse(state.ReadyTodo, references),
        }));
    }

    private async Task<AgentToolExecutionResult> StartNextTodoAsync(
        AgentProfile profile,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken)
    {
        var delivery = await store.StartNextReadyTodoAsync(profile.OwnerUserId, profile.Id,
            cancellationToken).ConfigureAwait(false);
        var state = await store.GetTodoScheduleStateAsync(profile.OwnerUserId, profile.Id,
            cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            status = delivery is not null ? "started" : state.RunningTodo is not null
                ? "executor_busy" : "no_ready_todo",
            todo = state.RunningTodo is null ? null : TodoResponse(state.RunningTodo, references),
        }));
    }

    private async Task<AgentToolExecutionResult> UpdateTodoAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireManager(profile, room);
        var todoReference = RequiredString(arguments, "todo_ref");
        var todoAuthority = references.Todo(todoReference);
        var todoId = todoAuthority?.TodoId ?? (references.AllowsLegacyIds
            ? todoReference : throw AgentTeamValidation.Invalid("todo_ref"));
        if (todoAuthority is not null && todoAuthority.RoomId != room.Id)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Todo reference does not belong to the current team.");
        var todo = await store.GetTodoAsync(profile.OwnerUserId, todoId, cancellationToken)
            .ConfigureAwait(false) ?? throw new AgentTeamException(AgentTeamError.NotFound,
                "Todo was not found.");
        if (!string.Equals(todo.Draft.TeamRoomId, room.Id, StringComparison.Ordinal))
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Todo does not belong to the current team.");

        var assignedReference = OptionalString(arguments, "assigned_agent_ref");
        var assignedAgentId = assignedReference is null ? null :
            references.AgentId(assignedReference) ??
            throw AgentTeamValidation.Invalid("assigned_agent_ref");
        var status = ParseEnum<AgentTodoStatus>(RequiredString(arguments, "status"));
        if (status is not (AgentTodoStatus.Pending or AgentTodoStatus.Cancelled))
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Communication runs can only requeue or cancel a Todo.");
        if (status == AgentTodoStatus.Pending &&
            todo.Status is AgentTodoStatus.InProgress or AgentTodoStatus.Completed or
                AgentTodoStatus.Cancelled)
            throw new AgentTeamException(AgentTeamError.Conflict,
                "Only a pending, ready, or blocked Todo can be requeued.");

        var updated = await store.UpdateTodoAsync(profile.OwnerUserId, todoId,
            RequiredLong(arguments, "expected_revision"),
            status,
            OptionalString(arguments, "result") ?? string.Empty, assignedAgentId,
            cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(TodoResponse(updated, references)),
            EndsCycle: updated.IsTerminal);
    }

    private async Task<AgentToolExecutionResult> FinishTodoAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        AgentRunReferenceVault references,
        AgentTodo? executionTodo,
        JsonElement arguments,
        AgentTodoStatus status,
        CancellationToken cancellationToken)
    {
        if (delivery.Trigger != AgentDeliveryTrigger.Todo || executionTodo is null ||
            status is not (AgentTodoStatus.Completed or AgentTodoStatus.Blocked))
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Todo completion requires the owning executor delivery.");
        var todoReference = RequiredString(arguments, "todo_ref");
        var authority = references.Todo(todoReference)
            ?? throw AgentTeamValidation.Invalid("todo_ref");
        if (authority.TodoId != executionTodo.Id || authority.RoomId != room.Id ||
            authority.AgentId != profile.Id || delivery.TargetAgentId != profile.Id)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Todo reference does not belong to this executor delivery.");
        var current = await store.GetTodoAsync(profile.OwnerUserId, executionTodo.Id,
            cancellationToken).ConfigureAwait(false) ?? throw new AgentTeamException(
                AgentTeamError.NotFound, "Todo was not found.");
        if (current.Status != AgentTodoStatus.InProgress ||
            current.Draft.AgentId != profile.Id)
            throw new AgentTeamException(AgentTeamError.Conflict,
                "Todo execution no longer owns the scheduled slot.");

        var detail = RequiredString(arguments,
            status == AgentTodoStatus.Completed ? "summary" : "reason");
        var updated = await store.UpdateTodoAsync(profile.OwnerUserId, current.Id,
            RequiredLong(arguments, "expected_revision"), status, detail, null,
            cancellationToken).ConfigureAwait(false);
        _ = await store.AppendTodoProgressAsync(profile.OwnerUserId, current.Id, profile.Id,
            status == AgentTodoStatus.Completed ? AgentTodoProgressKind.Completed :
                AgentTodoProgressKind.Blocked,
            status == AgentTodoStatus.Completed ? "completed" : "blocked", detail,
            cancellationToken: cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(TodoResponse(updated, references)),
            EndsCycle: true);
    }

    private async Task<AgentToolExecutionResult> AppendProgressAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var suggestions = OptionalObjectArray(arguments, "asset_update_suggestions", 8)
            .Select(value => new AgentTeamAssetUpdateSuggestion(
                ParseEnum<AgentTeamAssetCategory>(RequiredString(value, "category")),
                RequiredString(value, "title"), RequiredString(value, "markdown"),
                RequiredString(value, "rationale"))).ToArray();
        var todoReference = RequiredString(arguments, "todo_ref");
        var authority = references.Todo(todoReference);
        var todoId = authority?.TodoId ?? (references.AllowsLegacyIds
            ? todoReference : throw AgentTeamValidation.Invalid("todo_ref"));
        if (authority is not null && authority.RoomId != room.Id)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Todo reference does not belong to the current team.");
        var progress = await store.AppendTodoProgressAsync(profile.OwnerUserId, todoId,
            profile.Id, ParseEnum<AgentTodoProgressKind>(RequiredString(arguments, "kind")),
            OptionalString(arguments, "stage") ?? string.Empty,
            RequiredString(arguments, "detail"), suggestions, cancellationToken)
            .ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            todo_ref = references.TodoReference(room.Id, todoId, profile.Id),
            progress.Sequence,
            progress.Kind,
            progress.Stage,
            progress.Detail,
            progress.CreatedAtUnixMs,
        }));
    }

    private static object TodoResponse(AgentTodo value, AgentRunReferenceVault references) => new
    {
        todo_ref = references.TodoReference(value.Draft.TeamRoomId, value.Id,
            value.Draft.AgentId),
        assignee_ref = references.AgentReference(value.Draft.AgentId),
        value.Draft.Title,
        value.Draft.Detail,
        value.Draft.Priority,
        dependency_refs = value.Draft.Dependencies.Select(id =>
            references.TodoReference(value.Draft.TeamRoomId, id, string.Empty)),
        source_message_refs = value.Sources.Select(source =>
            references.MessageReference(source.ConversationId, source.MessageId)),
        execution_contract = new
        {
            objective = value.Draft.ExecutionContract!.Objective,
            scope = value.Draft.ExecutionContract.Scope,
            expected_outputs = value.Draft.ExecutionContract.Outputs,
            acceptance_criteria = value.Draft.ExecutionContract.Criteria,
            constraints = value.Draft.ExecutionContract.Limits,
        },
        execution_plan = new
        {
            requires_execution = value.Draft.ExecutionPlan!.RequiresExecution,
            builtin_capabilities = value.Draft.ExecutionPlan.Capabilities.Select(
                BuiltinCapabilityName),
            plugins = value.Draft.ExecutionPlan.SelectedPlugins.Select(plugin => new
                { display_name = plugin.DisplayName, reason = plugin.Reason }),
            selection_revision = value.Draft.ExecutionPlan.SelectionRevision,
            selected_at_unix_ms = value.Draft.ExecutionPlan.SelectedAtUnixMs,
        },
        value.Status,
        value.Result,
        value.SortOrder,
        value.Revision,
    };

    private static AgentTodoBuiltinCapability ParseBuiltinCapability(string value) => value switch
    {
        "project_read" => AgentTodoBuiltinCapability.ProjectRead,
        "project_write" => AgentTodoBuiltinCapability.ProjectWrite,
        "terminal" => AgentTodoBuiltinCapability.Terminal,
        "requirement_survey_read" => AgentTodoBuiltinCapability.RequirementSurveyRead,
        "requirement_survey_write" => AgentTodoBuiltinCapability.RequirementSurveyWrite,
        _ => throw AgentTeamValidation.Invalid("builtin_capabilities"),
    };

    private static string BuiltinCapabilityName(AgentTodoBuiltinCapability value) => value switch
    {
        AgentTodoBuiltinCapability.ProjectRead => "project_read",
        AgentTodoBuiltinCapability.ProjectWrite => "project_write",
        AgentTodoBuiltinCapability.Terminal => "terminal",
        AgentTodoBuiltinCapability.RequirementSurveyRead => "requirement_survey_read",
        AgentTodoBuiltinCapability.RequirementSurveyWrite => "requirement_survey_write",
        _ => throw AgentTeamValidation.Invalid("builtin_capabilities"),
    };

    private static bool? OptionalBoolean(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object && value.TryGetProperty(name, out var property) &&
        property.ValueKind is JsonValueKind.True or JsonValueKind.False
            ? property.GetBoolean()
            : null;
}
