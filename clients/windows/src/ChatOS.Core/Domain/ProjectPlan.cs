namespace ChatOS.Core.Domain;

public sealed record ProjectRequirement(
    string Id,
    string? ProjectId,
    string? ParentRequirementId,
    string Type,
    string Title,
    string? Summary,
    string? Detail,
    string? BusinessValue,
    string? AcceptanceCriteria,
    int Priority,
    string Status,
    DateTimeOffset? UpdatedAt);

public sealed record ProjectWorkItem(
    string Id,
    string? RequirementId,
    string Title,
    string? Detail,
    string Status,
    int Priority,
    IReadOnlyList<string> Tags,
    bool IsPlanningTask,
    DateTimeOffset? DueAt);

public sealed record ProjectPlanEdge(
    string SourceId,
    string TargetId,
    string Kind);

public sealed record ProjectPlanCounts(
    int Total,
    int Open,
    int Done,
    int Blocked)
{
    public static ProjectPlanCounts FromWorkItems(IEnumerable<ProjectWorkItem> workItems)
    {
        var items = workItems.ToArray();
        var doneStatuses = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "done", "completed", "succeeded", "success",
        };
        var blockedStatuses = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "blocked", "failed",
        };
        var done = items.Count(item => doneStatuses.Contains(item.Status));
        var blocked = items.Count(item => blockedStatuses.Contains(item.Status));
        return new ProjectPlanCounts(items.Length, Math.Max(0, items.Length - done), done, blocked);
    }
}

public sealed record ProjectPlanSnapshot(
    string ProjectId,
    IReadOnlyList<ProjectRequirement> Requirements,
    IReadOnlyList<ProjectWorkItem> WorkItems,
    IReadOnlyList<ProjectPlanEdge> Edges,
    ProjectPlanCounts Counts);

public sealed record ProjectRequirementDocument(
    string Id,
    string Title,
    string Type,
    string Format,
    string Content,
    int Version,
    DateTimeOffset? UpdatedAt);

public sealed record ProjectRequirementExecutionLaunch(
    string ProjectId,
    string RequirementId,
    string ConversationId,
    string ExecutionGroupId,
    string? MessageId,
    string ConfirmationStatus,
    bool HasStartedRuns,
    string? OverallStatus,
    string? ContactId,
    int TaskCount,
    bool IncludePrerequisiteDependents,
    string? FailureKind,
    string? FailureReason,
    DateTimeOffset? CreatedAt);
