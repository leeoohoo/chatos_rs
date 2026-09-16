import ChatOSConnector
import ChatOSCore
import SwiftUI

struct ProjectAgentGroupChatView: View {
    @StateObject private var viewModel: AgentGroupChatViewModel
    @State private var showsCreateRoom = false
    @State private var showsCreateAgent = false
    @State private var showsAgentBuilder = false
    @State private var abandonDeliveryID: String?

    init(
        projectID: String,
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler,
        builderService: LocalAgentBuilderService,
        pluginService: NativeLocalConnectorService
    ) {
        _viewModel = StateObject(
            wrappedValue: AgentGroupChatViewModel(
                projectID: projectID,
                ownerUserID: ownerUserID,
                service: service,
                scheduler: scheduler,
                builderService: builderService,
                pluginService: pluginService
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
        .sheet(isPresented: $showsAgentBuilder) {
            LocalAgentBuilderSheet(viewModel: viewModel)
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
            "结束这个 Agent Run？",
            isPresented: Binding(
                get: { abandonDeliveryID != nil },
                set: { if !$0 { abandonDeliveryID = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("结束 Run", role: .destructive) {
                guard let deliveryID = abandonDeliveryID else { return }
                abandonDeliveryID = nil
                Task { await viewModel.abandonRun(deliveryID: deliveryID) }
            }
            Button("取消", role: .cancel) { abandonDeliveryID = nil }
        } message: {
            Text("该 delivery 会标记为失败并释放 Agent 队列；已保存的检查点和事件仍会保留。")
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
                if !viewModel.interruptedRuns.isEmpty {
                    Divider()
                    interruptedRuns
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

    private var interruptedRuns: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("检测到未完成的本地 Agent Run", systemImage: "exclamationmark.arrow.trianglehead.2.clockwise.rotate.90")
                .appFont(.caption)
                .fontWeight(.semibold)
            ForEach(viewModel.interruptedRuns) { item in
                HStack(spacing: 10) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(item.agentName) · \(item.statusText)")
                            .appFont(.caption)
                        if let reason = item.run.checkpoint.stopReason, !reason.isEmpty {
                            Text(reason)
                                .appFont(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                    }
                    Spacer()
                    let isActing = viewModel.runActionDeliveryIDs.contains(item.delivery.id)
                    if isActing {
                        ProgressView().controlSize(.small)
                    } else {
                        Button("恢复") {
                            Task { await viewModel.resumeRun(deliveryID: item.delivery.id) }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .disabled(viewModel.isRunningAgents)
                        Button("结束", role: .destructive) {
                            abandonDeliveryID = item.delivery.id
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .disabled(viewModel.isRunningAgents)
                    }
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color.orange.opacity(0.08))
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
            Menu {
                Button("手动创建", systemImage: "square.and.pencil") {
                    showsCreateAgent = true
                }
                Button("让 Agent Builder 创建", systemImage: "sparkles") {
                    showsAgentBuilder = true
                }
            } label: {
                Label("添加 Agent", systemImage: "person.badge.plus")
            }
            .buttonStyle(.bordered)
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
                Text(message.content)
                    .appFont(.body)
                    .textSelection(.enabled)
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
            HStack(alignment: .bottom, spacing: 10) {
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
                        .frame(width: 28, height: 28)
                }
                .menuStyle(.borderlessButton)
                .disabled(viewModel.activeMembers.isEmpty)

                TextField("输入消息；不选择 @ 时交给默认 Agent", text: $viewModel.draftMessage, axis: .vertical)
                    .textFieldStyle(.plain)
                    .lineLimit(1...6)
                    .onSubmit { Task { await viewModel.sendMessage() } }

                Button {
                    Task { await viewModel.sendMessage() }
                } label: {
                    if viewModel.isSending {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "arrow.up.circle.fill").font(.title2)
                    }
                }
                .buttonStyle(.plain)
                .disabled(
                    viewModel.isSending
                        || viewModel.draftMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                )
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
            ForEach(viewModel.activeMembers) { item in
                VStack(alignment: .leading, spacing: 3) {
                    HStack {
                        Image(systemName: "person.crop.circle.fill")
                            .foregroundStyle(.tint)
                        Text(item.profile?.draft.name ?? item.member.agentID)
                            .appFont(.body).fontWeight(.medium)
                    }
                    Text(item.member.draft.role)
                        .appFont(.caption).foregroundStyle(.secondary)
                    if viewModel.room?.defaultAgentID == item.member.agentID {
                        Text("默认 Agent")
                            .appFont(.caption2).foregroundStyle(.tint)
                    }
                }
                .padding(.vertical, 5)
            }
            Spacer()
            Menu {
                Button("手动创建", systemImage: "square.and.pencil") {
                    showsCreateAgent = true
                }
                Button("Agent Builder", systemImage: "sparkles") {
                    showsAgentBuilder = true
                }
            } label: {
                Label("添加 Agent", systemImage: "plus")
            }
                .buttonStyle(.borderedProminent)
                .frame(maxWidth: .infinity)
        }
        .padding(14)
        .background(Color(nsColor: .controlBackgroundColor))
    }
}

private struct CreateAgentRoomSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    @State private var name = "项目 Agent 群聊"
    @State private var goal = ""
    @State private var isSaving = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("创建本地 Agent 群聊").font(.title2).fontWeight(.semibold)
            TextField("群聊名称", text: $name)
            TextField("群聊目标（可选）", text: $goal, axis: .vertical).lineLimit(2...5)
            Text("聊天记录和调度状态只保存在这台 Mac。")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("取消") { dismiss() }
                Button("创建") {
                    isSaving = true
                    Task {
                        if await viewModel.createRoom(name: name, goal: goal) { dismiss() }
                        isSaving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(isSaving || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(24)
        .frame(width: 480)
    }
}

private struct CreateLocalAgentSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: AgentGroupChatViewModel
    @State private var name = ""
    @State private var role = ""
    @State private var responsibility = ""
    @State private var rolePrompt = ""
    @State private var modelConfigID = ""
    @State private var selectedPluginIDs: Set<String> = []
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
                }
                if viewModel.installedPlugins.isEmpty {
                    Text("没有可用于 Agent 的本机 Plugin")
                        .foregroundStyle(.secondary)
                } else {
                    Section("本地 Plugin") {
                        ForEach(viewModel.installedPlugins) { plugin in
                            Toggle(isOn: Binding(
                                get: { selectedPluginIDs.contains(plugin.id) },
                                set: { selected in
                                    if selected {
                                        selectedPluginIDs.insert(plugin.id)
                                    } else {
                                        selectedPluginIDs.remove(plugin.id)
                                    }
                                }
                            )) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(plugin.displayName)
                                    Text(plugin.description.isEmpty ? plugin.id : plugin.description)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
            Text("Agent 只会直接启动这里明确选择、且已在本机安装并启用的 Plugin。")
                .font(.caption).foregroundStyle(.secondary)
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
                            pluginIDs: selectedPluginIDs.sorted()
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
        }
    }
}

private struct LocalAgentBuilderSheet: View {
    @Environment(\.dismiss) private var dismiss
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
        Form {
            TextField(
                "描述希望新 Agent 承担的工作",
                text: $brief,
                axis: .vertical
            )
            .lineLimit(4...8)
            if viewModel.availableModels.isEmpty {
                Text("没有可供 Builder 使用的模型，请先配置并启用模型。")
                    .foregroundStyle(.secondary)
            } else {
                Picker("Builder 模型", selection: $builderModelConfigID) {
                    ForEach(viewModel.availableModels) { model in
                        Text("\(model.name) · \(model.modelName)").tag(model.id)
                    }
                }
            }
            Text("Builder 只能读取当前项目、群成员、可用模型和本机 Plugin 清单；它只能提交草案，不能直接创建成员。")
                .font(.caption)
                .foregroundStyle(.secondary)
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
                draftField("模型", modelName(draft.modelConfigID))
                draftField(
                    "本地 Plugin",
                    draft.pluginIDs.isEmpty
                        ? "无"
                        : draft.pluginIDs.map(pluginName).joined(separator: "、")
                )
                draftField("创建理由", draft.rationale.isEmpty ? "未说明" : draft.rationale)
                VStack(alignment: .leading, spacing: 5) {
                    Text("角色 Prompt").font(.caption).foregroundStyle(.secondary)
                    Text(draft.rolePrompt)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(10)
                        .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
                }
                Text("点击确认前不会创建 Agent。确认时客户端会重新校验模型与本机 Plugin 是否仍然可用。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(minHeight: 360, maxHeight: 560)
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

    private func pluginName(_ id: String) -> String {
        viewModel.installedPlugins.first(where: { $0.id == id })?.displayName ?? id
    }
}
