using System.Net;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tasks;

public sealed class MessageTaskGraphService : IMessageTaskGraphService
{
    private readonly ChatOSApiClient _client;

    public MessageTaskGraphService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<MessageTaskGraphSnapshot> FetchGraphAsync(
        string messageId,
        MessageTaskLookup? lookup,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<MessageTaskGraphResponseDto>(
            Endpoint(messageId, "graph", lookup),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task<MessageTask> FetchTaskAsync(
        string messageId,
        string taskId,
        MessageTaskLookup? lookup,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<MessageTaskDto>(
            Endpoint(messageId, $"tasks/{Path(taskId)}", lookup),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task<MessageTaskRunDetail> FetchRunAsync(
        string messageId,
        string runId,
        MessageTaskLookup? lookup,
        bool includeEvents = true,
        int eventLimit = 40,
        int eventOffset = 0,
        CancellationToken cancellationToken = default)
    {
        var extra = new Dictionary<string, string>
        {
            ["include_events"] = includeEvents ? "true" : "false",
            ["event_limit"] = Math.Clamp(eventLimit, 1, 100).ToString(),
            ["event_offset"] = Math.Max(0, eventOffset).ToString(),
        };
        var response = await _client.GetAsync<MessageTaskRunDetailDto>(
            Endpoint(messageId, $"runs/{Path(runId)}", lookup, extra),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task<MessageTaskRun> RetryRunAsync(
        string messageId,
        string runId,
        MessageTaskLookup? lookup,
        string? instruction,
        CancellationToken cancellationToken = default)
    {
        instruction = Normalize(instruction);
        var response = await _client.PostAsync<MessageTaskRetryResponseDto>(
            Endpoint(messageId, $"runs/{Path(runId)}/retry", lookup),
            new RetryMessageTaskRunRequestDto(instruction),
            cancellationToken).ConfigureAwait(false);
        if (!response.Success)
        {
            throw new ChatOSApiException("任务重试未被接受。", HttpStatusCode.Conflict);
        }

        return response.Run.ToDomain();
    }

    public async Task CancelTaskAsync(
        string messageId,
        string taskId,
        MessageTaskLookup? lookup,
        string? reason,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<MessageTaskCancelResponseDto>(
            Endpoint(messageId, $"tasks/{Path(taskId)}/cancel", lookup),
            new CancelMessageTaskRequestDto(Normalize(reason)),
            cancellationToken).ConfigureAwait(false);
        if (!response.Success)
        {
            throw new ChatOSApiException("任务取消未被接受。", HttpStatusCode.Conflict);
        }
    }

    private static string Endpoint(
        string messageId,
        string suffix,
        MessageTaskLookup? lookup,
        IReadOnlyDictionary<string, string>? extra = null)
    {
        var query = new List<string>();
        if (extra is not null)
        {
            query.AddRange(extra.Select(pair => $"{Query(pair.Key)}={Query(pair.Value)}"));
        }

        AddQuery(query, "session_id", lookup?.ConversationId);
        AddQuery(query, "turn_id", lookup?.TurnId);
        AddQuery(query, "source_user_message_id", lookup?.SourceUserMessageId);
        var path = $"messages/{Path(messageId)}/task-runner/{suffix}";
        return query.Count == 0 ? path : $"{path}?{string.Join('&', query)}";
    }

    private static void AddQuery(List<string> query, string key, string? value)
    {
        value = Normalize(value);
        if (value is not null)
        {
            query.Add($"{Query(key)}={Query(value)}");
        }
    }

    private static string Path(string value) => Uri.EscapeDataString(value);

    private static string Query(string value) => Uri.EscapeDataString(value);

    private static string? Normalize(string? value)
    {
        value = value?.Trim();
        return string.IsNullOrEmpty(value) ? null : value;
    }
}

internal sealed record MessageTaskGraphResponseDto
{
    [JsonPropertyName("root_task_ids")]
    public IReadOnlyList<string> RootTaskIds { get; init; } = Array.Empty<string>();

    [JsonPropertyName("nodes")]
    public IReadOnlyList<MessageTaskGraphNodeDto> Nodes { get; init; } = Array.Empty<MessageTaskGraphNodeDto>();

    [JsonPropertyName("edges")]
    public IReadOnlyList<MessageTaskGraphEdgeDto> Edges { get; init; } = Array.Empty<MessageTaskGraphEdgeDto>();

    [JsonPropertyName("source_session_id")]
    public string? SourceConversationId { get; init; }

    [JsonPropertyName("source_turn_id")]
    public string? SourceTurnId { get; init; }

    [JsonPropertyName("source_user_message_id")]
    public string? SourceUserMessageId { get; init; }

    public MessageTaskGraphSnapshot ToDomain() => new(
        RootTaskIds,
        Nodes.Select(static value => value.ToDomain()).ToArray(),
        Edges.Select(static value => value.ToDomain()).ToArray(),
        SourceConversationId,
        SourceTurnId,
        SourceUserMessageId);
}

internal sealed record MessageTaskGraphNodeDto
{
    [JsonPropertyName("task")]
    public required MessageTaskDto Task { get; init; }

    [JsonPropertyName("depth")]
    public int Depth { get; init; }

    [JsonPropertyName("is_root")]
    public bool IsRoot { get; init; }

    [JsonPropertyName("is_current_message")]
    public bool IsCurrentMessage { get; init; }

    public MessageTaskGraphNode ToDomain() => new(
        Task.ToDomain(),
        Math.Max(0, Depth),
        IsRoot,
        IsCurrentMessage,
        Array.Empty<MessageTask>());
}

internal sealed record MessageTaskGraphEdgeDto(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("source")] string Source,
    [property: JsonPropertyName("target")] string Target,
    [property: JsonPropertyName("kind")] string? Kind)
{
    public MessageTaskGraphEdge ToDomain() => new(Id, Source, Target, Kind ?? "prerequisite");
}

internal sealed record MessageTaskReferenceDto(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("title")] string? Title,
    [property: JsonPropertyName("status")] string? Status)
{
    public MessageTaskReference ToDomain() => new(Id, Trim(Title), Trim(Status));

    private static string? Trim(string? value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}

internal sealed record MessageTaskModelConfigDto(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("name")] string? Name,
    [property: JsonPropertyName("provider")] string? Provider,
    [property: JsonPropertyName("model")] string? Model)
{
    public MessageTaskModelConfigSummary ToDomain() => new(Id, Name, Provider, Model);
}

internal sealed record MessageTaskDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("title")]
    public string? Title { get; init; }

    [JsonPropertyName("description")]
    public string? Description { get; init; }

    [JsonPropertyName("objective")]
    public string? Objective { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("priority")]
    public int? Priority { get; init; }

    [JsonPropertyName("tags")]
    public IReadOnlyList<string> Tags { get; init; } = Array.Empty<string>();

    [JsonPropertyName("default_model_config_id")]
    public string? DefaultModelConfigId { get; init; }

    [JsonPropertyName("default_model_config")]
    public MessageTaskModelConfigDto? DefaultModelConfig { get; init; }

    [JsonPropertyName("creator_user_id")]
    public string? CreatorUserId { get; init; }

    [JsonPropertyName("creator_username")]
    public string? CreatorUsername { get; init; }

    [JsonPropertyName("creator_display_name")]
    public string? CreatorDisplayName { get; init; }

    [JsonPropertyName("result_summary")]
    public string? ResultSummary { get; init; }

    [JsonPropertyName("process_log")]
    public string? ProcessLog { get; init; }

    [JsonPropertyName("last_run_id")]
    public string? LastRunId { get; init; }

    [JsonPropertyName("last_run")]
    public MessageTaskRunSummaryDto? LastRun { get; init; }

    [JsonPropertyName("parent_task_id")]
    public string? ParentTaskId { get; init; }

    [JsonPropertyName("parent_task")]
    public MessageTaskReferenceDto? ParentTask { get; init; }

    [JsonPropertyName("source_run_id")]
    public string? SourceRunId { get; init; }

    [JsonPropertyName("source_run")]
    public MessageTaskRunSummaryDto? SourceRun { get; init; }

    [JsonPropertyName("source_session_id")]
    public string? SourceConversationId { get; init; }

    [JsonPropertyName("source_turn_id")]
    public string? SourceTurnId { get; init; }

    [JsonPropertyName("source_user_message_id")]
    public string? SourceUserMessageId { get; init; }

    [JsonPropertyName("prerequisite_task_ids")]
    public IReadOnlyList<string> PrerequisiteTaskIds { get; init; } = Array.Empty<string>();

    [JsonPropertyName("prerequisite_tasks")]
    public IReadOnlyList<MessageTaskReferenceDto> PrerequisiteTasks { get; init; } = Array.Empty<MessageTaskReferenceDto>();

    [JsonPropertyName("project_task_id")]
    public string? ProjectTaskId { get; init; }

    [JsonPropertyName("execution_client_ref")]
    public string? ExecutionClientRef { get; init; }

    [JsonPropertyName("dependency_context_refs")]
    public IReadOnlyList<string> DependencyContextRefs { get; init; } = Array.Empty<string>();

    [JsonPropertyName("schedule")]
    public JsonElement Schedule { get; init; }

    [JsonPropertyName("task_tool_state")]
    public JsonElement TaskToolState { get; init; }

    [JsonPropertyName("mcp_config")]
    public JsonElement McpConfig { get; init; }

    [JsonPropertyName("input_payload")]
    public JsonElement InputPayload { get; init; }

    [JsonPropertyName("created_at")]
    public string? CreatedAt { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }

    public MessageTask ToDomain()
    {
        var input = InputPayload.ValueKind == JsonValueKind.Object ? InputPayload : default;
        return new MessageTask(
            Id,
            Normalize(Title) ?? Id,
            Normalize(Description),
            Normalize(Objective),
            Normalize(Status),
            Priority,
            Tags,
            Normalize(DefaultModelConfigId),
            DefaultModelConfig?.ToDomain(),
            Normalize(CreatorUserId),
            Normalize(CreatorUsername),
            Normalize(CreatorDisplayName),
            Normalize(ResultSummary),
            Normalize(ProcessLog),
            Normalize(LastRunId) ?? LastRun?.Id,
            Normalize(LastRun?.Status),
            LastRun?.ToDomain(),
            Normalize(ParentTaskId),
            ParentTask?.ToDomain(),
            Normalize(SourceRunId),
            SourceRun?.ToDomain(),
            Normalize(SourceConversationId),
            Normalize(SourceTurnId),
            Normalize(SourceUserMessageId),
            PrerequisiteTaskIds,
            PrerequisiteTasks.Select(static value => value.ToDomain()).ToArray(),
            input.String("project_task_id") ?? Normalize(ProjectTaskId),
            input.String("execution_client_ref") ?? Normalize(ExecutionClientRef),
            input.StringArray("dependency_context_refs") ?? DependencyContextRefs,
            Schedule.JsonOrNull(),
            TaskToolState.JsonOrNull(),
            McpConfig.JsonOrNull(),
            InputPayload.JsonOrNull(),
            ParseDate(CreatedAt),
            ParseDate(UpdatedAt));
    }

    private static string? Normalize(string? value)
    {
        value = value?.Trim();
        return string.IsNullOrEmpty(value) ? null : value;
    }

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var parsed) ? parsed : null;
}

internal sealed record MessageTaskRunSummaryDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("model_phase_status")]
    public string? ModelPhaseStatus { get; init; }

    [JsonPropertyName("result_summary")]
    public string? ResultSummary { get; init; }

    [JsonPropertyName("report")]
    public JsonElement Report { get; init; }

    [JsonPropertyName("error_message")]
    public string? ErrorMessage { get; init; }

    [JsonPropertyName("started_at")]
    public string? StartedAt { get; init; }

    [JsonPropertyName("finished_at")]
    public string? FinishedAt { get; init; }

    public MessageTaskLastRunSummary ToDomain() => new(
        Id,
        Status,
        ModelPhaseStatus,
        ResultSummary,
        Report.ReportContent(),
        ErrorMessage,
        ParseDate(StartedAt),
        ParseDate(FinishedAt));

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var parsed) ? parsed : null;
}

