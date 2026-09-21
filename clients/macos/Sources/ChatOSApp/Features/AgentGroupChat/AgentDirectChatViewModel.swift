import ChatOSConnector
import ChatOSCore
import SwiftUI

enum AgentDirectTimelineItem: Identifiable {
    case message(ProjectAgentMessage)
    case agentProposal(LocalAgentCreationProposal)
    case teamProposal(LocalAgentTeamCreationProposal)
    case membershipProposal(LocalAgentMembershipProposal)

    var id: String {
        switch self {
        case let .message(value): "message:\(value.id)"
        case let .agentProposal(value): "agent-proposal:\(value.id)"
        case let .teamProposal(value): "team-proposal:\(value.id)"
        case let .membershipProposal(value): "membership-proposal:\(value.id)"
        }
    }

    var createdAtUnixMs: Int64 {
        switch self {
        case let .message(value): value.createdAtUnixMs
        case let .agentProposal(value): value.createdAtUnixMs
        case let .teamProposal(value): value.createdAtUnixMs
        case let .membershipProposal(value): value.createdAtUnixMs
        }
    }
}

@MainActor
final class AgentDirectChatViewModel: ObservableObject {
    @Published private(set) var conversation: ProjectAgentRoom?
    @Published private(set) var agents: [LocalAgentProfile] = []
    @Published private(set) var messages: [ProjectAgentMessage] = []
    @Published private(set) var pendingAgentProposals: [LocalAgentCreationProposal] = []
    @Published private(set) var pendingTeamProposals: [LocalAgentTeamCreationProposal] = []
    @Published private(set) var pendingMembershipProposals: [LocalAgentMembershipProposal] = []
    @Published private(set) var teams: [ProjectAgentRoom] = []
    @Published var draftMessage = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published private(set) var attachmentDataByID: [String: Data] = [:]
    @Published private(set) var isLoading = false
    @Published private(set) var hasCompletedInitialLoad = false
    @Published private(set) var isLoadingOlderMessages = false
    @Published private(set) var hasOlderMessages = false
    @Published private(set) var isSending = false
    @Published private(set) var isRunningAgents = false
    @Published private(set) var isInitialTimelineReady = false
    @Published private(set) var proposalActionIDs: Set<String> = []
    @Published var errorMessage: String?
    @Published private(set) var schedulerIssue: AgentSchedulerIssue?
    @Published private(set) var scrollToLatestRequest = 0

    private let ownerUserID: String
    private let conversationID: String
    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private let projectsService: NativeLocalProjectsService
    private var openedStore: SQLiteAgentGroupChatStore?
    private var schedulerTask: Task<Void, Never>?
    private var schedulerNeedsAnotherPass = false
    private var changeObservationTask: Task<Void, Never>?
    private var supplementaryLoadTask: Task<Void, Never>?
    private let messagePageSize = 20

