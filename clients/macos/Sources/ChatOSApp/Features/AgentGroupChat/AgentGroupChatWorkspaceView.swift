import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI

extension Notification.Name {
    static let agentGroupChatRoomsDidChange = Notification.Name("ChatOS.AgentGroupChatRoomsDidChange")
    static let agentSkillLibraryDidChange = Notification.Name("ChatOS.AgentSkillLibraryDidChange")
    static let agentHeartbeatConfigurationDidChange = Notification.Name(
        "ChatOS.AgentHeartbeatConfigurationDidChange"
    )
}

private enum AgentGroupChatWorkspaceDestination: Hashable {
    case agents
    case direct(String)
    case room(String)
}

@MainActor
private final class AgentGroupChatWorkspaceViewModel: ObservableObject {
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

struct AgentGroupChatWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: AgentGroupChatWorkspaceViewModel
    @State private var showsCreateTeam = false
    @State private var destination: AgentGroupChatWorkspaceDestination = .agents

    private let ownerUserID: String
    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private let skillLibrary: LocalAgentSkillLibrary

    init(
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        skillLibrary: LocalAgentSkillLibrary
    ) {
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
        self.builderService = builderService
        self.skillLibrary = skillLibrary
        _viewModel = StateObject(
            wrappedValue: AgentGroupChatWorkspaceViewModel(
                ownerUserID: ownerUserID,
                service: service,
                scheduler: scheduler,
                builderService: builderService
            )
        )
    }

    var body: some View {
        HStack(spacing: 0) {
            teamList
                .frame(width: 260)
            Divider()
            detail
                .workspaceFill()
        }
        .workspaceFill()
        .navigationTitle("Agent")
        .task { await viewModel.activate() }
        .onReceive(NotificationCenter.default.publisher(for: .agentGroupChatRoomsDidChange)) { _ in
            Task { await viewModel.load() }
        }
        .sheet(isPresented: $showsCreateTeam) {
            CreateAgentTeamSheet(
                projects: availableProjects,
                isCreating: viewModel.isCreating
            ) { projectID, name, goal in
                let created = await viewModel.createRoom(projectID: projectID, name: name, goal: goal)
                if created, let roomID = viewModel.selectedRoomID {
                    destination = .room(roomID)
                }
                return created
            }
        }
        .alert(
            model.localized("Agent 群聊错误", english: "Agent Group Chat Error"),
            isPresented: Binding(
                get: { viewModel.errorMessage != nil },
                set: { if !$0 { viewModel.errorMessage = nil } }
            )
        ) {
            Button(model.localized("好", english: "OK"), role: .cancel) {
                viewModel.errorMessage = nil
            }
        } message: {
            Text(viewModel.errorMessage ?? "")
        }
    }

    private var availableProjects: [ResourceItem] {
        let boundProjectIDs = Set(viewModel.rooms.map(\.projectID))
        return model.projects.filter { !boundProjectIDs.contains($0.id) }
    }

    private var selectedRoom: ProjectAgentRoom? {
        guard case let .room(roomID) = destination else { return nil }
        return viewModel.rooms.first { $0.id == roomID }
    }

    private var selectedDirectConversation: ProjectAgentRoom? {
        guard case let .direct(roomID) = destination else { return nil }
        return viewModel.directConversations.first { $0.id == roomID }
    }

