using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tasks;

public sealed record PluginHostIdentity(
    string PluginId,
    string ComponentKey,
    string ReleaseId,
    string Version,
    string ArtifactSha256);

public sealed record PluginHostTaskDraft(
    [property: JsonPropertyName("clientRef")] string ClientRef,
    [property: JsonPropertyName("title")] string Title,
    [property: JsonPropertyName("objective")] string Objective,
    [property: JsonPropertyName("detail")] string? Detail,
    [property: JsonPropertyName("acceptanceCriteria")] string? AcceptanceCriteria,
    [property: JsonPropertyName("prerequisiteRefs")] IReadOnlyList<string> PrerequisiteRefs);

public sealed record PluginHostTaskBatchRequest(
    [property: JsonPropertyName("idempotencyKey")] string IdempotencyKey,
    [property: JsonPropertyName("tasks")] IReadOnlyList<PluginHostTaskDraft> Tasks);

public sealed record PluginHostTaskReference(
    [property: JsonPropertyName("clientRef")] string ClientRef,
    [property: JsonPropertyName("taskID")] string TaskId,
    [property: JsonPropertyName("title")] string Title,
    [property: JsonPropertyName("status")] string Status,
    [property: JsonPropertyName("lastRunID")] string? LastRunId,
    [property: JsonPropertyName("updatedAt")] string UpdatedAt);

public sealed record PluginHostTaskBatch(
    [property: JsonPropertyName("batchID")] string BatchId,
    [property: JsonPropertyName("reused")] bool Reused,
    [property: JsonPropertyName("tasks")] IReadOnlyList<PluginHostTaskReference> Tasks);

public sealed record PluginHostTaskRunResult(
    [property: JsonPropertyName("taskID")] string TaskId,
    [property: JsonPropertyName("ok")] bool Ok,
    [property: JsonPropertyName("message")] string? Message,
    [property: JsonPropertyName("runID")] string? RunId);

