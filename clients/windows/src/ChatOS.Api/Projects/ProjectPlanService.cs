using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Projects;

public sealed class ProjectPlanService : IProjectPlanService
{
    private readonly ChatOSApiClient _client;

    public ProjectPlanService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<ProjectPlanSnapshot> FetchPlanAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<ProjectPlanDto>(
            $"projects/{Path(projectId)}/plan?include_archived=false&include_work_items=false",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain(projectId);
    }

    public async Task<ProjectPlanSnapshot> FetchWorkItemsAsync(
        string projectId,
        string requirementId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<RequirementWorkItemsDto>(
            $"projects/{Path(projectId)}/requirements/{Path(requirementId)}/work-items" +
            "?include_archived=false&include_dependency_graph=true",
            cancellationToken).ConfigureAwait(false);
        var items = response.WorkItems.Select(static value => value.ToDomain()).ToArray();
        return new ProjectPlanSnapshot(
            projectId,
            Array.Empty<ProjectRequirement>(),
            items,
            response.DependencyGraph?.Edges.Select(static value => value.ToDomain()).ToArray()
                ?? Array.Empty<ProjectPlanEdge>(),
            ProjectPlanCounts.FromWorkItems(items));
    }

    public async Task<IReadOnlyList<ProjectRequirementDocument>> FetchDocumentsAsync(
        string projectId,
        string requirementId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<IReadOnlyList<RequirementDocumentDto>>(
            $"projects/{Path(projectId)}/requirements/{Path(requirementId)}/documents",
            cancellationToken).ConfigureAwait(false);
        return response.Select(static value => value.ToDomain()).ToArray();
    }

    public async Task<ProjectRequirementExecutionLaunch?> FetchExecutionAsync(
        string projectId,
        string requirementId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<ProjectRequirementExecutionDto>(
            $"projects/{Path(projectId)}/requirements/{Path(requirementId)}/execution-plan",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomainOrNull(projectId, requirementId);
    }

    public async Task<ProjectRequirementExecutionLaunch> CreateExecutionAsync(
        string projectId,
        string requirementId,
        bool includePrerequisiteDependents,
        string? planningFeedback,
        CancellationToken cancellationToken = default)
    {
        planningFeedback = planningFeedback?.Trim();
        var response = await _client.PostAsync<ProjectRequirementExecutionDto>(
            $"projects/{Path(projectId)}/requirements/{Path(requirementId)}/execute",
            new CreateRequirementExecutionRequestDto(
                includePrerequisiteDependents,
                string.IsNullOrEmpty(planningFeedback) ? null : planningFeedback),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomainOrNull(projectId, requirementId)
            ?? throw new ChatOSApiException(
                "Execution plan response did not include conversation_id and execution_group_id.");
    }

    private static string Path(string value) => Uri.EscapeDataString(value);
}

internal sealed record ProjectPlanDto
{
    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("requirements")]
    public IReadOnlyList<ProjectRequirementDto> Requirements { get; init; } = Array.Empty<ProjectRequirementDto>();

    [JsonPropertyName("work_items")]
    public IReadOnlyList<ProjectWorkItemDto> WorkItems { get; init; } = Array.Empty<ProjectWorkItemDto>();

    [JsonPropertyName("work_item_counts")]
    public ProjectPlanCountsDto? WorkItemCounts { get; init; }

    [JsonPropertyName("dependency_graph")]
    public ProjectDependencyGraphDto? DependencyGraph { get; init; }

    public ProjectPlanSnapshot ToDomain(string fallbackProjectId)
    {
        var items = WorkItems.Select(static value => value.ToDomain()).ToArray();
        return new ProjectPlanSnapshot(
            ProjectId ?? fallbackProjectId,
            Requirements.Select(static value => value.ToDomain()).ToArray(),
            items,
            DependencyGraph?.Edges.Select(static value => value.ToDomain()).ToArray()
                ?? Array.Empty<ProjectPlanEdge>(),
            WorkItemCounts?.ToDomain() ?? ProjectPlanCounts.FromWorkItems(items));
    }
}

internal sealed record RequirementWorkItemsDto
{
    [JsonPropertyName("work_items")]
    public IReadOnlyList<ProjectWorkItemDto> WorkItems { get; init; } = Array.Empty<ProjectWorkItemDto>();

    [JsonPropertyName("dependency_graph")]
    public ProjectDependencyGraphDto? DependencyGraph { get; init; }
}

internal sealed record ProjectRequirementDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("parent_requirement_id")]
    public string? ParentRequirementId { get; init; }

    [JsonPropertyName("requirement_type")]
    public string? RequirementType { get; init; }

    [JsonPropertyName("title")]
    public required string Title { get; init; }

    [JsonPropertyName("summary")]
    public string? Summary { get; init; }

    [JsonPropertyName("detail")]
    public string? Detail { get; init; }

    [JsonPropertyName("business_value")]
    public string? BusinessValue { get; init; }

    [JsonPropertyName("acceptance_criteria")]
    public string? AcceptanceCriteria { get; init; }

    [JsonPropertyName("priority")]
    public int? Priority { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }

    public ProjectRequirement ToDomain() => new(
        Id,
        ProjectId,
        ParentRequirementId,
        RequirementType ?? "requirement",
        Title,
        Summary,
        Detail,
        BusinessValue,
        AcceptanceCriteria,
        Priority ?? 0,
        Status ?? "draft",
        ParseDate(UpdatedAt));

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var parsed) ? parsed : null;
}

