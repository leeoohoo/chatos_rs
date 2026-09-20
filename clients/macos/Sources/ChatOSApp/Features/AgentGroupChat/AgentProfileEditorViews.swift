import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI

struct AgentProfileEditorSheet: View {
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

struct CreateAgentTeamSheet: View {
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
