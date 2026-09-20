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
                        ownerUserID: message.ownerUserID,
                        roomID: message.roomID,
                        messageID: message.id,
                        creatorName: viewModel.displayName(
                            senderID: message.senderID,
                            kind: message.senderKind
                        ),
                        attachments: message.attachmentItems,
                        dataByID: viewModel.attachmentDataByID,
                        service: model.agentGroupChatService
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