internal sealed record ProjectWorkItemDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("requirement_id")]
    public string? RequirementId { get; init; }

    [JsonPropertyName("title")]
    public required string Title { get; init; }

    [JsonPropertyName("description")]
    public string? Description { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("priority")]
    public int? Priority { get; init; }

    [JsonPropertyName("tags")]
    public IReadOnlyList<string> Tags { get; init; } = Array.Empty<string>();

    [JsonPropertyName("is_planning_task")]
    public bool? IsPlanningTask { get; init; }

    [JsonPropertyName("due_at")]
    public string? DueAt { get; init; }

    public ProjectWorkItem ToDomain() => new(
        Id,
        RequirementId,
        Title,
        Description,
        Status ?? "todo",
        Priority ?? 0,
        Tags,
        IsPlanningTask ?? false,
        DateTimeOffset.TryParse(DueAt, out var parsed) ? parsed : null);
}

internal sealed record ProjectPlanCountsDto(
    [property: JsonPropertyName("total")] int? Total,
    [property: JsonPropertyName("open")] int? Open,
    [property: JsonPropertyName("done")] int? Done,
    [property: JsonPropertyName("blocked")] int? Blocked)
{
    public ProjectPlanCounts ToDomain() => new(Total ?? 0, Open ?? 0, Done ?? 0, Blocked ?? 0);
}

internal sealed record ProjectDependencyGraphDto
{
    [JsonPropertyName("edges")]
    public IReadOnlyList<ProjectPlanEdgeDto> Edges { get; init; } = Array.Empty<ProjectPlanEdgeDto>();
}

internal sealed record ProjectPlanEdgeDto(
    [property: JsonPropertyName("from")] string From,
    [property: JsonPropertyName("to")] string To,
    [property: JsonPropertyName("edge_type")] string? EdgeType)
{
    public ProjectPlanEdge ToDomain() => new(
        RemoveGraphPrefix(From),
        RemoveGraphPrefix(To),
        EdgeType ?? "depends_on");

    private static string RemoveGraphPrefix(string value)
    {
        var separator = value.IndexOf(':');
        return separator < 0 ? value : value[(separator + 1)..];
    }
}

internal sealed record RequirementDocumentDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("doc_type")]
    public string? DocumentType { get; init; }

    [JsonPropertyName("title")]
    public string? Title { get; init; }

    [JsonPropertyName("format")]
    public string? Format { get; init; }

    [JsonPropertyName("content")]
    public string? Content { get; init; }

    [JsonPropertyName("version")]
    public int? Version { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }

    public ProjectRequirementDocument ToDomain() => new(
        Id,
        Title ?? "未命名文档",
        DocumentType ?? "document",
        Format ?? "markdown",
        Content ?? string.Empty,
        Version ?? 1,
        DateTimeOffset.TryParse(UpdatedAt, out var parsed) ? parsed : null);
}

internal sealed record ProjectRequirementExecutionDto
{
    [JsonPropertyName("found")]
    public bool? Found { get; init; }

    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("requirement_id")]
    public string? RequirementId { get; init; }

    [JsonPropertyName("conversation_id")]
    public string? ConversationId { get; init; }

    [JsonPropertyName("execution_group_id")]
    public string? ExecutionGroupId { get; init; }

    [JsonPropertyName("message_id")]
    public string? MessageId { get; init; }

    [JsonPropertyName("contact_id")]
    public string? ContactId { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("confirmation_status")]
    public string? ConfirmationStatus { get; init; }

    [JsonPropertyName("has_started_runs")]
    public bool? HasStartedRuns { get; init; }

    [JsonPropertyName("task_count")]
    public int? TaskCount { get; init; }

    [JsonPropertyName("include_prerequisite_dependents")]
    public bool? IncludePrerequisiteDependents { get; init; }

    [JsonPropertyName("failure_kind")]
    public string? FailureKind { get; init; }

    [JsonPropertyName("failure_reason")]
    public string? FailureReason { get; init; }

    [JsonPropertyName("created_at")]
    public string? CreatedAt { get; init; }

    public ProjectRequirementExecutionLaunch? ToDomainOrNull(
        string fallbackProjectId,
        string fallbackRequirementId)
    {
        var conversationId = ConversationId?.Trim();
        var executionGroupId = ExecutionGroupId?.Trim();
        if (Found == false || string.IsNullOrEmpty(conversationId) || string.IsNullOrEmpty(executionGroupId))
        {
            return null;
        }

        return new ProjectRequirementExecutionLaunch(
            ProjectId ?? fallbackProjectId,
            RequirementId ?? fallbackRequirementId,
            conversationId,
            executionGroupId,
            MessageId,
            ConfirmationStatus ?? "pending",
            HasStartedRuns ?? false,
            Status,
            ContactId,
            TaskCount ?? 0,
            IncludePrerequisiteDependents ?? false,
            FailureKind,
            FailureReason,
            DateTimeOffset.TryParse(CreatedAt, out var parsed) ? parsed : null);
    }
}

internal sealed record CreateRequirementExecutionRequestDto(
    [property: JsonPropertyName("include_prerequisite_dependents")] bool IncludePrerequisiteDependents,
    [property: JsonPropertyName("planning_feedback")] string? PlanningFeedback);
