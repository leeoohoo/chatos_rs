import ChatOSConnector
import ChatOSCore
import SwiftUI

struct ProjectAgentGroupChatView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: AgentGroupChatViewModel
    @State private var showsCreateRoom = false
    @State private var showsCreateAgent = false
    @State private var showsAddExistingAgent = false
    @State private var showsAgentBuilder = false
    @State private var showsStopAllConfirmation = false
    @State private var editingMember: AgentGroupChatViewModel.MemberPresentation?

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
                if !viewModel.pendingProposals.isEmpty
                    || !viewModel.pendingRemovalProposals.isEmpty
                    || !viewModel.pendingTeamProposals.isEmpty
                    || !viewModel.pendingMembershipProposals.isEmpty {
                    Divider()
                    pendingProposals
                }
                Divider()
                transcript
                Divider()
                composer
            }
            Divider()
            memberSidebar
                .frame(width: 230)
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
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if viewModel.messages.isEmpty {
                        ContentUnavailableView(
                            "还没有消息",
                            systemImage: "bubble.left.and.bubble.right",
                            description: Text("创建 Agent 后，通过 @ 提及开始协作。")
                        )
                        .padding(.top, 70)
                    }
                    ForEach(viewModel.messages) { message in
                        messageRow(message)
                            .id(message.id)
                    }
                }
                .padding(18)
            }
            .onChange(of: viewModel.messages.count) {
                guard let id = viewModel.messages.last?.id else { return }
                withAnimation { proxy.scrollTo(id, anchor: .bottom) }
            }
        }
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
                    Text(message.content)
                        .appFont(.body)
                        .textSelection(.enabled)
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
                Button {
                    editingMember = item
                } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Image(systemName: "person.crop.circle.fill")
                                .foregroundStyle(.tint)
                            Text(item.profile?.draft.name ?? item.member.agentID)
                                .appFont(.body).fontWeight(.medium)
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                        Text(item.member.draft.role)
                            .appFont(.caption).foregroundStyle(.secondary)
                        if viewModel.room?.defaultAgentID == item.member.agentID {
                            Text("默认 Agent")
                                .appFont(.caption2).foregroundStyle(.tint)
                        }
                        if viewModel.room?.projectManagerAgentID == item.member.agentID {
                            Text("项目经理")
                                .appFont(.caption2).foregroundStyle(.green)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .padding(.vertical, 5)
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
                    showsCreateAgent = true
                }
                Button("Agent Builder", systemImage: "sparkles") {
                    showsAgentBuilder = true
                }
            } label: {
                Label("创建新 Agent", systemImage: "plus")
            }
            .buttonStyle(.bordered)
            .frame(maxWidth: .infinity)
        }
        .padding(14)
        .background(Color(nsColor: .controlBackgroundColor))
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
