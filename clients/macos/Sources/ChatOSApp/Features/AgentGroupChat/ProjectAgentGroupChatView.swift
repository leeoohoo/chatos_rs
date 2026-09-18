import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI

private enum AgentTeamSection: String, CaseIterable, Identifiable {
    case chat = "聊天"
    case tasks = "任务"
    case assets = "共享资产"
    case runs = "运行"

    var id: String { rawValue }
}

struct ProjectAgentGroupChatView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: AgentGroupChatViewModel
    @State private var showsCreateRoom = false
    @State private var showsCreateAgent = false
    @State private var showsAddExistingAgent = false
    @State private var showsAgentBuilder = false
    @State private var showsStopAllConfirmation = false
    @State private var editingMember: AgentGroupChatViewModel.MemberPresentation?
    @State private var selectedSection: AgentTeamSection = .chat
    @State private var selectedRunAgentID: String?
    @State private var editingAsset: LocalAgentTeamAsset?
    @State private var historyAsset: LocalAgentTeamAsset?
    @State private var inspectingRun: LocalAgentGroupChatRun?
    @State private var showsAssetEditor = false

    init(
        projectID: String,
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        projectsService: NativeLocalProjectsService
    ) {
        _viewModel = StateObject(
            wrappedValue: AgentGroupChatViewModel(
                projectID: projectID,
                ownerUserID: ownerUserID,
                service: service,
                scheduler: scheduler,
                builderService: builderService,
                projectsService: projectsService
            )
        )
    }

    var body: some View {
        Group {
            if viewModel.isLoading, viewModel.room == nil {
                ProgressView("正在读取本地 Agent 群聊…")
            } else if viewModel.room == nil {
                emptyRoom
            } else {
                roomContent
            }
        }
        .task { await viewModel.activate() }
        .sheet(isPresented: $showsCreateRoom) {
            CreateAgentRoomSheet(viewModel: viewModel)
        }
        .sheet(isPresented: $showsCreateAgent) {
            CreateLocalAgentSheet(viewModel: viewModel)
        }
        .sheet(isPresented: $showsAddExistingAgent) {
            InviteExistingAgentSheet(viewModel: viewModel)
        }
        .sheet(isPresented: $showsAgentBuilder) {
            LocalAgentBuilderSheet(viewModel: viewModel)
        }
        .sheet(item: $editingMember) { item in
            EditLocalAgentSheet(viewModel: viewModel, item: item)
        }
        .sheet(isPresented: $showsAssetEditor) {
            TeamAssetEditorSheet(viewModel: viewModel, asset: editingAsset)
        }
        .sheet(item: $historyAsset) { asset in
            TeamAssetHistorySheet(viewModel: viewModel, asset: asset)
        }
        .sheet(item: $inspectingRun) { run in
            TeamRunInspectorSheet(
                run: run,
                delivery: viewModel.recentRunDeliveries[run.id],
                agentName: viewModel.profilesByID[run.context.agentID]?.draft.name ?? "Agent"
            )
        }
        .alert(
            "Agent 群聊错误",
            isPresented: Binding(
                get: { viewModel.errorMessage != nil },
                set: { if !$0 { viewModel.errorMessage = nil } }
            )
        ) {
            Button("好", role: .cancel) { viewModel.errorMessage = nil }
        } message: {
            Text(viewModel.errorMessage ?? "")
        }
        .confirmationDialog(
            "停止当前项目的全部 Agent？",
            isPresented: $showsStopAllConfirmation,
            titleVisibility: .visible
        ) {
            Button("停止全部 Agent", role: .destructive) {
                Task { await viewModel.stopAllAgents() }
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("正在运行的 delivery 会标记失败，尚未开始的 delivery 会取消；Run 检查点和事件会保留。")
        }
    }

    private var emptyRoom: some View {
        ContentUnavailableView {
            Label("创建项目 Agent 群聊", systemImage: "person.3.sequence.fill")
        } description: {
            Text("群聊、消息和 Agent 调度保存在本机。每个 Agent 使用独立 Memory。")
        } actions: {
            Button("创建群聊") { showsCreateRoom = true }
                .buttonStyle(.borderedProminent)
        }
    }

    private var roomContent: some View {
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                roomHeader
                Divider()
                Picker("团队区域", selection: $selectedSection) {
                    ForEach(AgentTeamSection.allCases) { section in
                        Text(section.rawValue).tag(section)
                    }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                Divider()
                workspaceContent
            }
            Divider()
            memberSidebar
                .frame(width: 230)
        }
    }

    @ViewBuilder
    private var workspaceContent: some View {
        switch selectedSection {
        case .chat:
            if !viewModel.pendingProposals.isEmpty
                || !viewModel.pendingRemovalProposals.isEmpty
                || !viewModel.pendingTeamProposals.isEmpty
                || !viewModel.pendingMembershipProposals.isEmpty {
                pendingProposals
                Divider()
            }
            transcript
            Divider()
            composer
        case .tasks:
            TeamTodoBoardView(
                todos: viewModel.teamTodos,
                profilesByID: viewModel.profilesByID
            )
        case .assets:
            TeamAssetsView(
                assets: viewModel.teamAssets,
                onCreate: {
                    editingAsset = nil
                    showsAssetEditor = true
                },
                onEdit: { asset in
                    editingAsset = asset
                    showsAssetEditor = true
                },
                onHistory: { asset in
                    historyAsset = asset
                },
                onArchive: { asset in
                    Task { await viewModel.archiveTeamAsset(asset) }
                }
            )
        case .runs:
            TeamRunsView(
                runs: viewModel.recentRuns,
                profilesByID: viewModel.profilesByID,
                deliveriesByRunID: viewModel.recentRunDeliveries,
                selectedAgentID: selectedRunAgentID,
                onSelectAgent: { selectedRunAgentID = $0 },
                onInspect: { inspectingRun = $0 }
            )
        }
    }

    private var pendingProposals: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Agent 提交了待确认提案", systemImage: "checklist")
                .appFont(.caption)
                .fontWeight(.semibold)
            ForEach(viewModel.pendingProposals) { proposal in
                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("\(proposal.draft.name) · \(proposal.draft.role)")
                            .appFont(.body)
                            .fontWeight(.medium)
                        Text("职业：\(profession(proposal.draft.professionKey)?.label ?? proposal.draft.professionKey)")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                        Text("思考等级：\(proposal.draft.thinkingLevel ?? "跟随模型默认")")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                        if !proposal.draft.responsibility.isEmpty {
                            Text(proposal.draft.responsibility)
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                        Text("由 \(viewModel.profilesByID[proposal.proposerAgentID]?.draft.name ?? proposal.proposerAgentID) 提议；确认时会重新校验模型和本机 Plugin。")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.proposalActionIDs.contains(proposal.id) {
                        ProgressView().controlSize(.small)
                    } else {
                        Button("拒绝", role: .destructive) {
                            Task { await viewModel.rejectProposal(proposal) }
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        Button("确认创建") {
                            Task { await viewModel.approveProposal(proposal) }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(.background.opacity(0.7), in: RoundedRectangle(cornerRadius: 9))
            }
            ForEach(viewModel.pendingRemovalProposals) { proposal in
                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("移出团队 · \(viewModel.profilesByID[proposal.draft.targetAgentID]?.draft.name ?? proposal.draft.targetAgentID)")
                            .appFont(.body)
                            .fontWeight(.medium)
                        Text(proposal.draft.reason)
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                        if !proposal.draft.handoffPlan.isEmpty {
                            Text("交接：\(proposal.draft.handoffPlan)")
                                .appFont(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                        Text("只会移出当前项目团队；Agent、独立 Memory 和其他团队关系都会保留。")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.removalProposalActionIDs.contains(proposal.id) {
                        ProgressView().controlSize(.small)
                    } else {
                        Button("拒绝", role: .destructive) {
                            Task { await viewModel.rejectRemovalProposal(proposal) }
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        Button("确认移出", role: .destructive) {
                            Task { await viewModel.approveRemovalProposal(proposal) }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(.background.opacity(0.7), in: RoundedRectangle(cornerRadius: 9))
            }
            ForEach(viewModel.pendingMembershipProposals) { proposal in
                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        let agentName = viewModel.profilesByID[
                            proposal.draft.targetAgentID
                        ]?.draft.name ?? "Agent"
                        let teamName = viewModel.teamsByID[
                            proposal.draft.targetTeamRoomID
                        ]?.draft.name ?? "项目团队"
                        Text("邀请现有 Agent · \(agentName)")
                            .appFont(.body)
                            .fontWeight(.medium)
                        Text("加入 \(teamName)，职责：\(proposal.draft.role)")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                        if !proposal.draft.responsibility.isEmpty {
                            Text(proposal.draft.responsibility)
                                .appFont(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                        Text("确认后只新增这个团队的成员关系；Agent 的其他团队、私聊和独立 Memory 不受影响。")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.membershipProposalActionIDs.contains(proposal.id) {
                        ProgressView().controlSize(.small)
                    } else {
                        Button("拒绝", role: .destructive) {
                            Task { await viewModel.rejectMembershipProposal(proposal) }
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        Button("确认加入团队") {
                            Task { await viewModel.approveMembershipProposal(proposal) }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(.background.opacity(0.7), in: RoundedRectangle(cornerRadius: 9))
            }
            ForEach(viewModel.pendingTeamProposals) { proposal in
                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(proposal.draft.newProjectName == nil
                            ? "为已有项目创建团队"
                            : "新建项目并创建团队")
                            .appFont(.body)
                            .fontWeight(.medium)
                        if let newProjectName = proposal.draft.newProjectName {
                            Text("项目：\(newProjectName)")
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                            if let key = proposal.draft.newProjectTypeKey,
                               let type = projectType(key) {
                                Text("类型：\(type.label)")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        } else if let projectID = proposal.draft.existingProjectID {
                            Text("项目：\(model.workspaceProjects.first(where: { $0.id == projectID })?.name ?? "本地项目")")
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                        Text("团队：\(proposal.draft.teamName)")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                        Text(proposal.draft.newProjectName == nil
                            ? "真实项目 ID 由客户端内部透传；Agent 不会看到它。"
                            : "确认后 ChatOS 会在默认工作区新建项目，并用生成的 ID 绑定团队。")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.teamProposalActionIDs.contains(proposal.id) {
                        ProgressView().controlSize(.small)
                    } else {
                        Button("拒绝", role: .destructive) {
                            Task { await viewModel.rejectTeamProposal(proposal) }
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        Button(proposal.draft.newProjectName == nil ? "确认创建团队" : "确认创建项目和团队") {
                            Task {
                                if let project = await viewModel.approveTeamProposal(proposal) {
                                    model.registerCreatedProject(project)
                                }
                            }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(.background.opacity(0.7), in: RoundedRectangle(cornerRadius: 9))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color.accentColor.opacity(0.07))
    }

    private func profession(_ key: String) -> LocalAgentProfessionDefinition? {
        guard let owner = model.localProjectOwnerUserID else { return nil }
        return model.agentSkillLibrary.profession(ownerUserID: owner, key: key)
    }

    private func projectType(_ key: String) -> LocalProjectTypeDefinition? {
        guard let owner = model.localProjectOwnerUserID else { return nil }
        return model.agentSkillLibrary.projectType(ownerUserID: owner, key: key)
    }

    private var roomHeader: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(viewModel.room?.draft.name ?? "Agent 群聊")
                    .appFont(.headline)
                if let goal = viewModel.room?.draft.goal, !goal.isEmpty {
                    Text(goal).appFont(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer()
            Text("本地")
                .appFont(.caption2)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(.quaternary, in: Capsule())
            if viewModel.isRunningAgents {
                Button {
                    Task { await viewModel.pauseAgents() }
                } label: {
                    if viewModel.isPausingAgents {
                        ProgressView().controlSize(.small)
                    } else {
                        Label("暂停", systemImage: "pause.fill")
                    }
                }
                .buttonStyle(.bordered)
                .disabled(viewModel.isPausingAgents || viewModel.isStoppingAgents)
            }
            if viewModel.isRunningAgents || !viewModel.interruptedRuns.isEmpty {
                Button(role: .destructive) {
                    showsStopAllConfirmation = true
                } label: {
                    if viewModel.isStoppingAgents {
                        ProgressView().controlSize(.small)
                    } else {
                        Label("停止全部", systemImage: "stop.fill")
                    }
                }
                .buttonStyle(.bordered)
                .disabled(viewModel.isStoppingAgents)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var transcript: some View {
        AgentChatTimelineView(
            items: viewModel.messages,
            isInitialContentReady: !viewModel.isLoading,
            hasOlderItems: viewModel.hasOlderMessages,
            isLoadingOlderItems: viewModel.isLoadingOlderMessages,
            scrollToLatestRequest: viewModel.scrollToLatestRequest,
            loadOlderItems: { await viewModel.loadOlderMessages() },
            rowContent: { message in messageRow(message) },
            emptyContent: {
                ContentUnavailableView(
                    "还没有消息",
                    systemImage: "bubble.left.and.bubble.right",
                    description: Text("创建 Agent 后，通过 @ 提及开始协作。")
                )
                .padding(.top, 70)
            }
        )
    }

    private func messageRow(_ message: ProjectAgentMessage) -> some View {
        let isHuman = message.senderKind == .human
        return HStack {
            if isHuman { Spacer(minLength: 80) }
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Text(viewModel.displayName(senderID: message.senderID, kind: message.senderKind))
                        .appFont(.caption).fontWeight(.semibold)
                    if message.hopCount > 0 {
                        Text("第 \(message.hopCount) 跳")
                            .appFont(.caption2).foregroundStyle(.secondary)
                    }
                }
                if !message.content.isEmpty {
                    MarkdownDocumentView(markdown: message.content)
                }
                if !message.attachmentItems.isEmpty {
                    AgentMessageAttachmentChips(
                        attachments: message.attachmentItems,
                        dataByID: viewModel.attachmentDataByID
                    )
                }
                if !message.mentionedAgentIDs.isEmpty {
                    Text(message.mentionedAgentIDs.compactMap { id in
                        guard let name = viewModel.profilesByID[id]?.draft.name else { return nil }
                        return "@\(name)"
                    }.joined(separator: "  "))
                    .appFont(.caption)
                    .foregroundStyle(.tint)
                }
            }
            .padding(12)
            .background(
                isHuman ? Color.accentColor.opacity(0.12) : Color.secondary.opacity(0.10),
                in: RoundedRectangle(cornerRadius: 12)
            )
            if !isHuman { Spacer(minLength: 80) }
        }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 8) {
            if viewModel.isRunningAgents {
                HStack(spacing: 6) {
                    ProgressView().controlSize(.small)
                    Text("本地 Agent 正在处理群聊…")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            if !viewModel.selectedMentionAgentIDs.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(viewModel.selectedMentionAgentIDs.sorted(), id: \.self) { id in
                            Button {
                                viewModel.toggleMention(agentID: id)
                            } label: {
                                Text("@\(viewModel.profilesByID[id]?.draft.name ?? id)  ×")
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                    }
                }
            }
            AgentChatComposerView(
                text: $viewModel.draftMessage,
                attachments: $viewModel.attachments,
                attachmentError: $viewModel.attachmentError,
                isSending: viewModel.isSending,
                placeholder: "输入消息；不选择 @ 时交给默认 Agent，也可粘贴图片、文档和长文本…",
                mentionCandidates: mentionCandidates,
                onMentionSelected: { viewModel.selectMention(agentID: $0) },
                onSend: { Task { await viewModel.sendMessage() } }
            ) {
                Menu {
                    if viewModel.activeMembers.isEmpty {
                        Text("先创建 Agent")
                    }
                    ForEach(viewModel.activeMembers) { item in
                        Button {
                            viewModel.toggleMention(agentID: item.member.agentID)
                        } label: {
                            Label(
                                item.profile?.draft.name ?? item.member.agentID,
                                systemImage: viewModel.selectedMentionAgentIDs.contains(item.member.agentID)
                                    ? "checkmark.circle.fill" : "circle"
                            )
                        }
                    }
                } label: {
                    Image(systemName: "at")
                }
                .menuStyle(.borderlessButton)
                .disabled(viewModel.activeMembers.isEmpty)
                .help("选择要 @ 的 Agent；不选择时交给默认 Agent")
            }
        }
        .padding(12)
        .background(.bar)
    }

    private var mentionCandidates: [AgentChatMentionCandidate] {
        viewModel.activeMembers.compactMap { item in
            guard let profile = item.profile,
                  !viewModel.selectedMentionAgentIDs.contains(item.member.agentID) else {
                return nil
            }
            return AgentChatMentionCandidate(
                id: item.member.agentID,
                name: profile.draft.name,
                subtitle: profession(profile.draft.professionKey)?.label
            )
        }
    }

    private var memberSidebar: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("成员").appFont(.headline)
                Spacer()
                Text("\(viewModel.members.count)")
                    .appFont(.caption).foregroundStyle(.secondary)
            }
            if viewModel.activeMembers.isEmpty {
                Text("还没有 Agent。创建第一个成员后，它会成为默认 Agent。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            if !viewModel.activeMembers.isEmpty,
               viewModel.room?.projectManagerAgentID == nil {
                Label("尚未指定项目经理，团队任务板暂不可创建任务。", systemImage: "exclamationmark.triangle")
                    .appFont(.caption)
                    .foregroundStyle(.orange)
            }
            ForEach(viewModel.activeMembers) { item in
                HStack(spacing: 8) {
                    Button {
                        selectedRunAgentID = item.member.agentID
                        selectedSection = .runs
                    } label: {
                        memberSummary(item)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .help("查看此 Agent 的运行情况")

                    Button {
                        Task {
                            if await viewModel.prepareAgentEditor() {
                                editingMember = item
                            }
                        }
                    } label: {
                        if viewModel.isLoadingModels {
                            ProgressView()
                                .controlSize(.small)
                                .frame(width: 24, height: 24)
                        } else {
                            Image(systemName: "slider.horizontal.3")
                                .frame(width: 24, height: 24)
                        }
                    }
                    .buttonStyle(.borderless)
                    .help("编辑 Agent")
                    .disabled(viewModel.isLoadingModels)
                }
                .padding(9)
                .background(
                    selectedRunAgentID == item.member.agentID && selectedSection == .runs
                        ? Color.accentColor.opacity(0.12)
                        : Color.clear,
                    in: RoundedRectangle(cornerRadius: 9)
                )
                .overlay {
                    if selectedRunAgentID == item.member.agentID && selectedSection == .runs {
                        RoundedRectangle(cornerRadius: 9)
                            .stroke(Color.accentColor.opacity(0.35), lineWidth: 1)
                    }
                }
            }
            Spacer()
            Button {
                showsAddExistingAgent = true
            } label: {
                Label("邀请 Agent", systemImage: "person.crop.circle.badge.plus")
            }
            .buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity)
            Menu {
                Button("手动创建", systemImage: "square.and.pencil") {
                    Task {
                        if await viewModel.prepareAgentEditor() {
                            showsCreateAgent = true
                        }
                    }
                }
                Button("Agent Builder", systemImage: "sparkles") {
                    Task {
                        if await viewModel.prepareAgentEditor() {
                            showsAgentBuilder = true
                        }
                    }
                }
            } label: {
                if viewModel.isLoadingModels {
                    ProgressView().controlSize(.small)
                } else {
                    Label("创建新 Agent", systemImage: "plus")
                }
            }
            .buttonStyle(.bordered)
            .frame(maxWidth: .infinity)
            .disabled(viewModel.isLoadingModels)
        }
        .padding(14)
        .background(Color(nsColor: .controlBackgroundColor))
        .onChange(of: viewModel.activeMembers.map(\.member.agentID)) { _, agentIDs in
            guard let selectedRunAgentID, !agentIDs.contains(selectedRunAgentID) else { return }
            self.selectedRunAgentID = nil
        }
    }

    private func memberSummary(_ item: AgentGroupChatViewModel.MemberPresentation) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Image(systemName: "person.crop.circle.fill")
                    .foregroundStyle(.tint)
                Text(item.profile?.draft.name ?? item.member.agentID)
                    .appFont(.body).fontWeight(.medium)
                Spacer()
                Image(systemName: "waveform.path.ecg")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Text(item.member.draft.role)
                .appFont(.caption).foregroundStyle(.secondary)
            HStack(spacing: 7) {
                if viewModel.room?.defaultAgentID == item.member.agentID {
                    Text("默认 Agent")
                        .appFont(.caption2).foregroundStyle(.tint)
                }
                if viewModel.room?.projectManagerAgentID == item.member.agentID {
                    Text("项目经理")
                        .appFont(.caption2).foregroundStyle(.green)
                }
            }
        }
    }
}

private struct TeamTodoBoardView: View {
    let todos: [LocalAgentTodo]
    let profilesByID: [String: LocalAgentProfile]

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                if todos.isEmpty {
                    ContentUnavailableView(
                        "还没有团队任务",
                        systemImage: "checklist",
                        description: Text("项目经理创建的任务会显示在这里。")
                    )
                    .padding(.top, 70)
                }
                ForEach(todos) { todo in
                    VStack(alignment: .leading, spacing: 10) {
                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(todo.title).appFont(.headline)
                                Text(profilesByID[todo.agentID]?.draft.name ?? "Agent")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Text(todoStatusLabel(todo.status))
                                .appFont(.caption2.weight(.semibold))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(todoStatusColor(todo.status).opacity(0.14), in: Capsule())
                                .foregroundStyle(todoStatusColor(todo.status))
                        }
                        Text(todo.executionContract.objective)
                            .appFont(.body)
                            .textSelection(.enabled)
                        if !todo.executionContract.scope.isEmpty {
                            LabeledContent("范围", value: todo.executionContract.scope)
                                .appFont(.caption)
                        }
                        contractList("交付物", todo.executionContract.expectedOutputs)
                        contractList("验收条件", todo.executionContract.acceptanceCriteria)
                        if !todo.executionContract.constraints.isEmpty {
                            contractList("约束", todo.executionContract.constraints)
                        }
                        if !todo.blockedReason.isEmpty {
                            Label(todo.blockedReason, systemImage: "exclamationmark.octagon")
                                .appFont(.caption)
                                .foregroundStyle(.orange)
                        }
                        if !todo.result.isEmpty {
                            Divider()
                            MarkdownDocumentView(markdown: todo.result)
                        }
                    }
                    .padding(14)
                    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
                    .overlay {
                        RoundedRectangle(cornerRadius: 12)
                            .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                    }
                }
            }
            .padding(18)
        }
    }

    private func contractList(_ title: String, _ values: [String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).appFont(.caption.weight(.semibold)).foregroundStyle(.secondary)
            ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                Text("• \(value)").appFont(.caption).textSelection(.enabled)
            }
        }
    }

    private func todoStatusLabel(_ status: LocalAgentTodoStatus) -> String {
        switch status {
        case .pending: "待执行"
        case .inProgress: "执行中"
        case .blocked: "已阻塞"
        case .completed: "已完成"
        case .cancelled: "已取消"
        }
    }

    private func todoStatusColor(_ status: LocalAgentTodoStatus) -> Color {
        switch status {
        case .pending: .secondary
        case .inProgress: .blue
        case .blocked: .orange
        case .completed: .green
        case .cancelled: .red
        }
    }
}

private struct TeamAssetsView: View {
    let assets: [LocalAgentTeamAsset]
    let onCreate: () -> Void
    let onEdit: (LocalAgentTeamAsset) -> Void
    let onHistory: (LocalAgentTeamAsset) -> Void
    let onArchive: (LocalAgentTeamAsset) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("团队共享资产").appFont(.headline)
                    Text("任务启动时会固定读取当时的 revision。")
                        .appFont(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button(action: onCreate) {
                    Label("新建资产", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
            }
            .padding(16)
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if assets.isEmpty {
                        ContentUnavailableView(
                            "还没有共享资产",
                            systemImage: "doc.richtext",
                            description: Text("在这里维护项目背景、进度、技术栈和架构决策。")
                        )
                        .padding(.top, 60)
                    }
                    ForEach(assets) { asset in
                        VStack(alignment: .leading, spacing: 10) {
                            HStack {
                                Text(asset.title).appFont(.headline)
                                Text(asset.category.displayName)
                                    .appFont(.caption2)
                                    .foregroundStyle(.secondary)
                                Spacer()
                                Text("r\(asset.revision)")
                                    .appFont(.caption.monospacedDigit())
                                    .foregroundStyle(.secondary)
                                Menu {
                                    Button("编辑") { onEdit(asset) }
                                    Button("版本历史") { onHistory(asset) }
                                    Button("归档", role: .destructive) { onArchive(asset) }
                                } label: {
                                    Image(systemName: "ellipsis.circle")
                                }
                                .menuStyle(.borderlessButton)
                            }
                            MarkdownDocumentView(markdown: asset.markdown)
                        }
                        .padding(14)
                        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
                    }
                }
                .padding(18)
            }
        }
    }
}

private struct TeamAssetHistorySheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    let asset: LocalAgentTeamAsset
    @State private var selectedRevision: Int?

    private var revisions: [LocalAgentTeamAssetRevision] {
        viewModel.teamAssetRevisions[asset.id] ?? []
    }

    private var selected: LocalAgentTeamAssetRevision? {
        if let selectedRevision,
           let revision = revisions.first(where: { $0.revision == selectedRevision }) {
            return revision
        }
        return revisions.first
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("版本历史").appFont(.title3.weight(.semibold))
                    Text(asset.title).appFont(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("完成") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            if viewModel.loadingTeamAssetRevisionIDs.contains(asset.id), revisions.isEmpty {
                ProgressView("正在读取版本历史…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if revisions.isEmpty {
                ContentUnavailableView(
                    "没有版本记录",
                    systemImage: "clock.arrow.circlepath",
                    description: Text("保存资产后，历史版本会显示在这里。")
                )
            } else {
                HStack(spacing: 0) {
                    List(revisions, selection: $selectedRevision) { revision in
                        VStack(alignment: .leading, spacing: 5) {
                            HStack {
                                Text("r\(revision.revision)")
                                    .appFont(.body.monospacedDigit().weight(.semibold))
                                if revision.revision == asset.revision {
                                    Text("当前")
                                        .appFont(.caption2.weight(.semibold))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(Color.accentColor.opacity(0.14), in: Capsule())
                                }
                            }
                            Text(revision.title).appFont(.caption).lineLimit(2)
                            Text(Self.timestamp(revision.createdAtUnixMs))
                                .appFont(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 4)
                        .tag(Optional(revision.revision))
                    }
                    .frame(width: 250)
                    Divider()
                    if let selected {
                        ScrollView {
                            VStack(alignment: .leading, spacing: 14) {
                                HStack(alignment: .firstTextBaseline) {
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(selected.title).appFont(.headline)
                                        Text("r\(selected.revision) · \(editorName(selected)) · \(Self.timestamp(selected.createdAtUnixMs))")
                                            .appFont(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                }
                                Divider()
                                MarkdownDocumentView(markdown: selected.markdown)
                            }
                            .padding(20)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
            }
        }
        .frame(minWidth: 920, minHeight: 620)
        .task {
            await viewModel.loadTeamAssetRevisions(asset)
            if selectedRevision == nil {
                selectedRevision = viewModel.teamAssetRevisions[asset.id]?.first?.revision
            }
        }
        .onChange(of: revisions.map(\.revision)) { _, values in
            if let selectedRevision, values.contains(selectedRevision) {
                return
            } else {
                selectedRevision = values.first
            }
        }
    }

    private func editorName(_ revision: LocalAgentTeamAssetRevision) -> String {
        guard let editorAgentID = revision.editorAgentID else { return "你" }
        return viewModel.profilesByID[editorAgentID]?.draft.name ?? "Agent"
    }

    private static func timestamp(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000)
            .formatted(date: .abbreviated, time: .shortened)
    }
}

private struct TeamAssetEditorSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    let asset: LocalAgentTeamAsset?
    @State private var category: LocalAgentTeamAssetCategory
    @State private var title: String
    @State private var markdown: String
    @State private var isSaving = false

    init(viewModel: AgentGroupChatViewModel, asset: LocalAgentTeamAsset?) {
        self.viewModel = viewModel
        self.asset = asset
        _category = State(initialValue: asset?.category ?? .overview)
        _title = State(initialValue: asset?.title ?? "")
        _markdown = State(initialValue: asset?.markdown ?? "# 项目上下文\n\n")
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(asset == nil ? "新建共享资产" : "编辑共享资产").appFont(.title3.weight(.semibold))
                Spacer()
                Button("取消") { dismiss() }
                Button("保存") {
                    isSaving = true
                    Task {
                        if await viewModel.saveTeamAsset(
                            existing: asset,
                            category: category,
                            title: title,
                            markdown: markdown
                        ) { dismiss() }
                        isSaving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isSaving)
            }
            .padding(16)
            Divider()
            HStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 10) {
                    Picker("分类", selection: $category) {
                        ForEach(LocalAgentTeamAssetCategory.allCases, id: \.self) {
                            Text($0.displayName).tag($0)
                        }
                    }
                    TextField("标题", text: $title)
                    TextEditor(text: $markdown)
                        .font(.body.monospaced())
                        .scrollContentBackground(.hidden)
                        .padding(8)
                        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
                }
                .padding(16)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                Divider()
                ScrollView {
                    MarkdownDocumentView(markdown: markdown)
                        .padding(18)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(minWidth: 900, minHeight: 620)
    }
}

private struct TeamRunsView: View {
    let runs: [LocalAgentGroupChatRun]
    let profilesByID: [String: LocalAgentProfile]
    let deliveriesByRunID: [UUID: ProjectAgentDelivery]
    let selectedAgentID: String?
    let onSelectAgent: (String?) -> Void
    let onInspect: (LocalAgentGroupChatRun) -> Void

    private var visibleRuns: [LocalAgentGroupChatRun] {
        guard let selectedAgentID else { return runs }
        return runs.filter { $0.context.agentID == selectedAgentID }
    }

    private var selectedAgentName: String? {
        guard let selectedAgentID else { return nil }
        return profilesByID[selectedAgentID]?.draft.name ?? "Agent"
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(selectedAgentName.map { "\($0) 的运行" } ?? "团队全部运行")
                        .appFont(.headline)
                    Text(selectedAgentID == nil
                         ? "按更新时间查看团队内所有 Run"
                         : "通讯与任务执行彼此独立，可同时运行")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if selectedAgentID != nil {
                    Button("查看全部") { onSelectAgent(nil) }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)

            if selectedAgentID != nil {
                HStack(spacing: 12) {
                    laneSummary(.manager)
                    laneSummary(.executor)
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 14)
            }

            Divider()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    if visibleRuns.isEmpty {
                        ContentUnavailableView(
                            selectedAgentID == nil ? "还没有运行记录" : "这个 Agent 还没有运行记录",
                            systemImage: "waveform.path.ecg",
                            description: Text("Agent 被唤醒后，通讯与任务执行 Run 会显示在这里。")
                        )
                        .padding(.top, 70)
                    }
                    ForEach(visibleRuns, id: \.id) { run in
                        VStack(alignment: .leading, spacing: 7) {
                            HStack {
                                Text(profilesByID[run.context.agentID]?.draft.name ?? "Agent")
                                    .appFont(.headline)
                                laneBadge(run.context.lane)
                                Spacer()
                                Text(run.checkpoint.status.displayName)
                                    .appFont(.caption.monospacedDigit())
                                    .foregroundStyle(statusColor(run.checkpoint.status))
                                Image(systemName: "chevron.right")
                                    .appFont(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                            if let delivery = deliveriesByRunID[run.id] {
                                Text("\(delivery.triggerKind.displayName) · \(run.events.count) 条事件 · \(run.checkpoint.modelCalls) 次模型调用")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            HStack(spacing: 12) {
                                Text(Self.timestamp(run.updatedAtUnixMs))
                                    .appFont(.caption2.monospacedDigit())
                                    .foregroundStyle(.secondary)
                                if let threadID = run.checkpoint.memory?.scope.threadID {
                                    Text("Memory \(threadID)")
                                        .appFont(.caption2.monospaced())
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                }
                            }
                            if let reason = run.checkpoint.stopReason, !reason.isEmpty {
                                Text(reason).appFont(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .padding(14)
                        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
                        .contentShape(Rectangle())
                        .onTapGesture { onInspect(run) }
                    }
                }
                .padding(18)
            }
        }
    }

    private func laneSummary(_ lane: LocalAgentRunLane) -> some View {
        let run = visibleRuns.first { $0.context.lane == lane }
        return VStack(alignment: .leading, spacing: 8) {
            HStack {
                laneBadge(lane)
                Spacer()
                if let run {
                    Circle()
                        .fill(statusColor(run.checkpoint.status))
                        .frame(width: 7, height: 7)
                    Text(run.checkpoint.status.displayName)
                        .appFont(.caption.weight(.medium))
                        .foregroundStyle(statusColor(run.checkpoint.status))
                } else {
                    Text("暂无记录")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            if let run {
                Text(deliveriesByRunID[run.id]?.triggerKind.displayName ?? "未知触发")
                    .appFont(.caption)
                Text("更新于 \(Self.timestamp(run.updatedAtUnixMs))")
                    .appFont(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
            } else {
                Text(lane == .manager ? "等待消息、心跳或任务状态唤醒" : "等待已启动的团队任务")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, minHeight: 82, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
        .contentShape(Rectangle())
        .onTapGesture {
            if let run { onInspect(run) }
        }
    }

    private func laneBadge(_ lane: LocalAgentRunLane) -> some View {
        Text(lane == .manager ? "通讯" : "任务执行")
            .appFont(.caption2.weight(.semibold))
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(
                (lane == .manager ? Color.blue : Color.purple).opacity(0.14),
                in: Capsule()
            )
    }

    private func statusColor(_ status: AgentRunCheckpoint.Status) -> Color {
        switch status {
        case .running: .blue
        case .completed: .green
        case .failed: .red
        case .paused, .needsReview, .limitReached: .orange
        case .ready: .secondary
        }
    }

    private static func timestamp(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000)
            .formatted(date: .abbreviated, time: .standard)
    }
}

private struct TeamRunInspectorSheet: View {
    @Environment(\.dismiss) private var dismiss
    let run: LocalAgentGroupChatRun
    let delivery: ProjectAgentDelivery?
    let agentName: String

    private struct ToolInspection: Identifiable {
        let id: String
        let name: String
        let arguments: String
        let status: String
        let outcome: AgentToolOutcome?
    }

    private var toolInspections: [ToolInspection] {
        run.checkpoint.messages.flatMap { message in
            message.toolCalls.map { call in
                let outcome = run.checkpoint.receipts[call.id]
                let status: String
                if call.id == run.checkpoint.inFlightCallID {
                    status = "执行中断"
                } else if run.checkpoint.pendingCalls.contains(where: { $0.id == call.id }) {
                    status = "等待执行"
                } else if outcome?.isError == true {
                    status = "失败"
                } else if outcome != nil {
                    status = "已完成"
                } else {
                    status = "未执行"
                }
                return ToolInspection(
                    id: call.id,
                    name: call.name,
                    arguments: Self.prettyJSON(call.arguments),
                    status: status,
                    outcome: outcome
                )
            }
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Run 检查器").appFont(.title3.weight(.semibold))
                    Text("\(agentName) · \(run.context.lane == .manager ? "通讯" : "任务执行")")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("完成") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            .padding(16)
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    runSummary
                    inspectorSection("工具调用") {
                        if toolInspections.isEmpty {
                            Text("本次 Run 没有工具调用。")
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(toolInspections) { tool in
                                toolInspection(tool)
                            }
                        }
                    }
                    inspectorSection("运行事件") {
                        if run.events.isEmpty {
                            Text("本次 Run 没有事件记录。")
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(run.events.reversed()) { event in
                                HStack(alignment: .top, spacing: 10) {
                                    Circle()
                                        .fill(eventColor(event.kind))
                                        .frame(width: 7, height: 7)
                                        .padding(.top, 6)
                                    VStack(alignment: .leading, spacing: 3) {
                                        HStack {
                                            Text(event.kind).appFont(.caption.weight(.semibold))
                                            Spacer()
                                            Text(event.date.formatted(date: .abbreviated, time: .standard))
                                                .appFont(.caption2.monospacedDigit())
                                                .foregroundStyle(.secondary)
                                        }
                                        Text(event.detail).appFont(.caption).textSelection(.enabled)
                                        Text("模型调用：\(event.modelCalls)")
                                            .appFont(.caption2)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                if event.id != run.events.first?.id { Divider() }
                            }
                        }
                    }
                }
                .padding(20)
            }
        }
        .frame(minWidth: 900, minHeight: 680)
    }

    private var runSummary: some View {
        inspectorSection("运行信息") {
            Grid(alignment: .leading, horizontalSpacing: 24, verticalSpacing: 9) {
                summaryRow("状态", run.checkpoint.status.displayName)
                summaryRow("触发", delivery?.triggerKind.displayName ?? "未知")
                summaryRow("Delivery", delivery?.status.displayName ?? "未知")
                summaryRow("Memory", run.context.lane == .manager ? "Agent 长期通讯" : "Todo 独立执行")
                summaryRow("模型配置", run.modelConfigID)
                summaryRow("模型调用", "\(run.checkpoint.modelCalls) / \(run.policy.maximumModelCalls)")
                summaryRow("运行耗时", String(format: "%.1f 秒", run.checkpoint.elapsedSeconds))
                summaryRow("开始时间", Self.timestamp(run.createdAtUnixMs))
                summaryRow("更新时间", Self.timestamp(run.updatedAtUnixMs))
            }
            if let reason = run.checkpoint.stopReason, !reason.isEmpty {
                Divider()
                LabeledContent("停止原因") {
                    Text(reason).textSelection(.enabled)
                }
                .appFont(.caption)
            }
            if let result = run.checkpoint.result, !result.isEmpty {
                Divider()
                VStack(alignment: .leading, spacing: 6) {
                    Text("结果").appFont(.caption.weight(.semibold))
                    MarkdownDocumentView(markdown: result)
                }
            }
        }
    }

    private func summaryRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label).appFont(.caption).foregroundStyle(.secondary)
            Text(value).appFont(.caption).textSelection(.enabled)
        }
    }

    private func toolInspection(_ tool: ToolInspection) -> some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                Text("参数").appFont(.caption.weight(.semibold))
                Text(tool.arguments)
                    .appFont(.caption.monospaced())
                    .textSelection(.enabled)
                if let outcome = tool.outcome {
                    Text("结果").appFont(.caption.weight(.semibold))
                    Text(outcome.content)
                        .appFont(.caption.monospaced())
                        .foregroundStyle(outcome.isError ? Color.red : Color.primary)
                        .textSelection(.enabled)
                }
            }
            .padding(.top, 8)
        } label: {
            HStack {
                Text(tool.name).appFont(.body.monospaced())
                Spacer()
                Text(tool.status)
                    .appFont(.caption2.weight(.semibold))
                    .foregroundStyle(tool.status == "失败" ? Color.red : Color.secondary)
            }
        }
        .padding(12)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
    }

    private func inspectorSection<Content: View>(
        _ title: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).appFont(.headline)
            content()
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
    }

    private func eventColor(_ kind: String) -> Color {
        if kind.contains("failed") || kind.contains("error") || kind == "stopped" { return .red }
        if kind.contains("completed") { return .green }
        if kind.contains("pause") || kind.contains("review") { return .orange }
        if kind.contains("tool") { return .purple }
        return .blue
    }

    private static func prettyJSON(_ raw: String) -> String {
        guard let data = raw.data(using: .utf8),
              let value = try? JSONSerialization.jsonObject(with: data),
              let formatted = try? JSONSerialization.data(
                withJSONObject: value,
                options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
              ) else { return raw }
        return String(decoding: formatted, as: UTF8.self)
    }

    private static func timestamp(_ unixMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(unixMs) / 1_000)
            .formatted(date: .abbreviated, time: .standard)
    }
}

private extension AgentRunCheckpoint.Status {
    var displayName: String {
        switch self {
        case .ready: "等待开始"
        case .running: "运行中"
        case .paused: "已暂停"
        case .completed: "已完成"
        case .failed: "失败"
        case .needsReview: "需要检查"
        case .limitReached: "达到限制"
        }
    }
}

private extension ProjectAgentDeliveryStatus {
    var displayName: String {
        switch self {
        case .pending: "等待处理"
        case .running: "处理中"
        case .completed: "已完成"
        case .failed: "失败"
        case .cancelled: "已取消"
        }
    }
}

private extension ProjectAgentDeliveryTriggerKind {
    var displayName: String {
        switch self {
        case .mention: "Human @消息"
        case .defaultAgent: "Human 未点名消息"
        case .agentMention: "Agent 消息"
        case .heartbeat: "主动巡检"
        case .todo: "Todo 执行"
        case .todoStatus: "Todo 状态变化"
        }
    }
}

private extension LocalAgentTeamAssetCategory {
    var displayName: String {
        switch self {
        case .overview: "项目背景"
        case .currentProgress: "整体进度"
        case .techStack: "技术栈"
        case .architecture: "架构"
        case .conventions: "工程规范"
        case .decision: "重要决策"
        case .reference: "参考资料"
        }
    }
}

private struct EditLocalAgentSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    let item: AgentGroupChatViewModel.MemberPresentation
    @State private var name: String
    @State private var role: String
    @State private var responsibility: String
    @State private var rolePrompt: String
    @State private var modelConfigID: String
    @State private var thinkingLevel: String
    @State private var shouldBeProjectManager: Bool
    @State private var isSaving = false

    init(
        viewModel: AgentGroupChatViewModel,
        item: AgentGroupChatViewModel.MemberPresentation
    ) {
        self.viewModel = viewModel
        self.item = item
        let profile = item.profile
        _name = State(initialValue: profile?.draft.name ?? item.member.agentID)
        _role = State(initialValue: item.member.draft.role)
        _responsibility = State(initialValue: item.member.draft.responsibility)
        _rolePrompt = State(initialValue: profile?.draft.rolePrompt ?? "")
        _modelConfigID = State(initialValue: profile?.draft.modelConfigID ?? "")
        _thinkingLevel = State(initialValue: profile?.draft.thinkingLevel ?? "")
        _shouldBeProjectManager = State(
            initialValue: viewModel.room?.projectManagerAgentID == item.member.agentID
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("编辑本地 Agent").font(.title2).fontWeight(.semibold)
            Form {
                TextField("名称", text: $name)
                TextField("当前项目角色", text: $role)
                TextField("当前项目职责", text: $responsibility, axis: .vertical)
                    .lineLimit(2...4)
                TextField("角色 Prompt", text: $rolePrompt, axis: .vertical)
                    .lineLimit(4...8)
                Picker("模型", selection: $modelConfigID) {
                    ForEach(viewModel.availableModels) { model in
                        Text("\(model.name) · \(model.modelName)").tag(model.id)
                    }
                }
                Picker("思考等级", selection: $thinkingLevel) {
                    Text(defaultThinkingLabel).tag("")
                    ForEach(selectedModel?.thinkingLevels ?? [], id: \.self) { level in
                        Text(level).tag(level)
                    }
                }
                .disabled(selectedModel?.supportsReasoning != true)
                if item.profile?.draft.professionKey == "project_manager" {
                    Toggle("设为当前团队的项目经理", isOn: $shouldBeProjectManager)
                        .disabled(viewModel.room?.projectManagerAgentID == item.member.agentID)
                    Text("项目经理独占团队任务板的创建、分配、优先级和依赖管理权限。")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if !viewModel.availableModels.contains(where: { $0.id == modelConfigID }) {
                    Text("原模型当前不可用，请选择新的模型后保存。")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
            }
            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("保存") {
                    isSaving = true
                    Task {
                        let saved = await viewModel.updateAgentMembership(
                            agentID: item.member.agentID,
                            name: name,
                            role: role,
                            responsibility: responsibility,
                            rolePrompt: rolePrompt,
                            modelConfigID: modelConfigID,
                            thinkingLevel: thinkingLevel
                        )
                        guard saved else {
                            isSaving = false
                            return
                        }
                        if shouldBeProjectManager,
                           viewModel.room?.projectManagerAgentID != item.member.agentID {
                            guard await viewModel.setProjectManager(
                                agentID: item.member.agentID
                            ) else {
                                isSaving = false
                                return
                            }
                        }
                        dismiss()
                        isSaving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    isSaving
                        || [name, role, rolePrompt, modelConfigID].contains {
                            $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        }
                )
            }
        }
        .padding(24)
        .frame(width: 560)
        .onChange(of: modelConfigID) { normalizeThinkingLevel() }
    }

    private var selectedModel: LocalAgentBuilderModelOption? {
        viewModel.availableModels.first(where: { $0.id == modelConfigID })
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
}

private struct InviteExistingAgentSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: AgentGroupChatViewModel
    @State private var selectedAgentID = ""
    @State private var isSaving = false

    private var availableAgents: [LocalAgentProfile] {
        let memberIDs = Set(viewModel.members.map(\.agentID))
        return viewModel.agents.filter { !memberIDs.contains($0.id) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("邀请 Agent")
                .font(.title2.weight(.semibold))
            Text("选择一个已有 Agent 加入当前团队。同一个 Agent 可以加入多个团队。")
                .font(.callout)
                .foregroundStyle(.secondary)

            if availableAgents.isEmpty {
                ContentUnavailableView(
                    "没有可邀请的 Agent",
                    systemImage: "person.crop.circle.badge.exclamationmark",
                    description: Text("所有已有 Agent 都已加入当前团队，或尚未创建 Agent。")
                )
            } else {
                ScrollView {
                    LazyVStack(spacing: 8) {
                        ForEach(availableAgents) { agent in
                            agentRow(agent)
                        }
                    }
                    .padding(1)
                }
                .frame(height: min(CGFloat(availableAgents.count) * 86, 360))
            }

            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("邀请") {
                    isSaving = true
                    Task {
                        if await viewModel.inviteAgent(agentID: selectedAgentID) { dismiss() }
                        isSaving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(isSaving || selectedAgentID.isEmpty || availableAgents.isEmpty)
            }
        }
        .padding(24)
        .frame(width: 620)
        .onAppear { selectDefaultAgent() }
    }

    private func agentRow(_ agent: LocalAgentProfile) -> some View {
        Button {
            selectedAgentID = agent.id
        } label: {
            HStack(spacing: 12) {
                Image(systemName: "person.crop.circle")
                    .font(.title2)
                    .foregroundStyle(agent.id == selectedAgentID ? Color.accentColor : .secondary)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Text(agent.draft.name).font(.headline)
                        Text(professionName(agent.draft.professionKey))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Text(agent.draft.description.isEmpty ? modelName(agent.draft.modelConfigID) : agent.draft.description)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer()
                if agent.id == selectedAgentID {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.tint)
                }
            }
            .padding(12)
            .contentShape(Rectangle())
            .background(
                agent.id == selectedAgentID
                    ? Color.accentColor.opacity(0.10)
                    : Color.secondary.opacity(0.06),
                in: RoundedRectangle(cornerRadius: 10)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(agent.id == selectedAgentID ? Color.accentColor : .clear, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }

    private func selectDefaultAgent() {
        guard selectedAgentID.isEmpty else { return }
        selectedAgentID = availableAgents.first?.id ?? ""
    }

    private func professionName(_ key: String) -> String {
        guard let owner = model.localProjectOwnerUserID else { return key }
        return model.agentSkillLibrary.profession(ownerUserID: owner, key: key)?.label ?? key
    }

    private func modelName(_ id: String) -> String {
        viewModel.availableModels.first(where: { $0.id == id })?.name ?? "已配置模型"
    }
}

private struct CreateAgentRoomSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    @State private var name = "项目 Agent 群聊"
    @State private var goal = ""
    @State private var projectManagerAgentID = ""
    @State private var isSaving = false

    private var projectManagerCandidates: [LocalAgentProfile] {
        viewModel.agents.filter {
            $0.status == .active && $0.draft.professionKey == "project_manager"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("创建本地 Agent 群聊").font(.title2).fontWeight(.semibold)
            TextField("群聊名称", text: $name)
            TextField("群聊目标（可选）", text: $goal, axis: .vertical).lineLimit(2...5)
            if projectManagerCandidates.isEmpty {
                Label("请先在 Agent 管理中创建一个职业为“项目经理”的 Agent。", systemImage: "person.crop.circle.badge.exclamationmark")
                    .font(.callout)
                    .foregroundStyle(.orange)
            } else {
                Picker("项目经理", selection: $projectManagerAgentID) {
                    ForEach(projectManagerCandidates) { agent in
                        Text(agent.draft.name).tag(agent.id)
                    }
                }
                Text("项目经理负责创建、分配和维护团队任务与前置依赖。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text("聊天记录和调度状态只保存在这台 Mac。")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("创建") {
                    isSaving = true
                    Task {
                        if await viewModel.createRoom(
                            name: name,
                            goal: goal,
                            projectManagerAgentID: projectManagerAgentID
                        ) { dismiss() }
                        isSaving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    isSaving
                        || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || projectManagerAgentID.isEmpty
                )
            }
        }
        .padding(24)
        .frame(width: 480)
        .onAppear {
            if projectManagerAgentID.isEmpty {
                projectManagerAgentID = projectManagerCandidates.first?.id ?? ""
            }
        }
    }
}

private struct CreateLocalAgentSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: AgentGroupChatViewModel
    @State private var name = ""
    @State private var role = ""
    @State private var responsibility = ""
    @State private var rolePrompt = ""
    @State private var modelConfigID = ""
    @State private var thinkingLevel = ""
    @State private var professionKey = LocalAgentSkillCatalog.legacyProfessionKey
    @State private var isSaving = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("创建本地 Agent").font(.title2).fontWeight(.semibold)
            Form {
                TextField("名称", text: $name)
                TextField("群聊角色", text: $role)
                TextField("职责说明", text: $responsibility, axis: .vertical).lineLimit(2...4)
                TextField("角色 Prompt", text: $rolePrompt, axis: .vertical).lineLimit(4...8)
                if viewModel.availableModels.isEmpty {
                    Text("没有已启用且配置了密钥的模型")
                        .foregroundStyle(.secondary)
                } else {
                    Picker("模型", selection: $modelConfigID) {
                        ForEach(viewModel.availableModels) { model in
                            Text("\(model.name) · \(model.modelName)").tag(model.id)
                        }
                    }
                    Picker("思考等级", selection: $thinkingLevel) {
                        Text(defaultThinkingLabel).tag("")
                        ForEach(selectedModel?.thinkingLevels ?? [], id: \.self) { level in
                            Text(level).tag(level)
                        }
                    }
                    .disabled(selectedModel?.supportsReasoning != true)
                }
                Picker("职业", selection: $professionKey) {
                    ForEach(professions) { profession in
                        Text("\(profession.categoryLabel) · \(profession.label)")
                            .tag(profession.key)
                    }
                }
                if let selected = professions.first(where: { $0.key == professionKey }) {
                    Text(selected.description).font(.caption).foregroundStyle(.secondary)
                }
            }
            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("创建并加入") {
                    isSaving = true
                    Task {
                        if await viewModel.createAgentAndJoin(
                            name: name,
                            role: role,
                            responsibility: responsibility,
                            rolePrompt: rolePrompt,
                            modelConfigID: modelConfigID,
                            thinkingLevel: thinkingLevel,
                            professionKey: professionKey
                        ) { dismiss() }
                        isSaving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    isSaving
                        || [name, role, rolePrompt, modelConfigID].contains {
                            $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        }
                )
            }
        }
        .padding(24)
        .frame(width: 560)
        .onAppear {
            if modelConfigID.isEmpty {
                modelConfigID = viewModel.availableModels.first?.id ?? ""
            }
            normalizeThinkingLevel()
        }
        .onChange(of: modelConfigID) { normalizeThinkingLevel() }
    }

    private var selectedModel: LocalAgentBuilderModelOption? {
        viewModel.availableModels.first(where: { $0.id == modelConfigID })
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

    private var professions: [LocalAgentProfessionDefinition] {
        guard let owner = model.localProjectOwnerUserID else {
            return LocalAgentSkillCatalog.professions
        }
        return model.agentSkillLibrary.professions(ownerUserID: owner)
    }
}

private struct LocalAgentBuilderSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: AgentGroupChatViewModel
    @State private var brief = ""
    @State private var builderModelConfigID = ""
    @State private var proposedDraft: LocalAgentDraft?
    @State private var isGenerating = false
    @State private var isCreating = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Label("Agent Builder", systemImage: "sparkles")
                    .font(.title2)
                    .fontWeight(.semibold)
                Spacer()
                Text("本地受控创建")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if let proposedDraft {
                draftConfirmation(proposedDraft)
            } else {
                builderRequest
            }

            HStack {
                Spacer()
                Button("取消") { dismiss() }
                if let proposedDraft {
                    Button("重新生成") { self.proposedDraft = nil }
                        .disabled(isCreating)
                    Button("确认创建并加入") {
                        isCreating = true
                        Task {
                            if await viewModel.confirmAgentDraft(proposedDraft) { dismiss() }
                            isCreating = false
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isCreating)
                } else {
                    Button("生成草案") {
                        isGenerating = true
                        Task {
                            proposedDraft = await viewModel.generateAgentDraft(
                                brief: brief,
                                builderModelConfigID: builderModelConfigID
                            )
                            isGenerating = false
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(
                        isGenerating
                            || brief.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                            || builderModelConfigID.isEmpty
                    )
                }
            }
        }
        .padding(24)
        .frame(width: 620)
        .onAppear {
            if builderModelConfigID.isEmpty {
                builderModelConfigID = viewModel.availableModels.first?.id ?? ""
            }
        }
    }

    private var builderRequest: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("描述希望新 Agent 承担的工作")
                .font(.subheadline.weight(.medium))
            TextField(
                "",
                text: $brief,
                axis: .vertical
            )
            .lineLimit(4...8)
            .textFieldStyle(.roundedBorder)
            .frame(maxWidth: .infinity)

            if viewModel.availableModels.isEmpty {
                Text("没有可供 Builder 使用的模型，请先配置并启用模型。")
                    .foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 7) {
                    Text("Builder 模型")
                        .font(.subheadline.weight(.medium))
                    Picker("", selection: $builderModelConfigID) {
                        ForEach(viewModel.availableModels) { model in
                            Text("\(model.name) · \(model.modelName)").tag(model.id)
                        }
                    }
                    .labelsHidden()
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            if isGenerating {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("正在生成可确认的 Agent 草案…")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private func draftConfirmation(_ draft: LocalAgentDraft) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Label("等待你的确认", systemImage: "checkmark.seal")
                    .font(.headline)
                draftField("名称", draft.name)
                draftField("群聊角色", draft.role)
                draftField("职责", draft.responsibility.isEmpty ? "未单独设置" : draft.responsibility)
                draftField(
                    "职业",
                    profession(draft.professionKey)?.label
                        ?? draft.professionKey
                )
                draftField("模型", modelName(draft.modelConfigID))
                draftField("创建理由", draft.rationale.isEmpty ? "未说明" : draft.rationale)
                VStack(alignment: .leading, spacing: 5) {
                    Text("角色 Prompt").font(.caption).foregroundStyle(.secondary)
                    Text(draft.rolePrompt)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(10)
                        .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
                }
                Text("点击确认前不会创建 Agent。确认时客户端会重新校验模型是否仍然可用。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(minHeight: 360, maxHeight: 560)
    }

    private func profession(_ key: String) -> LocalAgentProfessionDefinition? {
        guard let owner = model.localProjectOwnerUserID else { return nil }
        return model.agentSkillLibrary.profession(ownerUserID: owner, key: key)
    }

    private func draftField(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            Text(value).textSelection(.enabled)
        }
    }

    private func modelName(_ id: String) -> String {
        guard let model = viewModel.availableModels.first(where: { $0.id == id }) else {
            return id
        }
        return "\(model.name) · \(model.modelName)"
    }

}
