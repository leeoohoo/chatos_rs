namespace ChatOS.Core.Domain;

public sealed record MessageTaskReference(
    string Id,
    string? Title,
    string? Status);

public sealed record MessageTaskModelConfigSummary(
    string Id,
    string? Name,
    string? Provider,
    string? Model)
{
    public string DisplayName
    {
        get
        {
            var providerModel = string.Join("/", new[] { Provider, Model }
                .Where(static value => !string.IsNullOrWhiteSpace(value)));
            return string.Join(" · ", new[] { Name, providerModel }
                .Where(static value => !string.IsNullOrWhiteSpace(value)));
        }
    }
}

public sealed record MessageTaskLastRunSummary(
    string Id,
    string? Status,
    string? ModelPhaseStatus,
    string? ResultSummary,
    string? ReportContent,
    string? ErrorMessage,
    DateTimeOffset? StartedAt,
    DateTimeOffset? FinishedAt);

public sealed record MessageTask(
    string Id,
    string Title,
    string? Description,
    string? Objective,
    string? Status,
    int? Priority,
    IReadOnlyList<string> Tags,
    string? DefaultModelConfigId,
    MessageTaskModelConfigSummary? DefaultModelConfig,
    string? CreatorUserId,
    string? CreatorUsername,
    string? CreatorDisplayName,
    string? ResultSummary,
    string? ProcessLog,
    string? LastRunId,
    string? LastRunStatus,
    MessageTaskLastRunSummary? LastRun,
    string? ParentTaskId,
    MessageTaskReference? ParentTask,
    string? SourceRunId,
    MessageTaskLastRunSummary? SourceRun,
    string? SourceConversationId,
    string? SourceTurnId,
    string? SourceUserMessageId,
    IReadOnlyList<string> PrerequisiteTaskIds,
    IReadOnlyList<MessageTaskReference> PrerequisiteTasks,
    string? ProjectTaskId,
    string? ExecutionClientRef,
    IReadOnlyList<string> DependencyContextRefs,
    string? ScheduleJson,
    string? TaskToolStateJson,
    string? McpConfigJson,
    string? InputPayloadJson,
    DateTimeOffset? CreatedAt,
    DateTimeOffset? UpdatedAt);

public sealed record MessageTaskGraphNode(
    MessageTask Task,
    int Depth,
    bool IsRoot,
    bool IsCurrentMessage,
    IReadOnlyList<MessageTask> GroupedTasks)
{
    public string Id => Task.Id;
}

public sealed record MessageTaskGraphEdge(
    string Id,
    string SourceId,
    string TargetId,
    string Kind);

public sealed record MessageTaskGraphSnapshot(
    IReadOnlyList<string> RootTaskIds,
    IReadOnlyList<MessageTaskGraphNode> Nodes,
    IReadOnlyList<MessageTaskGraphEdge> Edges,
    string? SourceConversationId,
    string? SourceTurnId,
    string? SourceUserMessageId);

public sealed record MessageTaskRun(
    string Id,
    string TaskId,
    string? Status,
    string? ModelPhaseStatus,
    DateTimeOffset? StartedAt,
    DateTimeOffset? FinishedAt,
    string? ResultSummary,
    string? ReportContent,
    string? ErrorMessage);

public sealed record MessageTaskRunEvent(
    string Id,
    string EventType,
    string? Message,
    DateTimeOffset? CreatedAt);

public sealed record MessageTaskRunDetail(
    MessageTask Task,
    MessageTaskRun Run,
    IReadOnlyList<MessageTaskRunEvent> Events,
    int EventsTotal,
    bool EventsHasMore);
