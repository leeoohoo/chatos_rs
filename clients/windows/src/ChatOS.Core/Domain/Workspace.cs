namespace ChatOS.Core.Domain;

public sealed record WorkspaceProject(
    string Id,
    string Name,
    string? RootPath,
    string? DisplayRootPath,
    string? LatestConversationId);

public sealed record WorkspaceContact(
    string Id,
    string AgentId,
    string Name,
    string? Status);

public sealed record WorkspaceConversation(
    string Id,
    string Title,
    string? ProjectId,
    string? ContactId,
    string? ContactAgentId,
    int MessageCount,
    DateTimeOffset UpdatedAt,
    bool IsArchived);

public sealed record WorkspaceSnapshot(
    IReadOnlyList<WorkspaceProject> Projects,
    IReadOnlyList<WorkspaceContact> Contacts,
    IReadOnlyList<WorkspaceConversation> Conversations)
{
    public static WorkspaceSnapshot Empty { get; } = new(
        Array.Empty<WorkspaceProject>(),
        Array.Empty<WorkspaceContact>(),
        Array.Empty<WorkspaceConversation>());
}

public sealed record LocalProjectCreationDraft(
    string Name,
    string DeviceId,
    string WorkspaceId,
    string? RelativePath);