    init(
        ownerUserID: String,
        conversationID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        projectsService: NativeLocalProjectsService
    ) {
        self.ownerUserID = ownerUserID
        self.conversationID = conversationID
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

    var teamsByID: [String: ProjectAgentRoom] {
        Dictionary(uniqueKeysWithValues: teams.map { ($0.id, $0) })
    }

    var title: String { conversation?.draft.name ?? "私聊" }

    var isHumanDirect: Bool { conversation?.conversationKind == .humanAgentDirect }

    var presentedErrorMessage: String? {
        errorMessage ?? schedulerIssue?.message
    }

    var timelineItems: [AgentDirectTimelineItem] {
        let items = messages.map(AgentDirectTimelineItem.message)
            + pendingAgentProposals.map(AgentDirectTimelineItem.agentProposal)
            + pendingTeamProposals.map(AgentDirectTimelineItem.teamProposal)
            + pendingMembershipProposals.map(AgentDirectTimelineItem.membershipProposal)
        return items.sorted {
            ($0.createdAtUnixMs, $0.id) < ($1.createdAtUnixMs, $1.id)
        }
    }

    func displayName(for message: ProjectAgentMessage) -> String {
        switch message.senderKind {
        case .human: "你"
        case .system: "系统"
        case .agent: profilesByID[message.senderID]?.draft.name ?? "Agent"
        }
    }

    func activate() async {
        startChangeObservation()
        await load()
        startScheduler()
    }

    private func startChangeObservation() {
        guard changeObservationTask == nil else { return }
        let service = service
        let ownerUserID = ownerUserID
        let conversationID = conversationID
        changeObservationTask = Task { [weak self] in
            let changes = await service.changes(
                ownerUserID: ownerUserID,
                roomID: conversationID
            )
            let refreshCoalescer = AgentChangeRefreshCoalescer { [weak self] in
                await self?.load()
            }
            defer { refreshCoalescer.cancel() }
            for await _ in changes {
                guard !Task.isCancelled else { break }
                refreshCoalescer.signal()
            }
        }
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        do {
            let store = try await resolveStore()
            async let loadedConversation = store.room(
                ownerUserID: ownerUserID,
                roomID: conversationID
            )
            async let loadedAgents = store.listAgents(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            async let loadedMessages = store.pageRecentMessages(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                beforeMessageID: nil,
                limit: messagePageSize
            )
            guard let conversation = try await loadedConversation,
                  conversation.conversationKind.isDirect else {
                throw AgentGroupChatError.notFound
            }
            let messagePage = try await loadedMessages
            self.conversation = conversation
            agents = try await loadedAgents
            try await reconcilePersistedSchedulerIssue(store: store)
            let wasEmpty = messages.isEmpty
            messages = mergeMessages(messages, with: messagePage.messages)
            if wasEmpty {
                hasOlderMessages = messagePage.hasMore
            }
            isLoading = false
            hasCompletedInitialLoad = true
            startSupplementaryLoad(
                conversation: conversation,
                messages: messagePage.messages,
                store: store
            )
        } catch {
            isLoading = false
            hasCompletedInitialLoad = true
            errorMessage = error.localizedDescription
        }
    }

    private func startSupplementaryLoad(
        conversation: ProjectAgentRoom,
        messages: [ProjectAgentMessage],
        store: SQLiteAgentGroupChatStore
    ) {
        supplementaryLoadTask?.cancel()
        let loadedAttachmentIDs = Set(attachmentDataByID.keys)
        supplementaryLoadTask = Task { [weak self] in
            guard let self else { return }
            do {
                let loadedAttachmentData = try await loadAttachmentData(
                    messages: messages,
                    excluding: loadedAttachmentIDs,
                    store: store
                )
                guard !Task.isCancelled else { return }
                attachmentDataByID.merge(loadedAttachmentData) { _, new in new }

                guard conversation.conversationKind == .humanAgentDirect else {
                    pendingTeamProposals = []
                    pendingAgentProposals = []
                    pendingMembershipProposals = []
                    teams = []
                    isInitialTimelineReady = true
                    return
                }
                async let loadedProposals = store.listTeamProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: conversationID,
                    status: .pending
                )
                async let loadedAgentProposals = store.listAgentProposals(
                    ownerUserID: ownerUserID,
                    roomID: conversationID,
                    status: .pending
                )
                async let loadedMembershipProposals = store.listMembershipProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: conversationID,
                    status: .pending
                )
                async let loadedTeams = store.listRooms(
                    ownerUserID: ownerUserID,
                    includeArchived: false
                )
                let teamProposals = try await loadedProposals
                let agentProposals = try await loadedAgentProposals
                let membershipProposals = try await loadedMembershipProposals
                let rooms = try await loadedTeams
                guard !Task.isCancelled else { return }
                pendingTeamProposals = teamProposals
                pendingAgentProposals = agentProposals
                pendingMembershipProposals = membershipProposals
                teams = rooms
                isInitialTimelineReady = true
            } catch {
                guard !Task.isCancelled else { return }
                errorMessage = error.localizedDescription
                // Messages are already available. A supplementary-card failure must not leave
                // the transcript waiting forever before it performs its initial positioning.
                isInitialTimelineReady = true
            }
        }
    }

    private func reconcilePersistedSchedulerIssue(
        store: SQLiteAgentGroupChatStore
    ) async throws {
        guard let issue = schedulerIssue else { return }
        guard let delivery = try await store.delivery(
            ownerUserID: ownerUserID,
            deliveryID: issue.deliveryID
        ), delivery.roomID == conversationID else {
            schedulerIssue = nil
            return
        }
        guard let run = try await store.run(
                ownerUserID: ownerUserID,
                deliveryID: issue.deliveryID
              ), run.checkpoint.status == .completed else { return }
        schedulerIssue = nil
    }

