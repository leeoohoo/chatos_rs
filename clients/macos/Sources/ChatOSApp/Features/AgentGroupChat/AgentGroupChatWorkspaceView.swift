import ChatOSConnector
import ChatOSCore
import SwiftUI

@MainActor
private final class AgentGroupChatWorkspaceViewModel: ObservableObject {
    @Published private(set) var rooms: [ProjectAgentRoom] = []
    @Published var selectedRoomID: String?
    @Published private(set) var isLoading = false
    @Published private(set) var isCreating = false
    @Published var errorMessage: String?

    private let ownerUserID: String
    private let service: NativeAgentGroupChatService
    private var openedStore: SQLiteAgentGroupChatStore?

    init(ownerUserID: String, service: NativeAgentGroupChatService) {
        self.ownerUserID = ownerUserID
        self.service = service
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let store = try await resolveStore()
            let rooms = try await store.listRooms(ownerUserID: ownerUserID)
            self.rooms = rooms
            if let selectedRoomID, rooms.contains(where: { $0.id == selectedRoomID }) {
                self.selectedRoomID = selectedRoomID
            } else {
                self.selectedRoomID = rooms.first?.id
            }
            errorMessage = nil
        } catch {
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

    private let ownerUserID: String
    private let service: NativeAgentGroupChatService
    private let scheduler: LocalAgentGroupChatScheduler
    private let builderService: LocalAgentBuilderService
    private let pluginService: NativeLocalConnectorService

    init(
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        pluginService: NativeLocalConnectorService
    ) {
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
        self.builderService = builderService
        self.pluginService = pluginService
        _viewModel = StateObject(
            wrappedValue: AgentGroupChatWorkspaceViewModel(
                ownerUserID: ownerUserID,
                service: service
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
        .navigationTitle(model.localized("Agent 群聊", english: "Agent Group Chat"))
        .task { await viewModel.load() }
        .sheet(isPresented: $showsCreateTeam) {
            CreateAgentTeamSheet(
                projects: availableProjects,
                isCreating: viewModel.isCreating
            ) { projectID, name, goal in
                await viewModel.createRoom(projectID: projectID, name: name, goal: goal)
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
        viewModel.rooms.first { $0.id == viewModel.selectedRoomID }
    }

    private var teamList: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("团队")
                        .font(.headline)
                    Text("每个团队绑定一个项目")
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

            if viewModel.isLoading, viewModel.rooms.isEmpty {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if viewModel.rooms.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    Label("还没有 Agent 团队", systemImage: "person.3.sequence")
                        .font(.headline)
                    Text("选择一个项目，创建只在本机运行的 Agent 团队。")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Button("创建团队") { showsCreateTeam = true }
                        .buttonStyle(.borderedProminent)
                        .disabled(availableProjects.isEmpty)
                    Spacer(minLength: 0)
                }
                .padding(18)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else {
                List(selection: $viewModel.selectedRoomID) {
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
                        .tag(room.id)
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
        if let room = selectedRoom {
            ProjectAgentGroupChatView(
                projectID: room.projectID,
                ownerUserID: ownerUserID,
                service: service,
                scheduler: scheduler,
                builderService: builderService,
                pluginService: pluginService
            )
            .id(room.id)
        } else {
            ContentUnavailableView {
                Label("选择或创建 Agent 团队", systemImage: "person.3.sequence.fill")
            } description: {
                Text("团队必须绑定一个项目，群聊、Agent 和调度状态保存在本机。")
            }
        }
    }

    private func projectName(for projectID: String) -> String {
        model.projects.first(where: { $0.id == projectID })?.title
            ?? model.localized("项目已移除", english: "Project Removed")
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
