import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation

@MainActor
final class AgentGroupChatWorkspaceViewModel: ObservableObject {
    struct TriggerRunPresentation: Identifiable {
        var id: UUID { run.id }
        let run: LocalAgentGroupChatRun
        let delivery: ProjectAgentDelivery?
        let room: ProjectAgentRoom?
        let triggerMessage: ProjectAgentMessage?
    }

    @Published private(set) var rooms: [ProjectAgentRoom] = []
    @Published private(set) var directConversations: [ProjectAgentRoom] = []
    @Published private(set) var agents: [LocalAgentProfile] = []
    @Published private(set) var availableModels: [LocalAgentBuilderModelOption] = []
    @Published private(set) var triggerRuns: [TriggerRunPresentation] = []
    @Published private(set) var selectedAgentID: String?
    @Published private(set) var isLoadingTriggerRuns = false
    @Published private(set) var runActionDeliveryIDs: Set<String> = []
    @Published var selectedRoomID: String?
    @Published private(set) var isLoading = false
    @Published private(set) var isLoadingModels = false
    @Published private(set) var isCreating = false
    @Published private(set) var isSavingAgent = false
    @Published var errorMessage: String?

    private let ownerUserID: String
    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private var openedStore: SQLiteAgentGroupChatStore?
    private var modelLoadTask: Task<LocalAgentBuilderResources, Error>?
    private var hasLoadedModels = false
    private var changeObservationTask: Task<Void, Never>?

    init(
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService
    ) {
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
        self.builderService = builderService
    }

    deinit {
        changeObservationTask?.cancel()
    }

    func activate() async {
        startChangeObservation()
        await load()
    }

