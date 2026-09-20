import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftUI

struct EditLocalAgentSheet: View {
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

struct InviteExistingAgentSheet: View {
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

struct CreateAgentRoomSheet: View {
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

struct CreateLocalAgentSheet: View {
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

struct LocalAgentBuilderSheet: View {
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