    private var teamList: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Agent")
                        .font(.headline)
                    Text("私聊与项目团队")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    showsCreateTeam = true
                } label: {
                    Image(systemName: "plus")
                }
                .buttonStyle(.borderless)
                .help("创建 Agent 团队")
                .disabled(availableProjects.isEmpty)
            }
            .padding(14)

            Divider()

            if viewModel.isLoading, viewModel.rooms.isEmpty, viewModel.agents.isEmpty {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(selection: $destination) {
                    Section {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Agent 管理")
                                    .font(.body.weight(.medium))
                                Text("\(viewModel.agents.count) 个已创建 Agent")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "person.crop.rectangle.stack")
                        }
                        .padding(.vertical, 4)
                        .tag(AgentGroupChatWorkspaceDestination.agents)
                    }

                    Section("团队") {
                        if viewModel.rooms.isEmpty {
                            Text("还没有项目团队")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(viewModel.rooms) { room in
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(room.draft.name)
                                        .font(.body.weight(.medium))
                                    Text(projectName(for: room.projectID))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                .padding(.vertical, 4)
                                .tag(AgentGroupChatWorkspaceDestination.room(room.id))
                            }
                        }
                    }


                    Section("私聊") {
                        if viewModel.directConversations.isEmpty {
                            Text("还没有私聊")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(viewModel.directConversations) { conversation in
                                Label {
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(conversation.draft.name)
                                            .font(.body.weight(.medium))
                                        Text(conversation.conversationKind == .humanAgentDirect
                                             ? "Agent 私聊" : "Agent 之间")
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                } icon: {
                                    Image(systemName: conversation.conversationKind == .humanAgentDirect
                                          ? "bubble.left.and.bubble.right"
                                          : "person.2.wave.2")
                                }
                                .padding(.vertical, 4)
                                .tag(AgentGroupChatWorkspaceDestination.direct(conversation.id))
                            }
                        }
                    }
                }
                .listStyle(.plain)
                .scrollContentBackground(.hidden)
                .background(Color(nsColor: .windowBackgroundColor))
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private var detail: some View {
        if destination == .agents {
            AgentManagementView(
                viewModel: viewModel,
                ownerUserID: ownerUserID,
                skillLibrary: skillLibrary
            ) { agent in
                Task {
                    if let conversation = await viewModel.openDirect(with: agent) {
                        destination = .direct(conversation.id)
                    }
                }
            }
        } else if let conversation = selectedDirectConversation {
            AgentDirectChatView(
                ownerUserID: ownerUserID,
                conversationID: conversation.id,
                service: service,
                scheduler: scheduler,
                builderService: builderService,
                projectsService: model.localProjectsService
            )
            .id(conversation.id)
        } else if let room = selectedRoom {
            ProjectAgentGroupChatView(
                projectID: room.projectID,
                ownerUserID: ownerUserID,
                service: service,
                scheduler: scheduler,
                builderService: builderService,
                projectsService: model.localProjectsService
            )
            .id(room.id)
        } else {
            ContentUnavailableView {
                Label("团队不存在", systemImage: "person.3.sequence.fill")
            } description: {
                Text("请选择其他团队，或新建一个绑定项目的团队。")
            }
        }
    }

    private func projectName(for projectID: String) -> String {
        model.projects.first(where: { $0.id == projectID })?.title
            ?? model.localized("项目已移除", english: "Project Removed")
    }
}

private enum AgentProfileEditorTarget: Identifiable {
    case create
    case edit(LocalAgentProfile)

    var id: String {
        switch self {
        case .create: "new-agent"
        case let .edit(profile): profile.id
        }
    }
}