    private func startChangeObservation() {
        guard changeObservationTask == nil else { return }
        let service = service
        let ownerUserID = ownerUserID
        changeObservationTask = Task { [weak self] in
            let changes = await service.changes(ownerUserID: ownerUserID)
            for await change in changes {
                guard !Task.isCancelled else { break }
                guard change.kind == .roomUpdated || change.kind == .deliveryClaimed else {
                    continue
                }
                try? await Task.sleep(for: .milliseconds(120))
                guard !Task.isCancelled else { break }
                await self?.load()
            }
        }
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let store = try await resolveStore()
            let rooms = try await store.listRooms(ownerUserID: ownerUserID)
            let directConversations = try await store.listDirectConversations(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            let agents = try await store.listAgents(ownerUserID: ownerUserID, includeArchived: false)
            self.rooms = rooms
            self.directConversations = directConversations
            self.agents = agents
            if let selectedAgentID,
               !agents.contains(where: { $0.id == selectedAgentID }) {
                self.selectedAgentID = nil
                triggerRuns = []
            }
            if let selectedRoomID, rooms.contains(where: { $0.id == selectedRoomID }) {
                self.selectedRoomID = selectedRoomID
            } else {
                self.selectedRoomID = nil
            }
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func prepareAgentEditor() async -> Bool {
        if hasLoadedModels {
            if availableModels.isEmpty {
                errorMessage = LocalAgentBuilderError.noAvailableModel.localizedDescription
                return false
            }
            return true
        }

        let task: Task<LocalAgentBuilderResources, Error>
        if let modelLoadTask {
            task = modelLoadTask
        } else {
            let builderService = builderService
            let ownerUserID = ownerUserID
            let created = Task {
                try await builderService.loadResources(ownerUserID: ownerUserID)
            }
            modelLoadTask = created
            task = created
        }
        isLoadingModels = true
        defer {
            isLoadingModels = false
            modelLoadTask = nil
        }
        do {
            let resources = try await task.value
            availableModels = resources.models
            hasLoadedModels = true
            guard !availableModels.isEmpty else {
                errorMessage = LocalAgentBuilderError.noAvailableModel.localizedDescription
                return false
            }
            errorMessage = nil
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func saveAgent(
        existing: LocalAgentProfile?,
        name: String,
        description: String,
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String?,
        professionKey: String,
        canManageStaff: Bool,
        canAccessLocalProjects: Bool,
        heartbeatEnabled: Bool,
        heartbeatIntervalSeconds: Int,
        heartbeatPrompt: String
    ) async -> Bool {
        guard !isSavingAgent else { return false }
        let modelConfigID = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let selectedModel = availableModels.first(where: { $0.id == modelConfigID }) else {
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
        let permissions = LocalAgentPermission.normalized(
            preserving: existing?.draft.defaultSkillIDs ?? [],
            canManageStaff: canManageStaff,
            canAccessLocalProjects: canAccessLocalProjects
        )
        let draft = LocalAgentProfileDraft(
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            description: description.trimmingCharacters(in: .whitespacesAndNewlines),
            rolePrompt: rolePrompt.trimmingCharacters(in: .whitespacesAndNewlines),
            modelConfigID: modelConfigID,
            thinkingLevel: normalizedThinkingLevel,
            professionKey: professionKey,
            defaultPluginIDs: [],
            defaultSkillIDs: permissions,
            heartbeatEnabled: heartbeatEnabled,
            heartbeatIntervalSeconds: heartbeatIntervalSeconds,
            heartbeatPrompt: heartbeatPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        )
        isSavingAgent = true
        defer { isSavingAgent = false }
        do {
            let store = try await resolveStore()
            if let existing {
                _ = try await store.updateAgentProfile(
                    ownerUserID: ownerUserID,
                    agentID: existing.id,
                    draft: draft
                )
            } else {
                _ = try await store.createAgent(ownerUserID: ownerUserID, draft: draft)
            }
            await load()
            NotificationCenter.default.post(name: .agentHeartbeatConfigurationDidChange, object: nil)
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func openDirect(with agent: LocalAgentProfile) async -> ProjectAgentRoom? {
        do {
            let store = try await resolveStore()
            let conversation = try await store.openHumanAgentDirect(
                ownerUserID: ownerUserID,
                agentID: agent.id
            )
            directConversations = try await store.listDirectConversations(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            errorMessage = nil
            return conversation
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func selectAgent(_ agentID: String) async {
        selectedAgentID = agentID
        await loadTriggerRuns()
    }

    func loadTriggerRuns() async {
        guard let selectedAgentID, !isLoadingTriggerRuns else { return }
        isLoadingTriggerRuns = true
        defer { isLoadingTriggerRuns = false }
        do {
            let store = try await resolveStore()
            let runs = try await store.listAgentRuns(
                ownerUserID: ownerUserID,
                agentID: selectedAgentID,
                limit: 100
            )
            var presentations: [TriggerRunPresentation] = []
            presentations.reserveCapacity(runs.count)
            for run in runs {
                let delivery = try await store.delivery(
                    ownerUserID: ownerUserID,
                    deliveryID: run.context.deliveryID
                )
                let room = try await store.room(
                    ownerUserID: ownerUserID,
                    roomID: run.context.roomID
                )
                let triggerMessage = try await store.message(
                    ownerUserID: ownerUserID,
                    roomID: run.context.roomID,
                    messageID: run.context.triggerMessageID
                )
                presentations.append(.init(
                    run: run,
                    delivery: delivery,
                    room: room,
                    triggerMessage: triggerMessage
                ))
            }
            guard self.selectedAgentID == selectedAgentID else { return }
            triggerRuns = presentations
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func resumeRun(deliveryID: String, projectID: String) async {
        guard runActionDeliveryIDs.insert(deliveryID).inserted else { return }
        defer { runActionDeliveryIDs.remove(deliveryID) }
        do {
            let result = try await scheduler.resumeDelivery(
                ownerUserID: ownerUserID,
                projectID: projectID,
                deliveryID: deliveryID
            )
            await loadTriggerRuns()
            if result.outcome != .completed {
                errorMessage = result.detail ?? "本地 Agent Run 尚未完成。"
            }
        } catch {
            await loadTriggerRuns()
            errorMessage = error.localizedDescription
        }
    }

    func retryInterruptedRun(deliveryID: String, projectID: String) async {
        guard runActionDeliveryIDs.insert(deliveryID).inserted else { return }
        defer { runActionDeliveryIDs.remove(deliveryID) }
        do {
            let result = try await scheduler.retryInterruptedDelivery(
                ownerUserID: ownerUserID,
                projectID: projectID,
                deliveryID: deliveryID
            )
            await loadTriggerRuns()
            if result.outcome != .completed {
                errorMessage = result.detail ?? "中断步骤尚未完成。"
            }
        } catch {
            await loadTriggerRuns()
            errorMessage = error.localizedDescription
        }
    }

    func abandonRun(deliveryID: String, projectID: String) async {
        guard runActionDeliveryIDs.insert(deliveryID).inserted else { return }
        defer { runActionDeliveryIDs.remove(deliveryID) }
        do {
            try await scheduler.abandonDelivery(
                ownerUserID: ownerUserID,
                projectID: projectID,
                deliveryID: deliveryID
            )
            await loadTriggerRuns()
        } catch {
            await loadTriggerRuns()
            errorMessage = error.localizedDescription
        }
    }

    func createRoom(projectID: String, name: String, goal: String) async -> Bool {
        guard !isCreating else { return false }
        isCreating = true
        defer { isCreating = false }
        do {
            let store = try await resolveStore()
            let room = try await store.createRoom(
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    goal: goal.trimmingCharacters(in: .whitespacesAndNewlines)
                )
            )
            rooms = try await store.listRooms(ownerUserID: ownerUserID)
            selectedRoomID = room.id
            errorMessage = nil
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    private func resolveStore() async throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try await service.store()
        openedStore = store
        return store
    }
}
