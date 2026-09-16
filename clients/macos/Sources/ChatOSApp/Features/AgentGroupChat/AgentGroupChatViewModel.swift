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
    @Published private(set) var installedPlugins: [NativeInstalledAgentPlugin] = []
    @Published private(set) var availableModels: [LocalAgentBuilderModelOption] = []
    @Published private(set) var interruptedRuns: [InterruptedRunPresentation] = []
    @Published private(set) var pendingProposals: [LocalAgentCreationProposal] = []
    @Published var draftMessage = ""
    @Published var selectedMentionAgentIDs: Set<String> = []
    @Published private(set) var isLoading = false
    @Published private(set) var isSending = false
    @Published private(set) var isRunningAgents = false
    @Published private(set) var isPausingAgents = false
    @Published private(set) var isStoppingAgents = false
    @Published private(set) var runActionDeliveryIDs: Set<String> = []
    @Published private(set) var proposalActionIDs: Set<String> = []
    @Published var errorMessage: String?

    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private let pluginService: NativeLocalConnectorService
    private var openedStore: SQLiteAgentGroupChatStore?
    private var schedulerTask: Task<Void, Never>?
    private var schedulerNeedsAnotherPass = false

    init(
        projectID: String,
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        pluginService: NativeLocalConnectorService
    ) {
        self.projectID = projectID
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
        self.builderService = builderService
        self.pluginService = pluginService
    }

    var profilesByID: [String: LocalAgentProfile] {
        Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
    }

    var activeMembers: [MemberPresentation] {
        let profiles = profilesByID
        return members.map { MemberPresentation(member: $0, profile: profiles[$0.agentID]) }
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
            let room = try await store.activeRoom(ownerUserID: ownerUserID, projectID: projectID)
            let members: [ProjectAgentRoomMember]
            let messages: [ProjectAgentMessage]
            let interruptedRuns: [InterruptedRunPresentation]
            let pendingProposals: [LocalAgentCreationProposal]
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
            } else {
                members = []
                messages = []
                interruptedRuns = []
                pendingProposals = []
            }
            self.agents = agents
            self.room = room
            self.members = members
            self.messages = messages
            self.interruptedRuns = interruptedRuns
            self.pendingProposals = pendingProposals
            self.installedPlugins = (try? await pluginService.installedAgentPlugins(
                ownerUserID: ownerUserID
            )) ?? []
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

    func createRoom(name: String, goal: String) async -> Bool {
        do {
            let store = try await resolveStore()
            _ = try await store.createRoom(
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    goal: goal.trimmingCharacters(in: .whitespacesAndNewlines)
                )
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
        pluginIDs: [String]
    ) async -> Bool {
        guard let room else {
            errorMessage = AgentGroupChatError.notFound.localizedDescription
            return false
        }
        let normalizedModelConfigID = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard availableModels.contains(where: { $0.id == normalizedModelConfigID }) else {
            errorMessage = LocalAgentBuilderError.modelUnavailable.localizedDescription
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
                    defaultPluginIDs: pluginIDs
                )
            )
            _ = try await store.addMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agent.id,
                draft: .init(
                    role: role.trimmingCharacters(in: .whitespacesAndNewlines),
                    responsibility: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    pluginAllowlist: pluginIDs
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

    func updateAgentMembership(
        agentID: String,
        name: String,
        role: String,
        responsibility: String,
        rolePrompt: String,
        modelConfigID: String,
        pluginIDs: [String]
    ) async -> Bool {
        guard let room, let existingProfile = profilesByID[agentID] else {
            errorMessage = AgentGroupChatError.notFound.localizedDescription
            return false
        }
        let normalizedModelConfigID = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard availableModels.contains(where: { $0.id == normalizedModelConfigID }) else {
            errorMessage = LocalAgentBuilderError.modelUnavailable.localizedDescription
            return false
        }
        let installedPluginIDs = Set(installedPlugins.map(\.id))
        guard pluginIDs.allSatisfy(installedPluginIDs.contains) else {
            errorMessage = "选择的本机 Plugin 已停用或卸载，请刷新后重试。"
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
                    defaultPluginIDs: pluginIDs,
                    defaultSkillIDs: existingProfile.draft.defaultSkillIDs
                ),
                memberDraft: .init(
                    role: role.trimmingCharacters(in: .whitespacesAndNewlines),
                    responsibility: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    pluginAllowlist: pluginIDs
                )
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

    func sendMessage() async {
        guard let room, !isSending else { return }
        let content = draftMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !content.isEmpty else { return }
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
                    mentionedAgentIDs: selectedMentionAgentIDs.sorted()
                ),
                limits: .init()
            )
            draftMessage = ""
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
                    let results = try await scheduler.drainProject(
                        ownerUserID: ownerUserID,
                        projectID: projectID
                    )
                    if let failure = results.last(where: { $0.outcome == .failed }) {
                        schedulerMessage = failure.detail ?? "本地 Agent 运行失败。"
                    } else if let suspended = results.last(where: { $0.outcome == .suspended }) {
                        schedulerMessage = suspended.detail ?? "本地 Agent 已暂停，运行检查点已保存。"
                    }
                } catch {
                    schedulerMessage = error.localizedDescription
                }
                await load()
                if let schedulerMessage { errorMessage = schedulerMessage }
            } while schedulerNeedsAnotherPass
            isRunningAgents = false
            schedulerTask = nil
        }
    }
}
