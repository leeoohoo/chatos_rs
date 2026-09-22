import ChatOSConnector
import ChatOSCore
import Combine
import Foundation

struct AgentSchedulerIssue: Equatable {
    let deliveryID: String
    let outcome: LocalAgentGroupChatScheduler.DeliveryAttemptOutcome
    let message: String

    init(_ receipt: LocalAgentGroupChatScheduler.DeliveryAttemptReceipt) {
        deliveryID = receipt.deliveryID
        outcome = receipt.outcome
        let fallback = switch receipt.outcome {
        case .failed: "本地 Agent 运行失败。"
        case .suspended: "本地 Agent 已暂停，运行检查点已保存。"
        case .completed: ""
        }
        message = receipt.detail ?? fallback
    }
}

enum AgentSchedulerIssueReducer {
    /// Account draining is intentionally global so Relay deliveries continue in conversations
    /// that are not visible. Presentation is local: only results for the visible room may change
    /// its issue, and a later completion clears the issue for that same delivery.
    static func reconcile(
        current: AgentSchedulerIssue?,
        receipts: [LocalAgentGroupChatScheduler.DeliveryAttemptReceipt],
        roomIDByDeliveryID: [String: String],
        roomID: String
    ) -> AgentSchedulerIssue? {
        var issues: [String: (order: Int, issue: AgentSchedulerIssue)] = [:]
        if let current {
            issues[current.deliveryID] = (-1, current)
        }
        for (order, receipt) in receipts.enumerated()
        where roomIDByDeliveryID[receipt.deliveryID] == roomID {
            switch receipt.outcome {
            case .completed:
                issues.removeValue(forKey: receipt.deliveryID)
            case .failed, .suspended:
                issues[receipt.deliveryID] = (order, AgentSchedulerIssue(receipt))
            }
        }
        return issues.values.max(by: { $0.order < $1.order })?.issue
    }
}

@MainActor
final class AgentGroupChatViewModel: ObservableObject {
    struct MemberPresentation: Identifiable {
        var id: String { member.agentID }
        let member: ProjectAgentRoomMember
        let profile: LocalAgentProfile?
    }

    struct InterruptedRunPresentation: Identifiable {
        var id: String { delivery.id }
        let run: LocalAgentGroupChatRun
        let delivery: ProjectAgentDelivery
        let agentName: String

        var statusText: String {
            switch run.checkpoint.status {
            case .ready, .running: "运行被中断"
            case .paused: "已暂停"
            case .needsReview: "需要检查副作用"
            case .limitReached: "达到运行限制"
            case .completed: "已完成"
            case .failed: "失败"
            }
        }
    }

    let projectID: String
    let ownerUserID: String

    @Published var room: ProjectAgentRoom?
    @Published var agents: [LocalAgentProfile] = []
    @Published var members: [ProjectAgentRoomMember] = []
    @Published var messages: [ProjectAgentMessage] = []
    @Published var availableModels: [LocalAgentBuilderModelOption] = []
    @Published var interruptedRuns: [InterruptedRunPresentation] = []
    @Published var pendingProposals: [LocalAgentCreationProposal] = []
    @Published var pendingRemovalProposals: [LocalAgentRemovalProposal] = []
    @Published var pendingTeamProposals: [LocalAgentTeamCreationProposal] = []
    @Published var pendingMembershipProposals: [LocalAgentMembershipProposal] = []
    @Published var teams: [ProjectAgentRoom] = []
    @Published var teamTodos: [LocalAgentTodo] = []
    @Published var teamAssets: [LocalAgentTeamAsset] = []
    @Published var requirementSurveys: [LocalAgentRequirementSurvey] = []
    @Published var submittingRequirementSurveyIDs: Set<String> = []
    @Published var teamAssetRevisions: [String: [LocalAgentTeamAssetRevision]] = [:]
    @Published var loadingTeamAssetRevisionIDs: Set<String> = []
    @Published var recentRuns: [LocalAgentGroupChatRun] = []
    @Published var recentRunDeliveries: [UUID: ProjectAgentDelivery] = [:]
    @Published var draftMessage = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published var attachmentDataByID: [String: Data] = [:]
    @Published var selectedMentionAgentIDs: Set<String> = []
    @Published var isLoading = false
    @Published var isLoadingModels = false
    @Published var isLoadingOlderMessages = false
    @Published var hasOlderMessages = false
    @Published var isSending = false
    @Published var isRunningAgents = false
    @Published var isPausingAgents = false
    @Published var isStoppingAgents = false
    @Published var runActionDeliveryIDs: Set<String> = []
    @Published var proposalActionIDs: Set<String> = []
    @Published var removalProposalActionIDs: Set<String> = []
    @Published var teamProposalActionIDs: Set<String> = []
    @Published var membershipProposalActionIDs: Set<String> = []
    @Published var errorMessage: String?
    @Published private(set) var schedulerIssue: AgentSchedulerIssue?
    @Published var scrollToLatestRequest = 0

