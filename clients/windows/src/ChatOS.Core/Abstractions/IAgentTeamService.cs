using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public sealed class AgentTeamChangedEventArgs(
    string ownerUserId,
    string? projectId,
    string? roomId,
    string kind) : EventArgs
{
    public string OwnerUserId { get; } = ownerUserId;
    public string? ProjectId { get; } = projectId;
    public string? RoomId { get; } = roomId;
    public string Kind { get; } = kind;
}

public interface IAgentTeamService
{
    event EventHandler<AgentTeamChangedEventArgs>? Changed;

    Task<IReadOnlyList<AgentProfile>> ListAgentsAsync(
        string ownerUserId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default);

    Task<AgentProfile> SaveAgentAsync(
        string ownerUserId,
        string? agentId,
        AgentProfileDraft draft,
        CancellationToken cancellationToken = default);

    Task ArchiveAgentAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentRoom>> ListRoomsAsync(
        string ownerUserId,
        string projectId,
        CancellationToken cancellationToken = default);

    Task<AgentRoom> CreateTeamAsync(
        string ownerUserId,
        string projectId,
        AgentRoomDraft draft,
        string projectManagerAgentId,
        CancellationToken cancellationToken = default);

    Task<AgentRoom> OpenDirectAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default);

    Task<AgentRoomMember> AddMemberAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        AgentRoomMemberDraft draft,
        CancellationToken cancellationToken = default);

    Task RemoveMemberAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        CancellationToken cancellationToken = default);

    Task<AgentRoom> ConfigureTeamAsync(
        string ownerUserId,
        string roomId,
        AgentRoomDraft draft,
        string? defaultAgentId,
        string projectManagerAgentId,
        CancellationToken cancellationToken = default);

    Task<AgentTeamSnapshot> LoadSnapshotAsync(
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken = default);

    Task<AgentPostResult> PostHumanMessageAsync(
        string ownerUserId,
        string roomId,
        string content,
        IReadOnlyList<string>? mentionedAgentIds = null,
        IReadOnlyList<AgentMessageAttachment>? attachments = null,
        CancellationToken cancellationToken = default);

    Task<AgentMessageAttachment?> GetMessageAttachmentAsync(
        string ownerUserId,
        string roomId,
        string attachmentId,
        CancellationToken cancellationToken = default);

    Task<AgentTodo> CreateTodoAsync(
        string ownerUserId,
        string managerAgentId,
        AgentTodoDraft draft,
        CancellationToken cancellationToken = default);

    Task<AgentTodo> UpdateTodoAsync(
        string ownerUserId,
        string actingAgentId,
        string todoId,
        long expectedRevision,
        AgentTodoStatus status,
        string result,
        string? assignedAgentId = null,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTodo>> ReorderTodosAsync(
        string ownerUserId,
        string managerAgentId,
        string roomId,
        IReadOnlyList<string> todoIds,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTodoProgress>> ListTodoProgressAsync(
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken = default);

    Task<AgentTeamAsset> SaveAssetAsync(
        string ownerUserId,
        string roomId,
        string? assetId,
        string? editorAgentId,
        AgentTeamAssetCategory category,
        string title,
        string markdown,
        int? expectedRevision,
        CancellationToken cancellationToken = default);

    Task ArchiveAssetAsync(
        string ownerUserId,
        string roomId,
        string assetId,
        string? editorAgentId,
        int expectedRevision,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementSurvey> SubmitRequirementSurveyAsync(
        string ownerUserId,
        string roomId,
        string surveyId,
        AgentRequirementSubmission submission,
        CancellationToken cancellationToken = default);

    Task DrainAsync(string ownerUserId, CancellationToken cancellationToken = default);
}