internal sealed record MessageTaskRunDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("task_id")]
    public required string TaskId { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("model_phase_status")]
    public string? ModelPhaseStatus { get; init; }

    [JsonPropertyName("started_at")]
    public string? StartedAt { get; init; }

    [JsonPropertyName("finished_at")]
    public string? FinishedAt { get; init; }

    [JsonPropertyName("result_summary")]
    public string? ResultSummary { get; init; }

    [JsonPropertyName("report")]
    public JsonElement Report { get; init; }

    [JsonPropertyName("error_message")]
    public string? ErrorMessage { get; init; }

    public MessageTaskRun ToDomain() => new(
        Id,
        TaskId,
        Status,
        ModelPhaseStatus,
        ParseDate(StartedAt),
        ParseDate(FinishedAt),
        ResultSummary,
        Report.ReportContent(),
        ErrorMessage);

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var parsed) ? parsed : null;
}

internal sealed record MessageTaskRunEventDto(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("event_type")] string EventType,
    [property: JsonPropertyName("message")] string? Message,
    [property: JsonPropertyName("created_at")] string? CreatedAt)
{
    public MessageTaskRunEvent ToDomain() => new(
        Id,
        EventType,
        string.IsNullOrWhiteSpace(Message) ? null : Message.Trim(),
        DateTimeOffset.TryParse(CreatedAt, out var parsed) ? parsed : null);
}