    let service: NativeAgentGroupChatService
    let scheduler: LocalAgentGroupChatScheduler
    let builderService: LocalAgentBuilderService
    let projectsService: NativeLocalProjectsService
    var openedStore: SQLiteAgentGroupChatStore?
    var schedulerTask: Task<Void, Never>?
    var schedulerNeedsAnotherPass = false
    var changeObservationTask: Task<Void, Never>?
    var supplementaryLoadTask: Task<Void, Never>?
    var modelLoadTask: Task<LocalAgentBuilderResources, Error>?
    var hasLoadedModels = false
    let messagePageSize = 50

    init(
        projectID: String,
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        projectsService: NativeLocalProjectsService
    ) {
        self.projectID = projectID
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
        self.builderService = builderService
        self.projectsService = projectsService
    }

    deinit {
        changeObservationTask?.cancel()
        supplementaryLoadTask?.cancel()
    }

    var profilesByID: [String: LocalAgentProfile] {
        Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
    }

    var activeMembers: [MemberPresentation] {
        let profiles = profilesByID
        return members.map { MemberPresentation(member: $0, profile: profiles[$0.agentID]) }
    }

    var teamsByID: [String: ProjectAgentRoom] {
        Dictionary(uniqueKeysWithValues: teams.map { ($0.id, $0) })
    }

    var presentedErrorMessage: String? {
        errorMessage ?? schedulerIssue?.message
    }

    func dismissPresentedError() {
        if errorMessage != nil {
            errorMessage = nil
        } else {
            schedulerIssue = nil
        }
    }

    func reconcileSchedulerResults(
        _ receipts: [LocalAgentGroupChatScheduler.DeliveryAttemptReceipt],
        roomID: String
    ) async throws {
        let store = try await resolveStore()
        let deliveries = try await store.deliveries(
            ownerUserID: ownerUserID,
            deliveryIDs: receipts.map(\.deliveryID)
        )
        schedulerIssue = AgentSchedulerIssueReducer.reconcile(
            current: schedulerIssue,
            receipts: receipts,
            roomIDByDeliveryID: deliveries.mapValues(\.roomID),
            roomID: roomID
        )
    }

    func displayName(senderID: String, kind: ProjectAgentMessageSenderKind) -> String {
        switch kind {
        case .human: "你"
        case .system: "系统"
        case .agent: profilesByID[senderID]?.draft.name ?? senderID
        }
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let store = try await resolveStore()
            let agents = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
            let teams = try await store.listRooms(ownerUserID: ownerUserID, includeArchived: false)
            let room = try await store.activeRoom(ownerUserID: ownerUserID, projectID: projectID)
            let members: [ProjectAgentRoomMember]
            let messagePage: ProjectAgentMessagePage?
            if let room {
                members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
                messagePage = try await store.pageRecentMessages(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    beforeMessageID: nil,
                    limit: messagePageSize
                )
            } else {
                members = []
                messagePage = nil
            }
            self.agents = agents
            let isSameRoom = self.room?.id == room?.id
            self.room = room
            self.members = members
            try await reconcilePersistedSchedulerIssue(store: store, roomID: room?.id)
            let messages = messagePage?.messages ?? []
            if isSameRoom, !self.messages.isEmpty {
                self.messages = mergeMessages(self.messages, with: messages)
            } else {
                self.messages = messages
                hasOlderMessages = messagePage?.hasMore ?? false
            }
            if room == nil {
                attachmentDataByID = [:]
                hasOlderMessages = false
            }
            self.teams = teams
            selectedMentionAgentIDs.formIntersection(Set(members.map(\.agentID)))
            // Background room updates must not dismiss an action error. The alert owner clears
            // it explicitly after the Human acknowledges it, while successful user actions can
            // still clear their own stale error state.
            startSupplementaryLoad(
                store: store,
                room: room,
                agents: agents,
                messages: messages,
                mergeAttachments: isSameRoom
            )
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func reconcilePersistedSchedulerIssue(
        store: SQLiteAgentGroupChatStore,
        roomID: String?
    ) async throws {
        guard let issue = schedulerIssue else { return }
        guard let delivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: issue.deliveryID
        ), delivery.roomID == roomID else {
            schedulerIssue = nil
            return
        }
        guard let run = try await store.run(
                ownerUserID: ownerUserID,
                deliveryID: issue.deliveryID
              ), run.checkpoint.status == .completed else { return }
        schedulerIssue = nil
    }

