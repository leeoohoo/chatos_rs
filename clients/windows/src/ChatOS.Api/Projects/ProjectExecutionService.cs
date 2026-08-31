using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Projects;

public sealed class ProjectExecutionService : IProjectExecutionService
{
    private readonly ChatOSApiClient _client;

    public ProjectExecutionService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<ProjectRequirementExecutionLaunch?> FetchExecutionAsync(
        ProjectExecutionIdentity identity,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<ProjectRequirementExecutionDto>(
            $"projects/{Path(identity.ProjectId)}/requirements/{Path(identity.RequirementId)}/execution-plan" +
            $"?conversation_id={Query(identity.ConversationId)}&execution_group_id={Query(identity.ExecutionGroupId)}",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomainOrNull(identity.ProjectId, identity.RequirementId);
    }

    public Task<ProjectExecutionActionResult> ConfirmExecutionAsync(
        ProjectExecutionIdentity identity,
        CancellationToken cancellationToken = default) =>
        MutateAsync(identity, "confirm-execution", null, cancellationToken);

    public Task<ProjectExecutionActionResult> StopExecutionAsync(
        ProjectExecutionIdentity identity,
        CancellationToken cancellationToken = default) =>
        MutateAsync(identity, "stop", true, cancellationToken);

    private async Task<ProjectExecutionActionResult> MutateAsync(
        ProjectExecutionIdentity identity,
        string action,
        bool? discardTasks,
        CancellationToken cancellationToken)
    {
        var response = await _client.PostAsync<ProjectExecutionActionResponseDto>(
            $"projects/{Path(identity.ProjectId)}/requirements/{Path(identity.RequirementId)}/{action}",
            new ProjectExecutionActionRequestDto(
                identity.ExecutionGroupId,
                identity.ConversationId,
                identity.ContactId,
                discardTasks),
            cancellationToken).ConfigureAwait(false);
        if (!response.Success)
        {
            throw new ChatOSApiException(string.IsNullOrWhiteSpace(response.Status)
                ? "执行计划操作未被接受。"
                : response.Status);
        }

        return response.ToDomain();
    }

    private static string Path(string value) => Uri.EscapeDataString(value);

    private static string Query(string value) => Uri.EscapeDataString(value);
}

internal sealed record ProjectExecutionActionRequestDto(
    [property: JsonPropertyName("execution_group_id")] string ExecutionGroupId,
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("contact_id")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? ContactId,
    [property: JsonPropertyName("discard_tasks")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] bool? DiscardTasks);

internal sealed record ProjectExecutionActionResponseDto
{
    [JsonPropertyName("success")]
    public bool Success { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("execution_group_id")]
    public string? ExecutionGroupId { get; init; }

    [JsonPropertyName("task_ids")]
    public IReadOnlyList<string> TaskIds { get; init; } = Array.Empty<string>();

    [JsonPropertyName("root_task_ids")]
    public IReadOnlyList<string> RootTaskIds { get; init; } = Array.Empty<string>();

    [JsonPropertyName("discarded_tasks")]
    public bool? DiscardedTasks { get; init; }

    public ProjectExecutionActionResult ToDomain() => new(
        Success,
        Status,
        ExecutionGroupId,
        TaskIds,
        RootTaskIds,
        DiscardedTasks);
}
