namespace ChatOS.Core.Domain;

public sealed record ProjectExecutionIdentity(
    string ProjectId,
    string RequirementId,
    string ExecutionGroupId,
    string ConversationId,
    string? ContactId);

public sealed record ProjectExecutionActionResult(
    bool Success,
    string? Status,
    string? ExecutionGroupId,
    IReadOnlyList<string> TaskIds,
    IReadOnlyList<string> RootTaskIds,
    bool? DiscardedTasks);