    private func startSupplementaryLoad(
        store: SQLiteAgentGroupChatStore,
        room: ProjectAgentRoom?,
        agents: [LocalAgentProfile],
        messages: [ProjectAgentMessage],
        mergeAttachments: Bool
    ) {
        supplementaryLoadTask?.cancel()
        guard let room else {
            interruptedRuns = []
            pendingProposals = []
            pendingRemovalProposals = []
            pendingTeamProposals = []
            pendingMembershipProposals = []
            teamTodos = []
            teamAssets = []
            requirementSurveys = []
            recentRuns = []
            recentRunDeliveries = [:]
            return
        }

        supplementaryLoadTask = Task { [weak self] in
            guard let self else { return }
            do {
                let loadedAttachmentData = try await loadAttachmentData(
                    messages: messages,
                    roomID: room.id,
                    store: store
                )
                let pendingProposals = try await store.listAgentProposals(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    status: .pending
                )
                let pendingRemovalProposals = try await store.listAgentRemovalProposals(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    status: .pending
                )
                let pendingTeamProposals = try await store.listTeamProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: room.id,
                    status: .pending
                )
                let pendingMembershipProposals = try await store.listMembershipProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: room.id,
                    status: .pending
                )
                let teamTodos = try await store.listTeamTodos(
                    ownerUserID: ownerUserID,
                    teamRoomID: room.id,
                    includeTerminal: true
                )
                let teamAssets = try await store.listTeamAssets(
                    ownerUserID: ownerUserID,
                    teamRoomID: room.id,
                    includeArchived: false
                )
                let requirementSurveys = try await store.listRequirementSurveys(
                    ownerUserID: ownerUserID,
                    projectID: projectID,
                    status: nil
                )
                let unfinishedRuns = try await store.listUnfinishedRuns(
                    ownerUserID: ownerUserID,
                    projectID: projectID,
                    limit: 100
                )
                let unfinishedDeliveries = try await store.deliveries(
                    ownerUserID: ownerUserID,
                    deliveryIDs: unfinishedRuns.map(\.context.deliveryID)
                )
                let profileNames = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0.draft.name) })
                var interruptedRuns: [InterruptedRunPresentation] = []
                for run in unfinishedRuns where run.checkpoint.status != .completed {
                    guard let delivery = unfinishedDeliveries[run.context.deliveryID],
                          delivery.status == .running else { continue }
                    interruptedRuns.append(.init(
                        run: run,
                        delivery: delivery,
                        agentName: profileNames[run.context.agentID] ?? run.context.agentID
                    ))
                }
                let recentRuns = try await store.listRoomRuns(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    limit: 500
                )
                let deliveriesByID = try await store.deliveries(
                    ownerUserID: ownerUserID,
                    deliveryIDs: recentRuns.map(\.context.deliveryID)
                )
                let recentRunDeliveries: [UUID: ProjectAgentDelivery] = Dictionary(
                    uniqueKeysWithValues: recentRuns.compactMap { run -> (UUID, ProjectAgentDelivery)? in
                        guard let delivery = deliveriesByID[run.context.deliveryID] else { return nil }
                        return (run.id, delivery)
                    }
                )
                guard !Task.isCancelled, self.room?.id == room.id else { return }
                if mergeAttachments {
                    attachmentDataByID.merge(loadedAttachmentData) { _, new in new }
                } else {
                    attachmentDataByID = loadedAttachmentData
                }
                self.interruptedRuns = interruptedRuns
                self.pendingProposals = pendingProposals
                self.pendingRemovalProposals = pendingRemovalProposals
                self.pendingTeamProposals = pendingTeamProposals
                self.pendingMembershipProposals = pendingMembershipProposals
                self.teamTodos = teamTodos
                self.teamAssets = teamAssets
                self.requirementSurveys = requirementSurveys
                let activeAssetIDs = Set(teamAssets.map(\.id))
                teamAssetRevisions = teamAssetRevisions.filter { activeAssetIDs.contains($0.key) }
                loadingTeamAssetRevisionIDs.formIntersection(activeAssetIDs)
                self.recentRuns = recentRuns
                self.recentRunDeliveries = recentRunDeliveries
            } catch {
                guard !Task.isCancelled, self.room?.id == room.id else { return }
                errorMessage = error.localizedDescription
            }
        }
    }

}
