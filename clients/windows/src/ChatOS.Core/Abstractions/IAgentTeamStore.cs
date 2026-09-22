using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IAgentTeamStore
{
    Task<IReadOnlyList<AgentProfile>> ListAgentsAsync(
        string ownerUserId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default);

    Task<AgentProfile?> GetAgentAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default);

    Task<AgentProfile> CreateAgentAsync(
        string ownerUserId,
        AgentProfileDraft draft,
        CancellationToken cancellationToken = default);

    Task<AgentProfile> UpdateAgentAsync(
        string ownerUserId,
        string agentId,
        AgentProfileDraft draft,
        AgentProfileStatus status,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentRoom>> ListRoomsAsync(
        string ownerUserId,
        string? projectId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default);

    Task<AgentRoom?> GetRoomAsync(
        string ownerUserId,
        string roomId,
        CancellationToken cancellationToken = default);

    Task<AgentRoom> CreateRoomAsync(
        string ownerUserId,
        string projectId,
        AgentRoomDraft draft,
        string? projectManagerAgentId,
        CancellationToken cancellationToken = default);

    Task<AgentRoom> OpenHumanAgentDirectAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default);

    Task<AgentRoom> OpenAgentDirectAsync(
        string ownerUserId,
        string sourceAgentId,
        string targetAgentId,
        CancellationToken cancellationToken = default);

    Task<AgentRoom> UpdateRoomAsync(
        string ownerUserId,
        string roomId,
        AgentRoomDraft draft,
        string? defaultAgentId,
        string? projectManagerAgentId,
        AgentRoomStatus status,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentRoomMember>> ListMembersAsync(
        string ownerUserId,
        string roomId,
        bool includeRemoved = false,
        CancellationToken cancellationToken = default);

    Task<AgentRoomMember> UpsertMemberAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        AgentRoomMemberDraft draft,
        AgentMemberStatus status = AgentMemberStatus.Active,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentMessage>> ListMessagesAsync(
        string ownerUserId,
        string roomId,
        int limit = 200,
        bool includeAttachmentPayloads = false,
        CancellationToken cancellationToken = default);

    Task<AgentMessage?> GetMessageAsync(
        string ownerUserId,
        string roomId,
        string messageId,
        CancellationToken cancellationToken = default,
        bool includeAttachmentPayloads = false);

    Task<AgentMessageAttachment?> GetMessageAttachmentAsync(
        string ownerUserId,
        string roomId,
        string attachmentId,
        CancellationToken cancellationToken = default);

    Task<AgentMessageAttachment?> GetMessageAttachmentForMessageAsync(
        string ownerUserId,
        string roomId,
        string messageId,
        string attachmentId,
        CancellationToken cancellationToken = default);

    Task<AgentPostResult> PostMessageAsync(
        string ownerUserId,
        string roomId,
        AgentMessageDraft draft,
        CancellationToken cancellationToken = default);

    Task MarkReadAsync(
        string ownerUserId,
        string roomId,
        string readerId,
        string throughMessageId,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentMessage>> ListUnreadMessagesAsync(
        string ownerUserId,
        string roomId,
        string agentId,
        int limit = 50,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentUnreadConversation>> ReadAllUnreadMessagesAndMarkReadAsync(
        string ownerUserId,
        string agentId,
        int limit = 200,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTodo>> ListTodosAsync(
        string ownerUserId,
        string roomId,
        bool includeTerminal = true,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTodo>> ListProjectTodosAsync(
        string ownerUserId,
        string projectId,
        bool includeTerminal = true,
        CancellationToken cancellationToken = default);

    Task<AgentTodo?> GetTodoAsync(
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken = default);

    Task<AgentTodoScheduleState> GetTodoScheduleStateAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default);

    Task<AgentDelivery?> StartNextReadyTodoAsync(
        string ownerUserId,
        string agentId,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTodoAssetSnapshot>> ListTodoAssetSnapshotsAsync(
        string ownerUserId,
        string todoId,
        CancellationToken cancellationToken = default);

    Task<AgentTodo> CreateTodoAsync(
        string ownerUserId,
        AgentTodoDraft draft,
        CancellationToken cancellationToken = default);

    Task<AgentTodo> UpdateTodoAsync(
        string ownerUserId,
        string todoId,
        long expectedRevision,
        AgentTodoStatus status,
        string result,
        string? assignedAgentId = null,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTodo>> ReorderTodosAsync(
        string ownerUserId,
        string roomId,
        IReadOnlyList<string> todoIds,
        CancellationToken cancellationToken = default);

    Task<AgentTodoProgress> AppendTodoProgressAsync(
        string ownerUserId,
        string todoId,
        string agentId,
        AgentTodoProgressKind kind,
        string stage,
        string detail,
        IReadOnlyList<AgentTeamAssetUpdateSuggestion>? assetUpdateSuggestions = null,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTodoProgress>> ListTodoProgressAsync(
        string ownerUserId,
        string todoId,
        int limit = 200,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTeamAsset>> ListAssetsAsync(
        string ownerUserId,
        string roomId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default);

    Task<AgentTeamAsset> UpsertAssetAsync(
        string ownerUserId,
        string roomId,
        string? assetId,
        string? editorAgentId,
        AgentTeamAssetCategory category,
        string title,
        string markdown,
        int? expectedRevision,
        CancellationToken cancellationToken = default);

    Task<AgentTeamAsset> ArchiveAssetAsync(
        string ownerUserId,
        string roomId,
        string assetId,
        string? editorAgentId,
        int expectedRevision,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentTeamAssetRevision>> ListAssetRevisionsAsync(
        string ownerUserId,
        string assetId,
        int limit = 100,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementSurvey> CreateRequirementSurveyAsync(
        string ownerUserId,
        string teamRoomId,
        string creatorAgentId,
        string sourceDeliveryId,
        string requestKey,
        AgentRequirementSurveyDraft draft,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentRequirementSurvey>> ListRequirementSurveysAsync(
        string ownerUserId,
        string projectId,
        AgentRequirementSurveyStatus? status = null,
        int limit = 200,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementSurvey?> GetRequirementSurveyAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementSurvey> SubmitRequirementSurveyAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        AgentRequirementSubmission submission,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementSurvey> ResolveRequirementSurveyAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        string resolverAgentId,
        AgentRequirementResolution resolution,
        CancellationToken cancellationToken = default);

    Task<AgentStaffingProposal> CreateStaffingProposalAsync(
        string ownerUserId,
        string sourceRoomId,
        string proposerAgentId,
        string sourceDeliveryId,
        string requestKey,
        AgentStaffingProposalDraft draft,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentStaffingProposal>> ListStaffingProposalsAsync(
        string ownerUserId,
        string sourceRoomId,
        AgentStaffingProposalStatus? status = null,
        CancellationToken cancellationToken = default);

    Task<AgentStaffingProposal> ResolveStaffingProposalAsync(
        string ownerUserId,
        string sourceRoomId,
        string proposalId,
        bool approve,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<string>> ListOwnersWithPendingDeliveriesAsync(
        CancellationToken cancellationToken = default);

    Task<AgentDelivery?> ClaimNextDeliveryAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default);

    Task<AgentDelivery?> ClaimNextDeliveryAsync(
        string ownerUserId,
        AgentDeliveryLane lane,
        CancellationToken cancellationToken = default);

    Task<AgentDelivery> CompleteDeliveryAsync(
        string ownerUserId,
        string deliveryId,
        string? responseMessageId,
        CancellationToken cancellationToken = default);

    Task<AgentDelivery> FailDeliveryAsync(
        string ownerUserId,
        string deliveryId,
        string error,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentDelivery>> EnqueueDueHeartbeatsAsync(
        long nowUnixMs,
        CancellationToken cancellationToken = default);

    Task<AgentRunSummary> SaveRunAsync(
        AgentRunSummary run,
        CancellationToken cancellationToken = default);

    Task<AgentRunSummary?> GetRunForDeliveryAsync(
        string ownerUserId,
        string deliveryId,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<AgentRunSummary>> ListRunsAsync(
        string ownerUserId,
        string roomId,
        int limit = 100,
        CancellationToken cancellationToken = default);
}
