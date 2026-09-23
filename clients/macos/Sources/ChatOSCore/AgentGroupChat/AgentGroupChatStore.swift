import Foundation

public protocol AgentGroupChatStore: Sendable {
    func createAgent(ownerUserID: String, draft: LocalAgentProfileDraft) async throws -> LocalAgentProfile
    func listAgents(ownerUserID: String, includeArchived: Bool) async throws -> [LocalAgentProfile]
    func updateAgentProfile(
        ownerUserID: String,
        agentID: String,
        draft: LocalAgentProfileDraft
    ) async throws -> LocalAgentProfile
    func updateAgentMembership(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        profileDraft: LocalAgentProfileDraft,
        memberDraft: ProjectAgentRoomMemberDraft
    ) async throws -> LocalAgentMembershipUpdateResult
    func createAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentCreationProposal
    func listAgentProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentCreationProposalStatus?
    ) async throws -> [LocalAgentCreationProposal]
    func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentProposalApproval
    func rejectAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentCreationProposal
    func createAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRemovalProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRemovalProposal
    func listAgentRemovalProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentRemovalProposalStatus?
    ) async throws -> [LocalAgentRemovalProposal]
    func approveAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRemovalProposal
    func rejectAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRemovalProposal
    func createMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentMembershipProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentMembershipProposal
    func listMembershipProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentMembershipProposalStatus?
    ) async throws -> [LocalAgentMembershipProposal]
    func approveMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentMembershipProposalApproval
    func rejectMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentMembershipProposal
    func createTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentTeamCreationProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamCreationProposal
    func listTeamProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentTeamCreationProposalStatus?
    ) async throws -> [LocalAgentTeamCreationProposal]
    func approveTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        resolvedProjectID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamProposalApproval
    func rejectTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamCreationProposal
    func createProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalProjectCreationProposalDraft,
        nowUnixMs: Int64
    ) async throws -> LocalProjectCreationProposal
    func listProjectProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalProjectCreationProposalStatus?
    ) async throws -> [LocalProjectCreationProposal]
    func approveProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        createdProjectID: String,
        nowUnixMs: Int64
    ) async throws -> LocalProjectCreationProposal
    func rejectProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) async throws -> LocalProjectCreationProposal
    func createRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft
    ) async throws -> ProjectAgentRoom
    func createManagedRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft,
        projectManagerAgentID: String
    ) async throws -> ProjectAgentRoom
    func openHumanAgentDirect(
        ownerUserID: String,
        agentID: String
    ) async throws -> ProjectAgentRoom
    func openAgentDirect(
        ownerUserID: String,
        initiatingAgentID: String,
        targetAgentID: String
    ) async throws -> ProjectAgentRoom
    func room(ownerUserID: String, roomID: String) async throws -> ProjectAgentRoom?
    func activeRoom(ownerUserID: String, projectID: String) async throws -> ProjectAgentRoom?
    func listRooms(ownerUserID: String, includeArchived: Bool) async throws -> [ProjectAgentRoom]
    func listDirectConversations(
        ownerUserID: String,
        includeArchived: Bool
    ) async throws -> [ProjectAgentRoom]
    func addMember(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        draft: ProjectAgentRoomMemberDraft
    ) async throws -> ProjectAgentRoomMember
    func listMembers(ownerUserID: String, roomID: String) async throws -> [ProjectAgentRoomMember]
    func setDefaultAgent(ownerUserID: String, roomID: String, agentID: String) async throws -> ProjectAgentRoom
    func setProjectManager(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) async throws -> ProjectAgentRoom
    func postMessage(
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft,
        limits: AgentGroupChatRoutingLimits
    ) async throws -> AgentGroupChatPostResult
    func messageAttachment(
        ownerUserID: String,
        roomID: String,
        messageID: String,
        attachmentID: String
    ) async throws -> ProjectAgentMessageAttachmentPayload?
    func listMessages(
        ownerUserID: String,
        roomID: String,
        afterUnixMs: Int64?,
        limit: Int
    ) async throws -> [ProjectAgentMessage]
    func pageMessages(
        ownerUserID: String,
        roomID: String,
        afterMessageID: String?,
        limit: Int
    ) async throws -> ProjectAgentMessagePage
    /// Reads the newest page first and then walks backwards with a stable message cursor.
    /// Returned messages are always chronological inside each page.
    func pageRecentMessages(
        ownerUserID: String,
        roomID: String,
        beforeMessageID: String?,
        limit: Int
    ) async throws -> ProjectAgentMessagePage
    func listUnreadMessages(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        limit: Int
    ) async throws -> ProjectAgentUnreadPage
    func markMessagesRead(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        throughMessageID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentReadCursor
    func readAllUnreadMessagesAndMarkRead(
        ownerUserID: String,
        agentID: String,
        limit: Int,
        nowUnixMs: Int64
    ) async throws -> [LocalAgentUnreadConversation]
    func listAgentTodos(
        ownerUserID: String,
        agentID: String,
        includeTerminal: Bool
    ) async throws -> [LocalAgentTodo]
    func listTeamAssets(
        ownerUserID: String,
        teamRoomID: String,
        includeArchived: Bool
    ) async throws -> [LocalAgentTeamAsset]
    func teamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String
    ) async throws -> LocalAgentTeamAsset?
    func upsertTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String?,
        editorAgentID: String?,
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String,
        expectedRevision: Int?,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamAsset
    func archiveTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        editorAgentID: String?,
        expectedRevision: Int,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTeamAsset
    func listTeamAssetRevisions(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        limit: Int
    ) async throws -> [LocalAgentTeamAssetRevision]
    func createRequirementSurvey(
        ownerUserID: String,
        projectID: String,
        creatorAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRequirementSurveyDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRequirementSurvey
    func listRequirementSurveys(
        ownerUserID: String,
        projectID: String,
        status: LocalAgentRequirementSurveyStatus?
    ) async throws -> [LocalAgentRequirementSurvey]
    func requirementSurvey(
        ownerUserID: String,
        projectID: String,
        surveyID: String
    ) async throws -> LocalAgentRequirementSurvey?
    func submitRequirementSurvey(
        ownerUserID: String,
        projectID: String,
        surveyID: String,
        submission: LocalAgentRequirementSurveySubmission,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRequirementSurvey
    func resolveRequirementSurvey(
        ownerUserID: String,
        projectID: String,
        surveyID: String,
        resolverAgentID: String,
        resolution: LocalAgentRequirementSurveyResolution,
        nowUnixMs: Int64
    ) async throws -> LocalAgentRequirementSurvey
    func listTodoTeamAssetSnapshots(
        ownerUserID: String,
        todoID: String
    ) async throws -> [LocalAgentTodoTeamAssetSnapshot]
    func todoTeamAssetSnapshot(
        ownerUserID: String,
        todoID: String,
        assetID: String,
        revision: Int
    ) async throws -> LocalAgentTodoTeamAssetSnapshot?
    func listTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        includeTerminal: Bool
    ) async throws -> [LocalAgentTodo]
    func createAgentTodo(
        ownerUserID: String,
        agentID: String,
        requestKey: String,
        draft: LocalAgentTodoDraft,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTodo
    func updateAgentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        update: LocalAgentTodoUpdate,
        nowUnixMs: Int64
    ) async throws -> LocalAgentTodo
    func agentTodoRequiresHumanRetry(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) async throws -> Bool
    func reorderAgentTodos(
        ownerUserID: String,
        agentID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodo]
    func reorderTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodo]
    func listAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        limit: Int
    ) async throws -> [LocalAgentTodoProgress]
    func listAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) async throws -> [LocalAgentTodoSourceLink]
    func linkAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        sources: [LocalAgentTodoSourceDraft],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodoSourceLink]
    func listAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) async throws -> [LocalAgentTodoDependency]
    func setAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        dependencies: [LocalAgentTodoDependencyDraft],
        nowUnixMs: Int64
    ) async throws -> [LocalAgentTodoDependency]
    func appendAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        kind: LocalAgentTodoProgressKind,
        runID: String?,
        stage: String,
        detail: String,
        assetUpdateSuggestions: [LocalAgentTeamAssetUpdateSuggestion],
        nowUnixMs: Int64
    ) async throws -> LocalAgentTodoProgress
    func agentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) async throws -> LocalAgentTodo?
    func todoForDelivery(
        ownerUserID: String,
        deliveryID: String
    ) async throws -> LocalAgentTodo?
    func agentTodoScheduleState(
        ownerUserID: String,
        agentID: String
    ) async throws -> LocalAgentTodoScheduleState
    func startNextReadyAgentTodo(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery?
    func enqueueAgentTodoStatus(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        excludingAgentID: String?,
        nowUnixMs: Int64
    ) async throws -> [ProjectAgentDelivery]
    func enqueueAgentTodoReady(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery?
    func enqueueReadyDependentAgentTodos(
        ownerUserID: String,
        prerequisiteTodoID: String,
        nowUnixMs: Int64
    ) async throws -> [ProjectAgentDelivery]
    func message(
        ownerUserID: String,
        roomID: String,
        messageID: String
    ) async throws -> ProjectAgentMessage?
    func claimNextDelivery(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery?
    func delivery(
        ownerUserID: String,
        deliveryID: String
    ) async throws -> ProjectAgentDelivery?
    func completeDelivery(
        ownerUserID: String,
        deliveryID: String,
        responseMessageID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery
    func completeHeartbeatDelivery(
        ownerUserID: String,
        deliveryID: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery
    func failDelivery(
        ownerUserID: String,
        deliveryID: String,
        error: String,
        nowUnixMs: Int64
    ) async throws -> ProjectAgentDelivery
}