private struct AgentManagementView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: AgentGroupChatWorkspaceViewModel
    let ownerUserID: String
    let skillLibrary: LocalAgentSkillLibrary
    let openDirect: (LocalAgentProfile) -> Void
    @State private var editorTarget: AgentProfileEditorTarget?
    @State private var showsSkillManager = false
    @State private var skillRevision = 0
    @State private var runToAbandon: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Agent 管理")
                        .font(.title3.weight(.semibold))
                    Text("管理 Agent、模型和权限。")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("Skill 管理", systemImage: "books.vertical") {
                    showsSkillManager = true
                }
                .buttonStyle(.bordered)
                Button {
                    openEditor(.create)
                } label: {
                    if viewModel.isLoadingModels {
                        ProgressView().controlSize(.small)
                    } else {
                        Label("创建 Agent", systemImage: "person.badge.plus")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isLoadingModels)
            }
            .padding(18)

            Divider()

            if viewModel.agents.isEmpty {
                ContentUnavailableView {
                    Label("还没有 Agent", systemImage: "person.crop.rectangle.stack")
                } description: {
                    Text("创建后可以直接私聊，也可以加入项目团队。")
                } actions: {
                    Button {
                        openEditor(.create)
                    } label: {
                        if viewModel.isLoadingModels {
                            ProgressView().controlSize(.small)
                        } else {
                            Text("创建 Agent")
                        }
                    }
                        .buttonStyle(.borderedProminent)
                        .disabled(viewModel.isLoadingModels)
                }
            } else {
                VStack(spacing: 0) {
                    ScrollView {
                        LazyVGrid(
                            columns: [GridItem(.adaptive(minimum: 280, maximum: 420), spacing: 14)],
                            alignment: .leading,
                            spacing: 14
                        ) {
                            ForEach(viewModel.agents) { agent in
                                agentCard(agent)
                            }
                        }
                        .padding(18)
                    }
                    .frame(maxHeight: viewModel.selectedAgentID == nil ? .infinity : 320)

                    if viewModel.selectedAgentID != nil {
                        Spacer(minLength: 24)
                        Divider()
                        triggerRunsPanel
                            .frame(height: 340)
                    } else {
                        Spacer(minLength: 0)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .sheet(item: $editorTarget) { target in
            AgentProfileEditorSheet(
                viewModel: viewModel,
                target: target,
                professions: professions
            )
        }
        .sheet(isPresented: $showsSkillManager) {
            AgentSkillManagementSheet(
                ownerUserID: ownerUserID,
                skillLibrary: skillLibrary,
                initialLanguage: model.contextLanguage,
                onLanguageChange: { model.contextLanguage = $0 }
            )
        }
        .onReceive(NotificationCenter.default.publisher(for: .agentSkillLibraryDidChange)) { _ in
            skillRevision += 1
        }
        .onReceive(NotificationCenter.default.publisher(for: .agentGroupChatRoomsDidChange)) { _ in
            Task { await viewModel.loadTriggerRuns() }
        }
        .confirmationDialog(
            "结束这次运行？",
            isPresented: Binding(
                get: { runToAbandon != nil },
                set: { if !$0 { runToAbandon = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("结束运行", role: .destructive) {
                guard let item = runToAbandon,
                      let deliveryID = item.delivery?.id else { return }
                runToAbandon = nil
                Task {
                    await viewModel.abandonRun(
                        deliveryID: deliveryID,
                        projectID: item.run.context.projectID
                    )
                }
            }
            Button("取消", role: .cancel) { runToAbandon = nil }
        } message: {
            Text("运行会标记为失败并释放 Agent 队列；检查点和事件记录仍会保留。")
        }
    }

    private func agentCard(_ agent: LocalAgentProfile) -> some View {
        let canManageStaff = LocalAgentPermission.canManageStaff(agent.draft.defaultSkillIDs)
        let canAccessLocalProjects = LocalAgentPermission.canAccessLocalProjects(
            agent.draft.defaultSkillIDs
        )
        return VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: canManageStaff ? "person.crop.circle.badge.checkmark" : "person.crop.circle")
                    .font(.title2)
                    .foregroundStyle(canManageStaff ? Color.accentColor : .secondary)
                VStack(alignment: .leading, spacing: 3) {
                    Text(agent.draft.name)
                        .font(.headline)
                    Text(canManageStaff ? "可招募和解雇成员" : "无人员管理权限")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            if !agent.draft.description.isEmpty {
                Text(agent.draft.description)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            HStack(spacing: 10) {
                Button("私聊", systemImage: "bubble.left.and.bubble.right") {
                    openDirect(agent)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .fixedSize()
                Spacer(minLength: 0)
                Button("编辑") { openEditor(.edit(agent)) }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .fixedSize()
                    .disabled(viewModel.isLoadingModels)
            }
            Divider()
            LabeledContent("模型") {
                Text(modelName(agent.draft.modelConfigID))
                    .lineLimit(1)
            }
            .font(.caption)
            LabeledContent("思考等级") {
                Text(agent.draft.thinkingLevel ?? "跟随模型默认")
            }
            .font(.caption)
            LabeledContent("职业") {
                Text(professions.first(where: { $0.key == agent.draft.professionKey })?.label
                    ?? agent.draft.professionKey)
            }
            .font(.caption)
            LabeledContent("工具与 Plugin") {
                Text("按任务自主发现")
            }
            .font(.caption)
            LabeledContent("主动巡检") {
                Text(
                    agent.draft.heartbeatEnabled
                        ? Self.heartbeatIntervalLabel(agent.draft.heartbeatIntervalSeconds)
                        : "关闭"
                )
            }
            .font(.caption)
            if canManageStaff || canAccessLocalProjects {
                HStack(spacing: 6) {
                    if canManageStaff {
                        Label("人员管理", systemImage: "person.2.badge.gearshape")
                    }
                    if canAccessLocalProjects {
                        Label("项目与团队", systemImage: "folder.badge.gearshape")
                    }
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(
                    viewModel.selectedAgentID == agent.id
                        ? Color.accentColor : Color.primary.opacity(0.08),
                    lineWidth: viewModel.selectedAgentID == agent.id ? 2 : 1
                )
        }
        .contentShape(RoundedRectangle(cornerRadius: 12))
        .onTapGesture {
            Task { await viewModel.selectAgent(agent.id) }
        }
    }

    @ViewBuilder
    private var triggerRunsPanel: some View {
        if let selectedAgentID = viewModel.selectedAgentID,
           let agent = viewModel.agents.first(where: { $0.id == selectedAgentID }) {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(agent.draft.name) · 运行动态")
                            .font(.headline)
                        Text("最近的消息处理、主动巡检和任务执行")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.isLoadingTriggerRuns {
                        ProgressView().controlSize(.small)
                    }
                    Button("刷新", systemImage: "arrow.clockwise") {
                        Task { await viewModel.loadTriggerRuns() }
                    }
                    .buttonStyle(.borderless)
                }

                HStack(spacing: 10) {
                    laneOverviewCard(
                        lane: .manager,
                        title: "沟通层",
                        subtitle: "处理消息、记忆与任务调度",
                        systemImage: "bubble.left.and.bubble.right"
                    )
                    laneOverviewCard(
                        lane: .executor,
                        title: "执行层",
                        subtitle: "执行 Todo、工具与 Plugin",
                        systemImage: "hammer"
                    )
                }

                if viewModel.triggerRuns.isEmpty, !viewModel.isLoadingTriggerRuns {
                    ContentUnavailableView(
                        "还没有运行记录",
                        systemImage: "bolt.horizontal.circle",
                        description: Text("Agent 开始处理消息或任务后，会显示在这里。")
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        LazyVStack(spacing: 10) {
                            ForEach(viewModel.triggerRuns) { item in
                                triggerRunCard(item)
                            }
                        }
                    }
                }
            }
            .padding(18)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        } else {
            ContentUnavailableView(
                "选择一个 Agent",
                systemImage: "person.crop.circle.badge.questionmark",
                description: Text("点击上方 Agent 卡片查看它的运行动态。")
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func triggerRunCard(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 18) {
                    runFact("运行通道", laneLabel(item.run.context.lane))
                    runFact("状态", runStatusLabel(item.run.checkpoint.status))
                    runFact("唤醒来源", triggerLabel(item))
                    runFact("耗时", durationLabel(item.run.checkpoint.elapsedSeconds))
                    runFact("模型请求", "\(item.run.checkpoint.modelCalls) 次")
                    Spacer(minLength: 0)
                }
                if let content = item.triggerMessage?.content,
                   !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    GroupBox("收到的内容") {
                        Text(content)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .lineLimit(3)
                            .textSelection(.enabled)
                    }
                }
                if let reason = displayedStopReason(item) {
                    Label(userFacingStopReason(reason), systemImage: "exclamationmark.circle")
                        .font(.callout)
                        .foregroundStyle(triggerColor(item.run.checkpoint.status))
                        .textSelection(.enabled)
                }
                if let result = item.run.checkpoint.result ?? item.run.checkpoint.completionResult,
                   !result.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    GroupBox("处理结果") {
                        Text(LocalizedStringKey(result))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .lineLimit(5)
                            .textSelection(.enabled)
                    }
                }
                if !item.run.events.isEmpty {
                    DisclosureGroup("技术诊断 · \(item.run.events.count) 条") {
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(item.run.events.suffix(12)) { event in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(diagnosticEventLabel(event.kind))
                                        .font(.caption.weight(.medium))
                                    Text(event.detail)
                                        .font(.caption2.monospaced())
                                        .foregroundStyle(.secondary)
                                        .lineLimit(3)
                                        .textSelection(.enabled)
                                }
                            }
                        }
                        .padding(.top, 6)
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                }
                .padding(.top, 8)
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: triggerIcon(item.delivery?.triggerKind))
                        .foregroundStyle(triggerColor(item.run.checkpoint.status))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(laneLabel(item.run.context.lane)) · \(runStatusLabel(item.run.checkpoint.status))")
                            .font(.callout.weight(.medium))
                        Text("\(triggerLabel(item)) · \(item.room?.draft.name ?? "来源会话已移除") · \(formattedTime(item.run.updatedAtUnixMs))")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text(runSummary(item))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            if isActionable(item) {
                Divider()
                HStack(spacing: 8) {
                    if let reason = displayedStopReason(item) {
                        Text(userFacingStopReason(reason))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                    Spacer()
                    if let deliveryID = item.delivery?.id,
                       viewModel.runActionDeliveryIDs.contains(deliveryID) {
                        ProgressView().controlSize(.small)
                    } else if let deliveryID = item.delivery?.id {
                        Button(item.run.checkpoint.status == .needsReview
                               ? "重试中断步骤" : "继续运行") {
                            Task {
                                if item.run.checkpoint.status == .needsReview {
                                    await viewModel.retryInterruptedRun(
                                        deliveryID: deliveryID,
                                        projectID: item.run.context.projectID
                                    )
                                } else {
                                    await viewModel.resumeRun(
                                        deliveryID: deliveryID,
                                        projectID: item.run.context.projectID
                                    )
                                }
                            }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        Button("结束", role: .destructive) {
                            runToAbandon = item
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                    }
                }
            }
        }
        .padding(12)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
    }

    private func triggerLabel(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> String {
        switch item.delivery?.triggerKind {
        case .mention:
            item.room?.conversationKind.isDirect == true ? "私聊消息" : "群聊提及"
        case .defaultAgent:
            switch item.room?.conversationKind {
            case .humanAgentDirect: "私聊消息"
            case .agentAgentDirect: "Agent 私聊"
            case .projectTeam: "群聊消息"
            case nil: "消息唤醒"
            }
        case .agentMention:
            item.room?.conversationKind.isDirect == true ? "Agent 私聊" : "Agent 提及"
        case .heartbeat: "主动巡检"
        case .todo: "Todo 执行"
        case .todoStatus: "Todo 状态"
        case nil: "系统唤醒"
        }
    }

    private func laneLabel(_ lane: LocalAgentRunLane) -> String {
        switch lane {
        case .manager: "沟通层"
        case .executor: "执行层"
        }
    }

    private func laneOverviewCard(
        lane: LocalAgentRunLane,
        title: String,
        subtitle: String,
        systemImage: String
    ) -> some View {
        let latest = viewModel.triggerRuns.first(where: { $0.run.context.lane == lane })
        return HStack(spacing: 10) {
            Image(systemName: systemImage)
                .font(.title3)
                .foregroundStyle(latest.map { triggerColor($0.run.checkpoint.status) } ?? .secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(title).font(.callout.weight(.semibold))
                    Text(latest.map { runStatusLabel($0.run.checkpoint.status) } ?? "暂无运行")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                Text(latest.map {
                    "\(subtitle) · \(formattedTime($0.run.updatedAtUnixMs))"
                } ?? subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
        .overlay {
            RoundedRectangle(cornerRadius: 9)
                .stroke(Color.primary.opacity(0.07), lineWidth: 1)
        }
    }

    private func runSummary(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> String {
        if let result = item.run.checkpoint.result ?? item.run.checkpoint.completionResult,
           !result.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return "已有处理结果"
        }
        switch item.run.checkpoint.status {
        case .running: return "正在处理"
        case .paused, .needsReview, .limitReached: return "等待处理"
        case .completed: return "处理完成"
        case .failed: return "未能完成"
        case .ready: return "等待开始"
        }
    }

    private func runFact(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(.tertiary)
            Text(value).font(.caption.weight(.medium))
        }
    }

    private func durationLabel(_ seconds: Double) -> String {
        guard seconds >= 1 else { return "不足 1 秒" }
        if seconds < 60 { return "\(Int(seconds)) 秒" }
        return "\(Int(seconds) / 60) 分 \(Int(seconds) % 60) 秒"
    }

    private func userFacingStopReason(_ reason: String) -> String {
        if reason.localizedCaseInsensitiveContains("no such column") {
            return "当时本地数据升级未完成，运行已中断；现在可以重试。"
        }
        if reason.localizedCaseInsensitiveContains("cancel") || reason.contains("取消") {
            return "这次运行已取消。"
        }
        if reason.localizedCaseInsensitiveContains("timeout") || reason.contains("超时") {
            return "处理时间较长，这次运行已暂停，可以稍后恢复。"
        }
        return reason
    }

    private func displayedStopReason(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> String? {
        if let reason = item.run.checkpoint.stopReason?.trimmingCharacters(
            in: .whitespacesAndNewlines
        ), !reason.isEmpty {
            return reason
        }
        return item.run.events.last(where: {
            $0.kind == "needs_review" || $0.kind == "resume_failed"
        })?.detail
    }

    private func diagnosticEventLabel(_ kind: String) -> String {
        switch kind {
        case "memory_connecting": "准备历史上下文"
        case "memory_connected", "memory_reconnected", "memory_bound": "历史上下文已就绪"
        case "memory_syncing": "保存运行记录"
        case "memory_synced": "运行记录已保存"
        case "model_request": "请求模型"
        case "model_response": "模型已响应"
        case "tool_started": "开始使用工具"
        case "tool_completed": "工具执行完成"
        case "tool_rejected": "工具未执行"
        case "needs_review": "运行中断"
        case "retry_authorized": "已确认重试"
        case "resume_failed": "恢复失败"
        case "memory_unavailable": "历史服务暂不可用"
        default: kind.replacingOccurrences(of: "_", with: " ")
        }
    }

    private func isActionable(
        _ item: AgentGroupChatWorkspaceViewModel.TriggerRunPresentation
    ) -> Bool {
        guard item.delivery?.status == .running else { return false }
        return switch item.run.checkpoint.status {
        case .paused, .needsReview, .limitReached:
            true
        case .ready, .running, .completed, .failed:
            false
        }
    }

    private func triggerIcon(_ kind: ProjectAgentDeliveryTriggerKind?) -> String {
        switch kind {
        case .heartbeat: "heart.circle"
        case .todo, .todoStatus: "checklist"
        case .mention, .defaultAgent, .agentMention: "bubble.left.and.bubble.right"
        case nil: "bolt.horizontal.circle"
        }
    }

    private func runStatusLabel(_ status: AgentRunCheckpoint.Status) -> String {
        switch status {
        case .ready: "等待"
        case .running: "运行中"
        case .paused: "已暂停"
        case .needsReview: "运行中断"
        case .limitReached: "达到限制"
        case .completed: "已完成"
        case .failed: "失败"
        }
    }

    private func triggerColor(_ status: AgentRunCheckpoint.Status) -> Color {
        switch status {
        case .completed: .green
        case .running: .accentColor
        case .paused, .needsReview, .limitReached: .orange
        case .failed: .red
        case .ready: .secondary
        }
    }

    private func formattedTime(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000).formatted(
            date: .abbreviated,
            time: .shortened
        )
    }

    private static func heartbeatIntervalLabel(_ seconds: Int) -> String {
        switch seconds {
        case 60: "每分钟"
        case 300: "每 5 分钟"
        case 900: "每 15 分钟"
        case 1_800: "每 30 分钟"
        case 3_600: "每小时"
        default: "每 \(seconds / 60) 分钟"
        }
    }

    private func modelName(_ id: String) -> String {
        guard let model = viewModel.availableModels.first(where: { $0.id == id }) else {
            return "已配置"
        }
        return "\(model.name) · \(model.modelName)"
    }

    private func openEditor(_ target: AgentProfileEditorTarget) {
        Task {
            guard await viewModel.prepareAgentEditor() else { return }
            editorTarget = target
        }
    }

    private var professions: [LocalAgentProfessionDefinition] {
        _ = skillRevision
        return skillLibrary.professions(ownerUserID: ownerUserID)
    }
}

private struct AgentProfileEditorSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatWorkspaceViewModel
    let target: AgentProfileEditorTarget
    let professions: [LocalAgentProfessionDefinition]

    @State private var name: String
    @State private var description: String
    @State private var rolePrompt: String
    @State private var modelConfigID: String
    @State private var thinkingLevel: String
    @State private var professionKey: String
    @State private var canManageStaff: Bool
    @State private var canAccessLocalProjects: Bool
    @State private var heartbeatEnabled: Bool
    @State private var heartbeatIntervalSeconds: Int
    @State private var heartbeatPrompt: String

    init(
        viewModel: AgentGroupChatWorkspaceViewModel,
        target: AgentProfileEditorTarget,
        professions: [LocalAgentProfessionDefinition]
    ) {
        self.viewModel = viewModel
        self.target = target
        self.professions = professions
        let profile: LocalAgentProfile?
        switch target {
        case .create:
            profile = nil
        case let .edit(value):
            profile = value
        }
        _name = State(initialValue: profile?.draft.name ?? "")
        _description = State(initialValue: profile?.draft.description ?? "")
        _rolePrompt = State(initialValue: profile?.draft.rolePrompt
            ?? LocalAgentPromptCatalog.render(.agentDefaultRole))
        _modelConfigID = State(initialValue: profile?.draft.modelConfigID ?? "")
        _thinkingLevel = State(initialValue: profile?.draft.thinkingLevel ?? "")
        _professionKey = State(initialValue: profile?.draft.professionKey
            ?? LocalAgentSkillCatalog.legacyProfessionKey)
        _canManageStaff = State(initialValue: profile.map {
            LocalAgentPermission.canManageStaff($0.draft.defaultSkillIDs)
        } ?? false)
        _canAccessLocalProjects = State(initialValue: profile.map {
            LocalAgentPermission.canAccessLocalProjects($0.draft.defaultSkillIDs)
        } ?? false)
        _heartbeatEnabled = State(initialValue: profile?.draft.heartbeatEnabled ?? false)
        _heartbeatIntervalSeconds = State(
            initialValue: profile?.draft.heartbeatIntervalSeconds ?? 900
        )
        _heartbeatPrompt = State(initialValue: profile?.draft.heartbeatPrompt ?? "")
    }

    private var existing: LocalAgentProfile? {
        guard case let .edit(profile) = target else { return nil }
        return profile
    }

    private var selectedModel: LocalAgentBuilderModelOption? {
        viewModel.availableModels.first(where: { $0.id == modelConfigID })
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(existing == nil ? "创建 Agent" : "编辑 Agent")
                    .font(.title2.weight(.semibold))
                Spacer()
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    editorField("名称") {
                        TextField("Agent 名称", text: $name)
                            .textFieldStyle(.roundedBorder)
                    }
                    editorField("说明") {
                        TextField("Agent 负责什么", text: $description, axis: .vertical)
                            .lineLimit(2...4)
                            .textFieldStyle(.roundedBorder)
                    }
                    editorField("模型") {
                        Picker("", selection: $modelConfigID) {
                            ForEach(viewModel.availableModels) { model in
                                Text("\(model.name) · \(model.modelName)").tag(model.id)
                            }
                        }
                        .labelsHidden()
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    editorField("思考等级") {
                        VStack(alignment: .leading, spacing: 5) {
                            Picker("", selection: $thinkingLevel) {
                                Text(defaultThinkingLabel).tag("")
                                ForEach(selectedModel?.thinkingLevels ?? [], id: \.self) { level in
                                    Text(level).tag(level)
                                }
                            }
                            .labelsHidden()
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .disabled(selectedModel?.supportsReasoning != true)
                            if selectedModel?.supportsReasoning != true {
                                Text("当前模型未启用思考能力")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    editorField("职业") {
                        VStack(alignment: .leading, spacing: 5) {
                            Picker("", selection: $professionKey) {
                                ForEach(professions) { profession in
                                    Text("\(profession.categoryLabel) · \(profession.label)")
                                        .tag(profession.key)
                                }
                            }
                            .labelsHidden()
                            if let selected = professions.first(where: { $0.key == professionKey }) {
                                Text(selected.description)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    editorField("角色 Prompt") {
                        TextField("角色 Prompt", text: $rolePrompt, axis: .vertical)
                            .lineLimit(5...10)
                            .textFieldStyle(.roundedBorder)
                    }
                    Toggle(isOn: $canManageStaff) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("允许招募和解雇成员")
                                .font(.subheadline.weight(.medium))
                            Text("授权后，Agent 可通过 Relay MCP 提交招募或移出团队提案；所有人员变更仍需你确认。")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Toggle(isOn: $canAccessLocalProjects) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("允许查看本地项目并创建团队")
                                .font(.subheadline.weight(.medium))
                            Text("Agent 只看到项目名称和临时单选项；真实项目 ID 与路径由 ChatOS 内部透传。也可以在默认工作区新建项目。")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Divider()
                    Toggle(isOn: $heartbeatEnabled) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("主动巡检")
                                .font(.subheadline.weight(.medium))
                            Text("按周期唤醒这个 Agent，依次检查它加入的全部团队和私聊；无事时不会发送消息。")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    if heartbeatEnabled {
                        editorField("巡检周期") {
                            Picker("", selection: $heartbeatIntervalSeconds) {
                                Text("每分钟").tag(60)
                                Text("每 5 分钟").tag(300)
                                Text("每 15 分钟").tag(900)
                                Text("每 30 分钟").tag(1_800)
                                Text("每小时").tag(3_600)
                            }
                            .labelsHidden()
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        editorField("巡检要求") {
                            TextField(
                                "例如：检查阻塞、无人响应的任务和需要主动推进的工作",
                                text: $heartbeatPrompt,
                                axis: .vertical
                            )
                            .lineLimit(3...6)
                            .textFieldStyle(.roundedBorder)
                        }
                        Text("主动巡检会产生模型调用；通讯与任务执行分离，同一 Agent 的同类运行仍会串行。")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button(existing == nil ? "创建" : "保存") {
                    Task {
                        if await viewModel.saveAgent(
                            existing: existing,
                            name: name,
                            description: description,
                            rolePrompt: rolePrompt,
                            modelConfigID: modelConfigID,
                            thinkingLevel: thinkingLevel,
                            professionKey: professionKey,
                            canManageStaff: canManageStaff,
                            canAccessLocalProjects: canAccessLocalProjects,
                            heartbeatEnabled: heartbeatEnabled,
                            heartbeatIntervalSeconds: heartbeatIntervalSeconds,
                            heartbeatPrompt: heartbeatPrompt
                        ) { dismiss() }
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    viewModel.isSavingAgent
                        || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || rolePrompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || modelConfigID.isEmpty
                )
            }
        }
        .padding(24)
        .frame(width: 640)
        .frame(minHeight: 560)
        .onAppear {
            if modelConfigID.isEmpty {
                modelConfigID = viewModel.availableModels.first?.id ?? ""
            }
            normalizeThinkingLevel()
        }
        .onChange(of: modelConfigID) {
            normalizeThinkingLevel()
        }
    }

    private var defaultThinkingLabel: String {
        guard let configured = selectedModel?.defaultThinkingLevel,
              !configured.isEmpty else { return "跟随模型默认" }
        return "跟随模型默认（\(configured)）"
    }

    private func normalizeThinkingLevel() {
        thinkingLevel = LocalAgentThinkingLevelCatalog.normalized(
            thinkingLevel,
            allowedValues: selectedModel?.thinkingLevels ?? []
        ) ?? ""
    }

    private func editorField<Content: View>(
        _ title: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title).font(.subheadline.weight(.medium))
            content()
        }
    }
}

private struct CreateAgentTeamSheet: View {
    @Environment(\.dismiss) private var dismiss
    let projects: [ResourceItem]
    let isCreating: Bool
    let create: (String, String, String) async -> Bool

    @State private var selectedProjectID = ""
    @State private var name = ""
    @State private var goal = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("创建 Agent 团队")
                .font(.title2.weight(.semibold))
            Text("选择一个项目。只有主动创建团队的项目才会进入 Agent 群聊。")
                .font(.callout)
                .foregroundStyle(.secondary)

            Picker("绑定项目", selection: $selectedProjectID) {
                Text("请选择项目").tag("")
                ForEach(projects) { project in
                    Text(project.title).tag(project.id)
                }
            }

            TextField("团队名称", text: $name)
            TextField("团队目标（可选）", text: $goal, axis: .vertical)
                .lineLimit(2...5)

            Text("团队创建后，可以手动创建 Agent，也可以让 Agent Builder 生成草案并由你确认。")
                .font(.caption)
                .foregroundStyle(.secondary)

            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("创建团队") {
                    Task {
                        if await create(selectedProjectID, resolvedName, goal) {
                            dismiss()
                        }
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(isCreating || selectedProjectID.isEmpty)
            }
        }
        .padding(24)
        .frame(width: 520)
        .onAppear {
            guard selectedProjectID.isEmpty else { return }
            selectedProjectID = projects.first?.id ?? ""
        }
    }

    private var resolvedName: String {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { return trimmed }
        let projectName = projects.first(where: { $0.id == selectedProjectID })?.title ?? "项目"
        return "\(projectName) Agent 团队"
    }
}
