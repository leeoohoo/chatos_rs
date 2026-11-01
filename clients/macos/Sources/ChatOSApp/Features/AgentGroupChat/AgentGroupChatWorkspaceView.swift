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
    case artifacts
    case direct(String)
    case room(String)
}

struct AgentGroupChatWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject private var viewModel: AgentGroupChatWorkspaceViewModel
    @State private var showsCreateTeam = false
    @State private var destination: AgentGroupChatWorkspaceDestination = .agents
    @State private var roomPendingDeletion: ProjectAgentRoom?
    @State private var teamPage = 0
    @State private var teamPageSize = 10
    @State private var directPage = 0
    @State private var directPageSize = 10

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
        skillLibrary: LocalAgentSkillLibrary,
        viewModel: AgentGroupChatWorkspaceViewModel
    ) {
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
        self.builderService = builderService
        self.skillLibrary = skillLibrary
        _viewModel = ObservedObject(wrappedValue: viewModel)
    }

    var body: some View {
        HStack(spacing: 0) {
            teamList
                .frame(width: 248)
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
        .confirmationDialog(
            model.localized("删除这个 Agent 团队？", english: "Delete this Agent team?"),
            isPresented: Binding(
                get: { roomPendingDeletion != nil },
                set: { if !$0 { roomPendingDeletion = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button(model.localized("删除团队", english: "Delete Team"), role: .destructive) {
                guard let room = roomPendingDeletion else { return }
                Task {
                    if await viewModel.deleteRoom(room) {
                        if destination == .room(room.id) { destination = .agents }
                        roomPendingDeletion = nil
                    }
                }
            }
            Button(model.localized("取消", english: "Cancel"), role: .cancel) {
                roomPendingDeletion = nil
            }
        } message: {
            Text(model.localized(
                "团队会立即从列表移除，尚未开始的工作会取消，正在运行的记录会停止；历史消息和运行记录仍保留在本机以保证数据完整。",
                english: "The team will disappear immediately. Queued work will be cancelled and active run records stopped; local history is retained for data integrity."
            ))
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
                HStack(spacing: 10) {
                    Image(systemName: "sparkles")
                        .appFont(.headline)
                        .foregroundStyle(AppPalette.ai)
                        .frame(width: 32, height: 32)
                        .background(AppPalette.aiSoft, in: RoundedRectangle(cornerRadius: 10))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Agent")
                            .appFont(.headline.weight(.semibold))
                        Text("私聊与项目团队")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
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
            .background(AppPalette.surface)

            Divider()

            if !viewModel.hasLoadedInitialSnapshot {
                ProgressView("正在加载 Agent…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 3) {
                        navigationRow(.agents) {
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
                                    .foregroundStyle(AppPalette.ai)
                            }
                        }

                        navigationRow(.artifacts) {
                            Label {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Agent 文档")
                                        .font(.body.weight(.medium))
                                    Text("跨设备发现与预览")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            } icon: {
                                Image(systemName: "icloud.and.arrow.down")
                                    .foregroundStyle(AppPalette.ai)
                            }
                        }

                        sidebarSectionTitle("团队")
                        if viewModel.rooms.isEmpty {
                            Text("还没有项目团队")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                        } else {
                            ForEach(viewModel.rooms.agentPage(index: teamPage, size: teamPageSize)) { room in
                                navigationRow(.room(room.id)) {
                                    HStack(spacing: 8) {
                                        Label {
                                            VStack(alignment: .leading, spacing: 3) {
                                                Text(room.draft.name)
                                                    .appFont(.body.weight(.medium))
                                                Text(projectName(for: room.projectID))
                                                    .appFont(.caption)
                                                    .foregroundStyle(.secondary)
                                                    .lineLimit(1)
                                            }
                                        } icon: {
                                            Image(systemName: "person.3.fill")
                                                .foregroundStyle(AppPalette.ai)
                                        }
                                        Spacer(minLength: 0)
                                        Menu {
                                            Button(
                                                model.localized("删除团队", english: "Delete Team"),
                                                systemImage: "trash",
                                                role: .destructive
                                            ) {
                                                roomPendingDeletion = room
                                            }
                                        } label: {
                                            Image(systemName: "ellipsis.circle")
                                                .foregroundStyle(.secondary)
                                        }
                                        .menuStyle(.borderlessButton)
                                        .menuIndicator(.hidden)
                                        .fixedSize()
                                    }
                                }
                            }
                            AgentListPaginationBar(
                                totalCount: viewModel.rooms.count,
                                page: $teamPage,
                                pageSize: $teamPageSize,
                                compact: true
                            )
                            .padding(.horizontal, 4)
                        }

                        sidebarSectionTitle("私聊")
                        if viewModel.directConversations.isEmpty {
                            Text("还没有私聊")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                        } else {
                            ForEach(
                                viewModel.directConversations.agentPage(
                                    index: directPage,
                                    size: directPageSize
                                )
                            ) { conversation in
                                navigationRow(.direct(conversation.id)) {
                                    HStack(spacing: 12) {
                                        if conversation.conversationKind == .humanAgentDirect,
                                           let agentID = conversation.defaultAgentID,
                                           let agent = viewModel.agents.first(where: { $0.id == agentID }) {
                                            AgentAvatarView(
                                                name: agent.draft.name,
                                                data: agent.draft.avatarData,
                                                size: AgentAvatarMetrics.navigation,
                                                cornerRadius: 15
                                            )
                                        } else {
                                            Image(systemName: "person.2.wave.2")
                                                .foregroundStyle(AppPalette.ai)
                                                .frame(
                                                    width: AgentAvatarMetrics.navigation,
                                                    height: AgentAvatarMetrics.navigation
                                                )
                                                .background(
                                                    AppPalette.aiSoft,
                                                    in: RoundedRectangle(cornerRadius: 15)
                                                )
                                        }

                                        VStack(alignment: .leading, spacing: 3) {
                                            Text(conversation.draft.name)
                                                .font(.body.weight(.medium))
                                                .lineLimit(1)
                                            Text(conversation.conversationKind == .humanAgentDirect
                                                 ? "Agent 私聊" : "Agent 之间")
                                                .font(.caption)
                                                .foregroundStyle(.secondary)
                                                .lineLimit(1)
                                        }
                                        Spacer(minLength: 0)
                                    }
                                    .frame(minHeight: AgentAvatarMetrics.navigation + 8)
                                }
                            }
                            AgentListPaginationBar(
                                totalCount: viewModel.directConversations.count,
                                page: $directPage,
                                pageSize: $directPageSize,
                                compact: true
                            )
                            .padding(.horizontal, 4)
                        }
                    }
                    .padding(8)
                }
                .background(AppPalette.surfaceSubtle)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(AppPalette.surfaceSubtle)
    }

    private func sidebarSectionTitle(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.caption2.weight(.semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.top, 12)
            .padding(.bottom, 3)
    }

    private func navigationRow<Content: View>(
        _ target: AgentGroupChatWorkspaceDestination,
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(
                destination == target ? Color.accentColor.opacity(0.12) : Color.clear,
                in: RoundedRectangle(cornerRadius: 8)
            )
            .contentShape(Rectangle())
            .onTapGesture { destination = target }
            .accessibilityAddTraits(.isButton)
            .accessibilityAddTraits(destination == target ? .isSelected : [])
            .accessibilityAction { destination = target }
    }

    @ViewBuilder
    private var detail: some View {
        if !viewModel.hasLoadedInitialSnapshot {
            ContentUnavailableView {
                Label("正在加载 Agent", systemImage: "person.3.sequence.fill")
            } description: {
                ProgressView()
                    .controlSize(.small)
            }
        } else if destination == .agents {
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
        } else if destination == .artifacts {
            AgentArtifactLibraryView(ownerUserID: ownerUserID, service: service)
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

@MainActor
extension AppModel {
    func agentWorkspaceViewModel(ownerUserID: String) -> AgentGroupChatWorkspaceViewModel {
        if let existing = agentWorkspaceViewModels[ownerUserID] {
            return existing
        }
        let created = AgentGroupChatWorkspaceViewModel(
            ownerUserID: ownerUserID,
            service: agentGroupChatService,
            scheduler: agentGroupChatScheduler,
            builderService: agentGroupChatBuilderService
        )
        agentWorkspaceViewModels[ownerUserID] = created
        return created
    }
}