    private func reconcileSchedulerResults(
        _ receipts: [LocalAgentGroupChatScheduler.DeliveryAttemptReceipt]
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
            roomID: conversationID
        )
    }

    @discardableResult
    func loadOlderMessages() async -> String? {
        guard !isLoadingOlderMessages,
              hasOlderMessages,
              let firstMessageID = messages.first?.id else { return nil }
        isLoadingOlderMessages = true
        defer { isLoadingOlderMessages = false }
        do {
            let store = try await resolveStore()
            let page = try await store.pageRecentMessages(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                beforeMessageID: firstMessageID,
                limit: messagePageSize
            )
            messages = mergeMessages(messages, with: page.messages)
            hasOlderMessages = page.hasMore
            let loadedAttachmentData = try await loadAttachmentData(
                messages: page.messages,
                store: store
            )
            attachmentDataByID.merge(loadedAttachmentData) { _, new in new }
            return firstMessageID
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func sendMessage() async {
        guard isHumanDirect, !isSending else { return }
        let content = draftMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        let outgoingAttachments = attachments
        guard !content.isEmpty || !outgoingAttachments.isEmpty else { return }
        errorMessage = nil
        isSending = true
        defer { isSending = false }
        do {
            let store = try await resolveStore()
            let post = try await store.postMessage(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                draft: .init(
                    senderKind: .human,
                    senderID: ownerUserID,
                    content: content,
                    attachments: outgoingAttachments.map(ProjectAgentMessageAttachmentDraft.init)
                ),
                limits: .init()
            )
            draftMessage = ""
            attachments = []
            attachmentError = nil
            await load()
            scrollToLatestRequest &+= 1
            if !post.deliveries.isEmpty { startScheduler() }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveTeamProposal(
        _ proposal: LocalAgentTeamCreationProposal
    ) async -> WorkspaceProject? {
        guard proposalActionIDs.insert(proposal.id).inserted else { return nil }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let createdProject: WorkspaceProject?
            let resolvedProjectID: String
            if let existingProjectID = proposal.draft.existingProjectID {
                let registry = try await projectsService.registry()
                guard let project = try await registry.get(
                    ownerUserID: ownerUserID,
                    id: existingProjectID
                ), project.status == .active else {
                    throw ProjectRegistryError.notFound
                }
                createdProject = nil
                resolvedProjectID = project.id
            } else if let importedDraft = proposal.draft.importedProjectDraft,
                      let absolutePath = proposal.draft.importedProjectAbsolutePath {
                let project = try await projectsService.createFromExistingDirectory(
                    ownerUserID: ownerUserID,
                    draft: importedDraft,
                    absolutePath: absolutePath
                )
                createdProject = project
                resolvedProjectID = project.id
            } else if let newProjectName = proposal.draft.newProjectName {
                let project = try await projectsService.createInDefaultWorkspace(
                    ownerUserID: ownerUserID,
                    name: newProjectName,
                    description: proposal.draft.newProjectDescription,
                    projectTypeKey: proposal.draft.newProjectTypeKey
                        ?? LocalAgentSkillCatalog.legacyProjectTypeKey
                )
                createdProject = project
                resolvedProjectID = project.id
            } else {
                throw AgentGroupChatError.conflict
            }
            let store = try await resolveStore()
            _ = try await store.approveTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                resolvedProjectID: resolvedProjectID,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            await load()
            startScheduler()
            return createdProject
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func approveAgentProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            _ = try await builderService.approveProposal(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                proposal: proposal
            )
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectAgentProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectAgentProposal(
                ownerUserID: ownerUserID,
                roomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectTeamProposal(_ proposal: LocalAgentTeamCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.approveMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: conversationID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func resolveStore() async throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try await service.store()
        openedStore = store
        return store
    }

    private func loadAttachmentData(
        messages: [ProjectAgentMessage],
        excluding loadedAttachmentIDs: Set<String> = [],
        store: SQLiteAgentGroupChatStore
    ) async throws -> [String: Data] {
        var result: [String: Data] = [:]
        for message in messages {
            for attachment in message.attachmentItems
            where attachment.kind == .image && !loadedAttachmentIDs.contains(attachment.id) {
                guard let payload = try await store.messageAttachment(
                    ownerUserID: ownerUserID,
                    roomID: conversationID,
                    messageID: message.id,
                    attachmentID: attachment.id
                ) else { continue }
                result[attachment.id] = try Data(
                    contentsOf: payload.localFileURL,
                    options: [.mappedIfSafe]
                )
            }
        }
        return result
    }

    private func mergeMessages(
        _ current: [ProjectAgentMessage],
        with incoming: [ProjectAgentMessage]
    ) -> [ProjectAgentMessage] {
        var byID = Dictionary(uniqueKeysWithValues: current.map { ($0.id, $0) })
        for message in incoming {
            byID[message.id] = message
        }
        return byID.values.sorted {
            ($0.createdAtUnixMs, $0.id) < ($1.createdAtUnixMs, $1.id)
        }
    }

    private func startScheduler() {
        schedulerNeedsAnotherPass = true
        guard schedulerTask == nil else { return }
        isRunningAgents = true
        schedulerTask = Task { [weak self] in
            guard let self else { return }
            repeat {
                schedulerNeedsAnotherPass = false
                do {
                    let receipts = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                    try await reconcileSchedulerResults(receipts)
                } catch is CancellationError {
                    // Another visible surface may already own the account drain. Its durable
                    // changes will refresh this conversation; cancellation is not a room error.
                } catch {
                    errorMessage = error.localizedDescription
                }
                await load()
                NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            } while schedulerNeedsAnotherPass
            isRunningAgents = false
            schedulerTask = nil
        }
    }

    func dismissError() {
        if errorMessage != nil {
            errorMessage = nil
        } else {
            schedulerIssue = nil
        }
    }
}