public sealed class TaskRunnerHostService(ChatOSApiClient client)
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    public async Task<PluginHostTaskBatch> PrepareBatchAsync(
        PluginHostTaskBatchRequest request,
        ProjectContextSnapshot project,
        PluginHostIdentity host,
        string defaultModelConfigId,
        CancellationToken cancellationToken = default)
    {
        Validate(request, project, host, defaultModelConfigId);
        var ordered = TopologicalOrder(request.Tasks);
        var identityDigest = Digest(new BatchIdentity(
            1,
            host,
            project.ProjectId,
            project.ProjectRevision,
            request.IdempotencyKey));
        var contentDigest = Digest(new BatchContent(identityDigest, defaultModelConfigId, request.Tasks));
        var batchId = $"host-batch-{identityDigest[..32]}";
        var batchTag = $"chatos-host-batch:{identityDigest[..32]}";
        var existing = await ListTasksAsync(project.ProjectId, batchTag, cancellationToken).ConfigureAwait(false);
        var byRef = new Dictionary<string, TaskRecordDto>(StringComparer.Ordinal);
        foreach (var task in existing)
        {
            if (!TryBridge(task.InputPayload, out var bridge) ||
                !string.Equals(bridge.BatchId, batchId, StringComparison.Ordinal) ||
                !string.Equals(bridge.BatchDigest, contentDigest, StringComparison.Ordinal) ||
                !byRef.TryAdd(bridge.ClientRef, task))
            {
                throw new InvalidOperationException("Task Runner contains a conflicting host batch record.");
            }
        }

        var reused = existing.Count > 0;
        foreach (var draft in ordered)
        {
            var prerequisiteIds = draft.PrerequisiteRefs.Select(reference =>
                byRef.TryGetValue(reference, out var dependency)
                    ? dependency.Id
                    : throw new InvalidOperationException($"Task prerequisite has not been created: {reference}"))
                .ToArray();
            if (byRef.TryGetValue(draft.ClientRef, out var current))
            {
                if (!string.Equals(current.ProjectId, project.ProjectId, StringComparison.Ordinal) ||
                    !string.Equals(current.DefaultModelConfigId, defaultModelConfigId, StringComparison.Ordinal) ||
                    !current.PrerequisiteTaskIds.ToHashSet(StringComparer.Ordinal)
                        .SetEquals(prerequisiteIds))
                {
                    throw new InvalidOperationException("The idempotent task does not match the current project, model, or dependency graph.");
                }
                continue;
            }

            reused = false;
            var created = await client.PostTaskRunnerAsync<TaskRecordDto>(
                "tasks",
                new CreateTaskDto(
                    draft.Title,
                    draft.Detail,
                    draft.Objective,
                    new Dictionary<string, object?>
                    {
                        ["hostBridge"] = new Dictionary<string, object?>
                        {
                            ["schemaVersion"] = 1,
                            ["batchId"] = batchId,
                            ["batchDigest"] = contentDigest,
                            ["clientRef"] = draft.ClientRef,
                            ["pluginId"] = host.PluginId,
                            ["componentKey"] = host.ComponentKey,
                            ["releaseId"] = host.ReleaseId,
                        },
                        ["pluginPayload"] = new Dictionary<string, object?>
                        {
                            ["detail"] = draft.Detail,
                            ["acceptanceCriteria"] = draft.AcceptanceCriteria,
                        },
                    },
                    "ready",
                    [batchTag, $"chatos-plugin:{SafeTag(host.PluginId)}"],
                    defaultModelConfigId,
                    project.ProjectId,
                    project,
                    prerequisiteIds),
                cancellationToken).ConfigureAwait(false);
            if (!string.Equals(created.ProjectId, project.ProjectId, StringComparison.Ordinal))
            {
                throw new InvalidOperationException("Task Runner returned a task for another project.");
            }
            byRef[draft.ClientRef] = created;
        }

        return new PluginHostTaskBatch(
            batchId,
            reused,
            request.Tasks.Select(draft => byRef.TryGetValue(draft.ClientRef, out var task)
                ? Reference(task, draft.ClientRef)
                : throw new InvalidOperationException("Task Runner did not return the complete task batch."))
                .ToArray());
    }

    public async Task<IReadOnlyList<PluginHostTaskReference>> TaskStatusesAsync(
        IReadOnlyList<string> taskIds,
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var ids = ValidateTaskIds(taskIds);
        ValidateText(projectId, 256, "Project context is missing.");
        if (ids.Count == 0) return Array.Empty<PluginHostTaskReference>();
        var tasks = await client.GetTaskRunnerAsync<IReadOnlyList<TaskSummaryDto>>(
            $"tasks/summaries?ids={Uri.EscapeDataString(string.Join(',', ids))}&project_id={Uri.EscapeDataString(projectId)}",
            cancellationToken).ConfigureAwait(false);
        if (!tasks.Select(value => value.Id).ToHashSet(StringComparer.Ordinal).SetEquals(ids) ||
            tasks.Any(value => !string.Equals(value.ProjectId, projectId, StringComparison.Ordinal)))
        {
            throw new InvalidOperationException("Some tasks are unavailable or do not belong to the current project.");
        }
        return tasks.Select(value => new PluginHostTaskReference(
            string.Empty,
            value.Id,
            value.Title,
            value.Status,
            value.LastRunId,
            value.UpdatedAt)).ToArray();
    }

    public async Task<IReadOnlyList<PluginHostTaskRunResult>> StartBatchAsync(
        IReadOnlyList<string> taskIds,
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var ids = ValidateTaskIds(taskIds);
        _ = await TaskStatusesAsync(ids, projectId, cancellationToken).ConfigureAwait(false);
        var response = await client.PostTaskRunnerAsync<BatchRunResponseDto>(
            "tasks/batch/runs",
            new BatchRunDto(ids),
            cancellationToken).ConfigureAwait(false);
        return response.Results.Select(value => new PluginHostTaskRunResult(
            value.TaskId,
            value.Ok,
            value.Message,
            value.RunId)).ToArray();
    }

    private async Task<IReadOnlyList<TaskRecordDto>> ListTasksAsync(
        string projectId,
        string tag,
        CancellationToken cancellationToken) =>
        await client.GetTaskRunnerAsync<IReadOnlyList<TaskRecordDto>>(
            $"tasks?project_scope=project&project_id={Uri.EscapeDataString(projectId)}&tag={Uri.EscapeDataString(tag)}&limit=100",
            cancellationToken).ConfigureAwait(false);

    private static void Validate(
        PluginHostTaskBatchRequest request,
        ProjectContextSnapshot project,
        PluginHostIdentity host,
        string defaultModelConfigId)
    {
        if (request.Tasks.Count is < 1 or > 50 ||
            project.SchemaVersion != 1 ||
            !ValidText(request.IdempotencyKey, 256) ||
            !ValidText(project.ProjectId, 256) ||
            !ValidText(defaultModelConfigId, 256) ||
            new[] { host.PluginId, host.ComponentKey, host.ReleaseId, host.Version, host.ArtifactSha256 }
                .Any(value => !ValidText(value, 512)))
        {
            throw new InvalidOperationException("Task batch parameters are invalid.");
        }
        var references = new HashSet<string>(StringComparer.Ordinal);
        foreach (var task in request.Tasks)
        {
            if (!ValidReference(task.ClientRef) || !references.Add(task.ClientRef) ||
                !ValidText(task.Title, 240) || !ValidText(task.Objective, 200_000) ||
                task.PrerequisiteRefs.Count > 50 || task.PrerequisiteRefs.Any(value => !ValidReference(value)))
            {
                throw new InvalidOperationException("Task draft or dependency reference is invalid.");
            }
        }
        if (request.Tasks.SelectMany(value => value.PrerequisiteRefs).Any(value => !references.Contains(value)))
        {
            throw new InvalidOperationException("Task dependency is outside the current batch.");
        }
    }

    private static IReadOnlyList<PluginHostTaskDraft> TopologicalOrder(IReadOnlyList<PluginHostTaskDraft> tasks)
    {
        var byReference = tasks.ToDictionary(value => value.ClientRef, StringComparer.Ordinal);
        var visiting = new HashSet<string>(StringComparer.Ordinal);
        var visited = new HashSet<string>(StringComparer.Ordinal);
        var result = new List<PluginHostTaskDraft>();
        void Visit(string reference)
        {
            if (visited.Contains(reference)) return;
            if (!visiting.Add(reference)) throw new InvalidOperationException("Task dependency graph contains a cycle.");
            if (!byReference.TryGetValue(reference, out var task))
                throw new InvalidOperationException("Task dependency reference does not exist.");
            foreach (var dependency in task.PrerequisiteRefs) Visit(dependency);
            visiting.Remove(reference);
            visited.Add(reference);
            result.Add(task);
        }
        foreach (var task in tasks) Visit(task.ClientRef);
        return result;
    }

    private static IReadOnlyList<string> ValidateTaskIds(IReadOnlyList<string> values)
    {
        var ids = values.Select(value => value.Trim()).ToArray();
        if (ids.Length > 200 || ids.Distinct(StringComparer.Ordinal).Count() != ids.Length ||
            ids.Any(value => !ValidReference(value)))
        {
            throw new InvalidOperationException("Task Runner references are invalid.");
        }
        return ids;
    }

    private static bool TryBridge(JsonElement? input, out HostBridge bridge)
    {
        bridge = default!;
        if (input is not { ValueKind: JsonValueKind.Object } value ||
            !value.TryGetProperty("hostBridge", out var hostBridge) ||
            hostBridge.ValueKind != JsonValueKind.Object ||
            !hostBridge.TryGetProperty("batchId", out var batchId) ||
            !hostBridge.TryGetProperty("batchDigest", out var batchDigest) ||
            !hostBridge.TryGetProperty("clientRef", out var clientRef) ||
            batchId.ValueKind != JsonValueKind.String ||
            batchDigest.ValueKind != JsonValueKind.String ||
            clientRef.ValueKind != JsonValueKind.String)
        {
            return false;
        }
        bridge = new HostBridge(batchId.GetString()!, batchDigest.GetString()!, clientRef.GetString()!);
        return true;
    }

    private static PluginHostTaskReference Reference(TaskRecordDto task, string clientRef) =>
        new(clientRef, task.Id, task.Title, task.Status, task.LastRunId, task.UpdatedAt);

    private static string Digest<T>(T value) => Convert.ToHexString(
        SHA256.HashData(JsonSerializer.SerializeToUtf8Bytes(value, JsonOptions))).ToLowerInvariant();

    private static string SafeTag(string value) => Convert.ToHexString(
        SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant()[..32];

    private static void ValidateText(string value, int maximumBytes, string message)
    {
        if (!ValidText(value, maximumBytes)) throw new InvalidOperationException(message);
    }

    private static bool ValidText(string value, int maximumBytes) =>
        !string.IsNullOrEmpty(value) && value == value.Trim() && Encoding.UTF8.GetByteCount(value) <= maximumBytes &&
        value.All(character => !char.IsControl(character));

    private static bool ValidReference(string value) =>
        ValidText(value, 256) && !value.Contains('/') && !value.Contains('\\') && value is not ("." or "..");

    private sealed record BatchIdentity(
        int SchemaVersion,
        PluginHostIdentity Plugin,
        string ProjectId,
        long ProjectRevision,
        string IdempotencyKey);

    private sealed record BatchContent(
        string IdentityDigest,
        string DefaultModelConfigId,
        IReadOnlyList<PluginHostTaskDraft> Tasks);

    private sealed record HostBridge(string BatchId, string BatchDigest, string ClientRef);

    private sealed record CreateTaskDto(
        [property: JsonPropertyName("title")] string Title,
        [property: JsonPropertyName("description")] string? Description,
        [property: JsonPropertyName("objective")] string Objective,
        [property: JsonPropertyName("input_payload")] IReadOnlyDictionary<string, object?> InputPayload,
        [property: JsonPropertyName("status")] string Status,
        [property: JsonPropertyName("tags")] IReadOnlyList<string> Tags,
        [property: JsonPropertyName("default_model_config_id")] string DefaultModelConfigId,
        [property: JsonPropertyName("project_id")] string ProjectId,
        [property: JsonPropertyName("project_context")] ProjectContextSnapshot ProjectContext,
        [property: JsonPropertyName("prerequisite_task_ids")] IReadOnlyList<string> PrerequisiteTaskIds);

    private sealed record TaskRecordDto
    {
        [JsonPropertyName("id")] public required string Id { get; init; }
        [JsonPropertyName("title")] public required string Title { get; init; }
        [JsonPropertyName("status")] public required string Status { get; init; }
        [JsonPropertyName("project_id")] public string? ProjectId { get; init; }
        [JsonPropertyName("input_payload")] public JsonElement? InputPayload { get; init; }
        [JsonPropertyName("prerequisite_task_ids")] public IReadOnlyList<string> PrerequisiteTaskIds { get; init; } = [];
        [JsonPropertyName("default_model_config_id")] public string? DefaultModelConfigId { get; init; }
        [JsonPropertyName("last_run_id")] public string? LastRunId { get; init; }
        [JsonPropertyName("updated_at")] public required string UpdatedAt { get; init; }
    }

    private sealed record TaskSummaryDto
    {
        [JsonPropertyName("id")] public required string Id { get; init; }
        [JsonPropertyName("title")] public required string Title { get; init; }
        [JsonPropertyName("status")] public required string Status { get; init; }
        [JsonPropertyName("project_id")] public string? ProjectId { get; init; }
        [JsonPropertyName("last_run_id")] public string? LastRunId { get; init; }
        [JsonPropertyName("updated_at")] public required string UpdatedAt { get; init; }
    }

    private sealed record BatchRunDto(
        [property: JsonPropertyName("task_ids")] IReadOnlyList<string> TaskIds);

    private sealed record BatchRunResponseDto(
        [property: JsonPropertyName("results")] IReadOnlyList<BatchRunItemDto> Results);

    private sealed record BatchRunItemDto(
        [property: JsonPropertyName("task_id")] string TaskId,
        [property: JsonPropertyName("ok")] bool Ok,
        [property: JsonPropertyName("message")] string? Message,
        [property: JsonPropertyName("run_id")] string? RunId);
}