internal sealed record MessageTaskRunDetailDto
{
    [JsonPropertyName("task")]
    public required MessageTaskDto Task { get; init; }

    [JsonPropertyName("run")]
    public required MessageTaskRunDto Run { get; init; }

    [JsonPropertyName("events")]
    public IReadOnlyList<MessageTaskRunEventDto> Events { get; init; } = Array.Empty<MessageTaskRunEventDto>();

    [JsonPropertyName("events_total")]
    public int? EventsTotal { get; init; }

    [JsonPropertyName("events_has_more")]
    public bool? EventsHasMore { get; init; }

    public MessageTaskRunDetail ToDomain() => new(
        Task.ToDomain(),
        Run.ToDomain(),
        Events.Select(static value => value.ToDomain()).ToArray(),
        EventsTotal ?? Events.Count,
        EventsHasMore ?? false);
}

internal sealed record MessageTaskRetryResponseDto(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("run")] MessageTaskRunDto Run);

internal sealed record MessageTaskCancelResponseDto(
    [property: JsonPropertyName("success")] bool Success);

internal sealed record RetryMessageTaskRunRequestDto(
    [property: JsonPropertyName("retry_instruction")] string? RetryInstruction);

internal sealed record CancelMessageTaskRequestDto(
    [property: JsonPropertyName("reason")] string? Reason);

