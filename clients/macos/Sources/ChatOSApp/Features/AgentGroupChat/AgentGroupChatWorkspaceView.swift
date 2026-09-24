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
    case remoteArtifacts
    case direct(String)
    case room(String)
}

struct AgentGroupChatWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: AgentGroupChatWorkspaceViewModel
    @State private var showsCreateTeam = false
    @State private var destination: AgentGroupChatWorkspaceDestination = .agents
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
                                .foregroundStyle(AppPalette.ai)
                        }
                        .padding(.vertical, 4)
                        .tag(AgentGroupChatWorkspaceDestination.agents)

                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("云端 Agent 文档")
                                    .font(.body.weight(.medium))
                                Text("跨设备发现与预览")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "icloud.and.arrow.down")
                                .foregroundStyle(AppPalette.ai)
                        }
                        .padding(.vertical, 4)
                        .tag(AgentGroupChatWorkspaceDestination.remoteArtifacts)
                    }

                    Section("团队") {
                        if viewModel.rooms.isEmpty {
                            Text("还没有项目团队")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(viewModel.rooms.agentPage(index: teamPage, size: teamPageSize)) { room in
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
                                .padding(.vertical, 4)
                                .tag(AgentGroupChatWorkspaceDestination.room(room.id))
                            }
                            AgentListPaginationBar(
                                totalCount: viewModel.rooms.count,
                                page: $teamPage,
                                pageSize: $teamPageSize,
                                compact: true
                            )
                            .listRowSeparator(.hidden)
                        }
                    }


                    Section("私聊") {
                        if viewModel.directConversations.isEmpty {
                            Text("还没有私聊")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(
                                viewModel.directConversations.agentPage(
                                    index: directPage,
                                    size: directPageSize
                                )
                            ) { conversation in
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
                                .contentShape(Rectangle())
                                .padding(.vertical, 4)
                                .tag(AgentGroupChatWorkspaceDestination.direct(conversation.id))
                            }
                            AgentListPaginationBar(
                                totalCount: viewModel.directConversations.count,
                                page: $directPage,
                                pageSize: $directPageSize,
                                compact: true
                            )
                            .listRowSeparator(.hidden)
                        }
                    }
                }
                .listStyle(.plain)
                .scrollContentBackground(.hidden)
                .background(AppPalette.surfaceSubtle)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(AppPalette.surfaceSubtle)
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
        } else if destination == .remoteArtifacts {
            AgentRemoteArtifactLibraryView(service: service)
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
