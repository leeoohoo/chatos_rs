import ChatOSConnector
import ChatOSCore
import Combine
import Foundation

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

    @Published private(set) var room: ProjectAgentRoom?
    @Published private(set) var agents: [LocalAgentProfile] = []
    @Published private(set) var members: [ProjectAgentRoomMember] = []
    @Published private(set) var messages: [ProjectAgentMessage] = []
    @Published private(set) var availableModels: [LocalAgentBuilderModelOption] = []
    @Published private(set) var interruptedRuns: [InterruptedRunPresentation] = []
    @Published private(set) var pendingProposals: [LocalAgentCreationProposal] = []
    @Published private(set) var pendingRemovalProposals: [LocalAgentRemovalProposal] = []
    @Published private(set) var pendingTeamProposals: [LocalAgentTeamCreationProposal] = []
    @Published private(set) var pendingMembershipProposals: [LocalAgentMembershipProposal] = []
    @Published private(set) var teams: [ProjectAgentRoom] = []
    @Published var draftMessage = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published private(set) var attachmentDataByID: [String: Data] = [:]
    @Published var selectedMentionAgentIDs: Set<String> = []
    @Published private(set) var isLoading = false
    @Published private(set) var isSending = false
    @Published private(set) var isRunningAgents = false
    @Published private(set) var isPausingAgents = false
    @Published private(set) var isStoppingAgents = false
    @Published private(set) var runActionDeliveryIDs: Set<String> = []
    @Published private(set) var proposalActionIDs: Set<String> = []
    @Published private(set) var removalProposalActionIDs: Set<String> = []
    @Published private(set) var teamProposalActionIDs: Set<String> = []
    @Published private(set) var membershipProposalActionIDs: Set<String> = []
    @Published var errorMessage: String?

    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private let projectsService: NativeLocalProjectsService
    private var openedStore: SQLiteAgentGroupChatStore?
    private var schedulerTask: Task<Void, Never>?
    private var schedulerNeedsAnotherPass = false

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
            let messages: [ProjectAgentMessage]
            let interruptedRuns: [InterruptedRunPresentation]
            let pendingProposals: [LocalAgentCreationProposal]
            let pendingRemovalProposals: [LocalAgentRemovalProposal]
            let pendingTeamProposals: [LocalAgentTeamCreationProposal]
            let pendingMembershipProposals: [LocalAgentMembershipProposal]
            if let room {
                members = try await store.listMembers(ownerUserID: ownerUserID, roomID: room.id)
                messages = try await store.listMessages(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    afterUnixMs: nil,
                    limit: 500
                )
                let recentRuns = try await store.listUnfinishedRuns(
                    ownerUserID: ownerUserID,
                    projectID: projectID,
                    limit: 100
                )
                let profileNames = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0.draft.name) })
                var values: [InterruptedRunPresentation] = []
                for run in recentRuns where run.checkpoint.status != .completed {
                    guard let delivery = try await store.delivery(
                        ownerUserID: ownerUserID,
                        deliveryID: run.context.deliveryID
                    ), delivery.status == .running else { continue }
                    values.append(.init(
                        run: run,
                        delivery: delivery,
                        agentName: profileNames[run.context.agentID] ?? run.context.agentID
                    ))
                }
                interruptedRuns = values
                pendingProposals = try await store.listAgentProposals(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    status: .pending
                )
                pendingRemovalProposals = try await store.listAgentRemovalProposals(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    status: .pending
                )
                pendingTeamProposals = try await store.listTeamProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: room.id,
                    status: .pending
                )
                pendingMembershipProposals = try await store.listMembershipProposals(
                    ownerUserID: ownerUserID,
                    sourceRoomID: room.id,
                    status: .pending
                )
            } else {
                members = []
                messages = []
                interruptedRuns = []
                pendingProposals = []
                pendingRemovalProposals = []
                pendingTeamProposals = []
                pendingMembershipProposals = []
            }
            self.agents = agents
            self.room = room
            self.members = members
            self.messages = messages
            if let room {
                attachmentDataByID = try await loadAttachmentData(
                    messages: messages,
                    roomID: room.id,
                    store: store
                )
            } else {
                attachmentDataByID = [:]
            }
            self.interruptedRuns = interruptedRuns
            self.pendingProposals = pendingProposals
            self.pendingRemovalProposals = pendingRemovalProposals
            self.pendingTeamProposals = pendingTeamProposals
            self.pendingMembershipProposals = pendingMembershipProposals
            self.teams = teams
            let builderResources = try? await builderService.loadResources(
                ownerUserID: ownerUserID
            )
            self.availableModels = builderResources?.models ?? []
            selectedMentionAgentIDs.formIntersection(Set(members.map(\.agentID)))
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func activate() async {
        await load()
        if room != nil {
            startScheduler()
        }
    }

    func createRoom(
        name: String,
        goal: String,
        projectManagerAgentID: String
    ) async -> Bool {
        do {
            let store = try await resolveStore()
            _ = try await store.createManagedRoom(
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    goal: goal.trimmingCharacters(in: .whitespacesAndNewlines)
                ),
                projectManagerAgentID: projectManagerAgentID
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func createAgentAndJoin(
        name: String,
        role: String,
        responsibility: String,
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String?,
        professionKey: String
    ) async -> Bool {
        guard let room else {
            errorMessage = AgentGroupChatError.notFound.localizedDescription
            return false
        }
        let normalizedModelConfigID = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let selectedModel = availableModels.first(where: { $0.id == normalizedModelConfigID }) else {
            errorMessage = LocalAgentBuilderError.modelUnavailable.localizedDescription
            return false
        }
        let normalizedThinkingLevel = LocalAgentThinkingLevelCatalog.normalized(
            thinkingLevel,
            allowedValues: selectedModel.thinkingLevels
        )
        if let thinkingLevel = thinkingLevel?.trimmingCharacters(in: .whitespacesAndNewlines),
           !thinkingLevel.isEmpty, normalizedThinkingLevel == nil {
            errorMessage = AgentGroupChatError.invalidField("thinkingLevel").localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            let agent = try await store.createAgent(
                ownerUserID: ownerUserID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    description: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    rolePrompt: rolePrompt.trimmingCharacters(in: .whitespacesAndNewlines),
                    modelConfigID: normalizedModelConfigID,
                    thinkingLevel: normalizedThinkingLevel,
                    professionKey: professionKey,
                    defaultPluginIDs: []
                )
            )
            _ = try await store.addMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agent.id,
                draft: .init(
                    role: role.trimmingCharacters(in: .whitespacesAndNewlines),
                    responsibility: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    pluginAllowlist: []
                )
            )
            if members.isEmpty {
                _ = try await store.setDefaultAgent(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    agentID: agent.id
                )
            }
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func inviteAgent(agentID: String) async -> Bool {
        guard let room,
              let profile = profilesByID[agentID],
              !members.contains(where: { $0.agentID == agentID }) else {
            errorMessage = AgentGroupChatError.conflict.localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            _ = try await store.addMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agentID,
                draft: .init(
                    role: profile.draft.name,
                    responsibility: profile.draft.description,
                    pluginAllowlist: []
                )
            )
            if members.isEmpty {
                _ = try await store.setDefaultAgent(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    agentID: agentID
                )
            }
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func updateAgentMembership(
        agentID: String,
        name: String,
        role: String,
        responsibility: String,
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String?
    ) async -> Bool {
        guard let room, let existingProfile = profilesByID[agentID] else {
            errorMessage = AgentGroupChatError.notFound.localizedDescription
            return false
        }
        let normalizedModelConfigID = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let selectedModel = availableModels.first(where: { $0.id == normalizedModelConfigID }) else {
            errorMessage = LocalAgentBuilderError.modelUnavailable.localizedDescription
            return false
        }
        let normalizedThinkingLevel = LocalAgentThinkingLevelCatalog.normalized(
            thinkingLevel,
            allowedValues: selectedModel.thinkingLevels
        )
        if let thinkingLevel = thinkingLevel?.trimmingCharacters(in: .whitespacesAndNewlines),
           !thinkingLevel.isEmpty, normalizedThinkingLevel == nil {
            errorMessage = AgentGroupChatError.invalidField("thinkingLevel").localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            _ = try await store.updateAgentMembership(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agentID,
                profileDraft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    description: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    rolePrompt: rolePrompt.trimmingCharacters(in: .whitespacesAndNewlines),
                    modelConfigID: normalizedModelConfigID,
                    thinkingLevel: normalizedThinkingLevel,
                    professionKey: existingProfile.draft.professionKey,
                    defaultPluginIDs: [],
                    defaultSkillIDs: existingProfile.draft.defaultSkillIDs
                ),
                memberDraft: .init(
                    role: role.trimmingCharacters(in: .whitespacesAndNewlines),
                    responsibility: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    pluginAllowlist: []
                )
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func setProjectManager(agentID: String) async -> Bool {
        guard let room,
              let profile = profilesByID[agentID],
              profile.draft.professionKey == "project_manager",
              members.contains(where: { $0.agentID == agentID && $0.status == .active }) else {
            errorMessage = AgentGroupChatError.invalidField(
                "projectManagerProfession"
            ).localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            _ = try await store.setProjectManager(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agentID
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func generateAgentDraft(brief: String, builderModelConfigID: String) async -> LocalAgentDraft? {
        do {
            let draft = try await builderService.generateDraft(
                ownerUserID: ownerUserID,
                projectID: projectID,
                brief: brief,
                builderModelConfigID: builderModelConfigID
            )
            errorMessage = nil
            return draft
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func confirmAgentDraft(_ draft: LocalAgentDraft) async -> Bool {
        do {
            _ = try await builderService.createConfirmedDraft(
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: draft
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func approveProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            _ = try await builderService.approveProposal(
                ownerUserID: ownerUserID,
                projectID: projectID,
                proposal: proposal
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectAgentProposal(
                ownerUserID: ownerUserID,
                roomID: proposal.roomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveRemovalProposal(_ proposal: LocalAgentRemovalProposal) async {
        guard removalProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { removalProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.approveAgentRemovalProposal(
                ownerUserID: ownerUserID,
                roomID: proposal.roomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectRemovalProposal(_ proposal: LocalAgentRemovalProposal) async {
        guard removalProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { removalProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectAgentRemovalProposal(
                ownerUserID: ownerUserID,
                roomID: proposal.roomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveTeamProposal(
        _ proposal: LocalAgentTeamCreationProposal
    ) async -> WorkspaceProject? {
        guard teamProposalActionIDs.insert(proposal.id).inserted else { return nil }
        defer { teamProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
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
            _ = try await store.approveTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                resolvedProjectID: resolvedProjectID,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            return createdProject
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func rejectTeamProposal(_ proposal: LocalAgentTeamCreationProposal) async {
        guard teamProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { teamProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard membershipProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { membershipProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.approveMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard membershipProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { membershipProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func sendMessage() async {
        guard let room, !isSending else { return }
        let content = draftMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        let outgoingAttachments = attachments
        guard !content.isEmpty || !outgoingAttachments.isEmpty else { return }
        isSending = true
        defer { isSending = false }
        do {
            let store = try await resolveStore()
            let post = try await store.postMessage(
                ownerUserID: ownerUserID,
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: ownerUserID,
                    content: content,
                    mentionedAgentIDs: selectedMentionAgentIDs.sorted(),
                    attachments: outgoingAttachments.map(ProjectAgentMessageAttachmentDraft.init)
                ),
                limits: .init()
            )
            draftMessage = ""
            attachments = []
            attachmentError = nil
            selectedMentionAgentIDs.removeAll()
            await load()
            if !post.deliveries.isEmpty {
                startScheduler()
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func toggleMention(agentID: String) {
        if selectedMentionAgentIDs.contains(agentID) {
            selectedMentionAgentIDs.remove(agentID)
        } else {
            selectedMentionAgentIDs.insert(agentID)
        }
    }

    func resumeRun(deliveryID: String) async {
        guard !isRunningAgents, runActionDeliveryIDs.insert(deliveryID).inserted else { return }
        defer { runActionDeliveryIDs.remove(deliveryID) }
        do {
            let result = try await scheduler.resumeDelivery(
                ownerUserID: ownerUserID,
                projectID: projectID,
                deliveryID: deliveryID
            )
            await load()
            switch result.outcome {
            case .completed:
                startScheduler()
            case .suspended, .failed:
                errorMessage = result.detail ?? "本地 Agent Run 尚未完成。"
            }
        } catch {
            await load()
            errorMessage = error.localizedDescription
        }
    }

    func abandonRun(deliveryID: String) async {
        guard !isRunningAgents, runActionDeliveryIDs.insert(deliveryID).inserted else { return }
        defer { runActionDeliveryIDs.remove(deliveryID) }
        do {
            try await scheduler.abandonDelivery(
                ownerUserID: ownerUserID,
                projectID: projectID,
                deliveryID: deliveryID
            )
            await load()
            startScheduler()
        } catch {
            await load()
            errorMessage = error.localizedDescription
        }
    }

    func pauseAgents() async {
        guard isRunningAgents, !isPausingAgents, !isStoppingAgents else { return }
        isPausingAgents = true
        schedulerNeedsAnotherPass = false
        let activeTask = schedulerTask
        activeTask?.cancel()
        await activeTask?.value
        await load()
        isPausingAgents = false
    }

    func stopAllAgents() async {
        guard !isStoppingAgents else { return }
        isStoppingAgents = true
        schedulerNeedsAnotherPass = false
        let activeTask = schedulerTask
        activeTask?.cancel()
        await activeTask?.value
        do {
            _ = try await scheduler.stopProject(
                ownerUserID: ownerUserID,
                projectID: projectID
            )
            await load()
            errorMessage = nil
        } catch {
            await load()
            errorMessage = error.localizedDescription
        }
        isStoppingAgents = false
    }

    private func resolveStore() async throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try await service.store()
        openedStore = store
        return store
    }

    private func loadAttachmentData(
        messages: [ProjectAgentMessage],
        roomID: String,
        store: SQLiteAgentGroupChatStore
    ) async throws -> [String: Data] {
        var result: [String: Data] = [:]
        for message in messages {
            for attachment in message.attachmentItems where attachment.kind == .image {
                guard let payload = try await store.messageAttachment(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
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

    private func startScheduler() {
        schedulerNeedsAnotherPass = true
        guard schedulerTask == nil else { return }
        isRunningAgents = true
        schedulerTask = Task { [weak self] in
            guard let self else { return }
            repeat {
                schedulerNeedsAnotherPass = false
                var schedulerMessage: String?
                do {
                    let results = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                    if let failure = results.last(where: { $0.outcome == .failed }) {
                        schedulerMessage = failure.detail ?? "本地 Agent 运行失败。"
                    } else if let suspended = results.last(where: { $0.outcome == .suspended }) {
                        schedulerMessage = suspended.detail ?? "本地 Agent 已暂停，运行检查点已保存。"
                    }
                } catch {
                    schedulerMessage = error.localizedDescription
                }
                await load()
                NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
                if let schedulerMessage { errorMessage = schedulerMessage }
            } while schedulerNeedsAnotherPass
            isRunningAgents = false
            schedulerTask = nil
        }
    }
}