internal static class MessageTaskJsonExtensions
{
    public static string? String(this JsonElement value, string property)
    {
        if (value.ValueKind != JsonValueKind.Object ||
            !value.TryGetProperty(property, out var child) ||
            child.ValueKind != JsonValueKind.String)
        {
            return null;
        }

        var result = child.GetString()?.Trim();
        return string.IsNullOrEmpty(result) ? null : result;
    }

    public static IReadOnlyList<string>? StringArray(this JsonElement value, string property)
    {
        if (value.ValueKind != JsonValueKind.Object ||
            !value.TryGetProperty(property, out var child) ||
            child.ValueKind != JsonValueKind.Array)
        {
            return null;
        }

        return child.EnumerateArray()
            .Where(static item => item.ValueKind == JsonValueKind.String)
            .Select(static item => item.GetString())
            .Where(static item => !string.IsNullOrWhiteSpace(item))
            .Select(static item => item!.Trim())
            .ToArray();
    }

    public static string? JsonOrNull(this JsonElement value) =>
        value.ValueKind is JsonValueKind.Undefined or JsonValueKind.Null
            ? null
            : JsonSerializer.Serialize(value, new JsonSerializerOptions { WriteIndented = true });

    public static string? ReportContent(this JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.String)
        {
            return value.GetString();
        }

        if (value.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        foreach (var key in new[] { "content", "text", "markdown" })
        {
            if (value.TryGetProperty(key, out var child) && child.ValueKind == JsonValueKind.String)
            {
                return child.GetString();
            }
        }

        return value.JsonOrNull();
    }
}
