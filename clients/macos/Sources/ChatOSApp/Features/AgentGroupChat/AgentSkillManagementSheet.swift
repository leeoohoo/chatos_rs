import ChatOSCore
import SwiftUI

private enum AgentSkillManagementKind: String, CaseIterable, Identifiable {
    case profession
    case projectType

    var id: String { rawValue }

    var title: String {
        switch self {
        case .profession: "职业 Skill"
        case .projectType: "项目类型 Skill"
        }
    }
}

private enum AgentSkillContentMode: String, CaseIterable, Identifiable {
    case preview
    case edit
    case split

    var id: String { rawValue }

    var title: String {
        switch self {
        case .preview: "预览"
        case .edit: "编辑"
        case .split: "分栏"
        }
    }
}

private enum AgentSkillDisclosureLayer: String, CaseIterable, Identifiable {
    case overview
    case instructions
    case references

    var id: String { rawValue }

    var title: String {
        switch self {
        case .overview: "概要"
        case .instructions: "完整说明"
        case .references: "参考资料"
        }
    }
}

@MainActor
private final class AgentSkillManagementViewModel: ObservableObject {
    @Published var kind: AgentSkillManagementKind = .profession
    @Published var language: ChatOSLanguage
    @Published var query = ""
    @Published var selectedKey: String?
    @Published var label = ""
    @Published var labelEN = ""
    @Published var summary = ""
    @Published var summaryEN = ""
    @Published var content = ""
    @Published var contentEN = ""
    @Published private(set) var professions: [LocalAgentProfessionDefinition] = []
    @Published private(set) var projectTypes: [LocalProjectTypeDefinition] = []
    @Published private(set) var isSaving = false
    @Published var errorMessage: String?

    let ownerUserID: String
    let library: LocalAgentSkillLibrary

    init(
        ownerUserID: String,
        library: LocalAgentSkillLibrary,
        initialLanguage: ChatOSLanguage
    ) {
        self.ownerUserID = ownerUserID
        self.library = library
        language = initialLanguage
        reload(preservingSelection: false)
    }

    var filteredProfessions: [LocalAgentProfessionDefinition] {
        let query = normalizedQuery
        guard !query.isEmpty else { return professions }
        return professions.filter {
            $0.label.localizedCaseInsensitiveContains(query)
                || $0.labelEN.localizedCaseInsensitiveContains(query)
                || $0.key.localizedCaseInsensitiveContains(query)
                || $0.categoryLabel.localizedCaseInsensitiveContains(query)
                || $0.categoryLabelEN.localizedCaseInsensitiveContains(query)
                || $0.description.localizedCaseInsensitiveContains(query)
                || $0.descriptionEN.localizedCaseInsensitiveContains(query)
        }
    }

    var filteredProjectTypes: [LocalProjectTypeDefinition] {
        let query = normalizedQuery
        guard !query.isEmpty else { return projectTypes }
        return projectTypes.filter {
            $0.label.localizedCaseInsensitiveContains(query)
                || $0.labelEN.localizedCaseInsensitiveContains(query)
                || $0.key.localizedCaseInsensitiveContains(query)
                || $0.categoryLabel.localizedCaseInsensitiveContains(query)
                || $0.categoryLabelEN.localizedCaseInsensitiveContains(query)
                || $0.description.localizedCaseInsensitiveContains(query)
                || $0.descriptionEN.localizedCaseInsensitiveContains(query)
        }
    }

    var selectedCategory: String {
        switch kind {
        case .profession:
            guard let item = professions.first(where: { $0.key == selectedKey }) else { return "" }
            return language == .english ? item.categoryLabelEN : item.categoryLabel
        case .projectType:
            guard let item = projectTypes.first(where: { $0.key == selectedKey }) else { return "" }
            return language == .english ? item.categoryLabelEN : item.categoryLabel
        }
    }

    var displayedContent: String {
        language == .english ? contentEN : content
    }

    var selectedProgressiveSkill: LocalAgentBoundProgressiveSkill? {
        switch kind {
        case .profession:
            guard let item = professions.first(where: { $0.key == selectedKey }) else {
                return nil
            }
            return LocalAgentProgressiveSkillCatalog.boundProfessionSkill(
                item,
                language: language
            )
        case .projectType:
            guard let item = projectTypes.first(where: { $0.key == selectedKey }) else {
                return nil
            }
            return LocalAgentProgressiveSkillCatalog.boundProjectTypeSkill(
                item,
                language: language
            )
        }
    }

    func displayLabel(_ item: LocalAgentProfessionDefinition) -> String {
        language == .english ? item.labelEN : item.label
    }

    func displayCategory(_ item: LocalAgentProfessionDefinition) -> String {
        language == .english ? item.categoryLabelEN : item.categoryLabel
    }

    func displayLabel(_ item: LocalProjectTypeDefinition) -> String {
        language == .english ? item.labelEN : item.label
    }

    func displayCategory(_ item: LocalProjectTypeDefinition) -> String {
        language == .english ? item.categoryLabelEN : item.categoryLabel
    }

    var hasOverride: Bool {
        guard let selectedKey else { return false }
        switch kind {
        case .profession:
            return library.hasProfessionOverride(ownerUserID: ownerUserID, key: selectedKey)
        case .projectType:
            return library.hasProjectTypeOverride(ownerUserID: ownerUserID, key: selectedKey)
        }
    }

    func switchKind() {
        selectedKey = nil
        selectFirstVisibleItem()
    }

    func select(_ key: String) {
        selectedKey = key
        loadEditor()
    }

    func save() {
        guard let selectedKey, !isSaving else { return }
        isSaving = true
        defer { isSaving = false }
        do {
            switch kind {
            case .profession:
                try library.updateProfessionBilingual(
                    ownerUserID: ownerUserID,
                    key: selectedKey,
                    label: label,
                    description: summary,
                    skillMarkdown: content,
                    labelEN: labelEN,
                    descriptionEN: summaryEN,
                    skillMarkdownEN: contentEN
                )
            case .projectType:
                try library.updateProjectTypeBilingual(
                    ownerUserID: ownerUserID,
                    key: selectedKey,
                    label: label,
                    description: summary,
                    ruleMarkdown: content,
                    labelEN: labelEN,
                    descriptionEN: summaryEN,
                    ruleMarkdownEN: contentEN
                )
            }
            errorMessage = nil
            reload(preservingSelection: true)
            NotificationCenter.default.post(name: .agentSkillLibraryDidChange, object: nil)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func reset() {
        guard let selectedKey, !isSaving else { return }
        do {
            switch kind {
            case .profession:
                try library.resetProfession(ownerUserID: ownerUserID, key: selectedKey)
            case .projectType:
                try library.resetProjectType(ownerUserID: ownerUserID, key: selectedKey)
            }
            errorMessage = nil
            reload(preservingSelection: true)
            NotificationCenter.default.post(name: .agentSkillLibraryDidChange, object: nil)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private var normalizedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func reload(preservingSelection: Bool) {
        let previous = preservingSelection ? selectedKey : nil
        professions = library.professions(ownerUserID: ownerUserID)
        projectTypes = library.projectTypes(ownerUserID: ownerUserID)
        selectedKey = previous
        if selectedKey == nil {
            selectFirstVisibleItem()
        } else {
            loadEditor()
        }
    }

    private func selectFirstVisibleItem() {
        switch kind {
        case .profession: select(professions.first?.key ?? "")
        case .projectType: select(projectTypes.first?.key ?? "")
        }
    }

    private func loadEditor() {
        guard let selectedKey, !selectedKey.isEmpty else {
            label = ""
            labelEN = ""
            summary = ""
            summaryEN = ""
            content = ""
            contentEN = ""
            return
        }
        switch kind {
        case .profession:
            guard let item = professions.first(where: { $0.key == selectedKey }) else { return }
            label = item.label
            labelEN = item.labelEN
            summary = item.description
            summaryEN = item.descriptionEN
            content = item.skillMarkdown
            contentEN = item.skillMarkdownEN
        case .projectType:
            guard let item = projectTypes.first(where: { $0.key == selectedKey }) else { return }
            label = item.label
            labelEN = item.labelEN
            summary = item.description
            summaryEN = item.descriptionEN
            content = item.ruleMarkdown
            contentEN = item.ruleMarkdownEN
        }
    }
}

struct AgentSkillManagementSheet: View {
    @Environment(\.dismiss) private var dismiss
    @StateObject private var viewModel: AgentSkillManagementViewModel
    @State private var contentMode: AgentSkillContentMode = .preview
    @State private var disclosureLayer: AgentSkillDisclosureLayer = .overview
    @State private var selectedResourcePath = "references/workflow.md"
    private let onLanguageChange: (ChatOSLanguage) -> Void

    init(
        ownerUserID: String,
        skillLibrary: LocalAgentSkillLibrary,
        initialLanguage: ChatOSLanguage,
        onLanguageChange: @escaping (ChatOSLanguage) -> Void
    ) {
        self.onLanguageChange = onLanguageChange
        _viewModel = StateObject(wrappedValue: AgentSkillManagementViewModel(
            ownerUserID: ownerUserID,
            library: skillLibrary,
            initialLanguage: initialLanguage
        ))
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            HSplitView {
                catalog
                    .frame(minWidth: 300, idealWidth: 340, maxWidth: 420)
                editor
                    .frame(minWidth: 800, maxWidth: .infinity, maxHeight: .infinity)
            }
            Divider()
            footer
        }
        .frame(minWidth: 1_180, idealWidth: 1_320, minHeight: 780, idealHeight: 900)
        .alert("无法保存 Skill", isPresented: Binding(
            get: { viewModel.errorMessage != nil },
            set: { if !$0 { viewModel.errorMessage = nil } }
        )) {
            Button("好", role: .cancel) { viewModel.errorMessage = nil }
        } message: {
            Text(viewModel.errorMessage ?? "")
        }
    }

    private var header: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Skill 管理")
                    .font(.title2.weight(.semibold))
                Text("管理 Agent 职业与项目类型的完整运行规则。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("新建 Agent Run 会使用当前选择的语言版本。")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            Picker("类型", selection: $viewModel.kind) {
                ForEach(AgentSkillManagementKind.allCases) { kind in
                    Text(kind.title).tag(kind)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 270)
            .onChange(of: viewModel.kind) { _, _ in viewModel.switchKind() }
            Picker("Skill 语言", selection: $viewModel.language) {
                Text("中文").tag(ChatOSLanguage.simplifiedChinese)
                Text("English").tag(ChatOSLanguage.english)
            }
            .pickerStyle(.segmented)
            .frame(width: 180)
            .onChange(of: viewModel.language) { _, language in
                onLanguageChange(language)
            }
        }
        .padding(18)
    }

    private var catalog: some View {
        VStack(spacing: 0) {
            TextField("搜索 Skill", text: $viewModel.query)
                .textFieldStyle(.roundedBorder)
                .padding(12)
            Divider()
            List(selection: Binding(
                get: { viewModel.selectedKey },
                set: { if let value = $0 { viewModel.select(value) } }
            )) {
                if viewModel.kind == .profession {
                    ForEach(viewModel.filteredProfessions) { item in
                        catalogRow(
                            title: viewModel.displayLabel(item),
                            category: viewModel.displayCategory(item),
                            key: item.key,
                            customized: viewModel.library.hasProfessionOverride(
                                ownerUserID: viewModel.ownerUserID,
                                key: item.key
                            )
                        )
                        .tag(item.key)
                    }
                } else {
                    ForEach(viewModel.filteredProjectTypes) { item in
                        catalogRow(
                            title: viewModel.displayLabel(item),
                            category: viewModel.displayCategory(item),
                            key: item.key,
                            customized: viewModel.library.hasProjectTypeOverride(
                                ownerUserID: viewModel.ownerUserID,
                                key: item.key
                            )
                        )
                        .tag(item.key)
                    }
                }
            }
            .listStyle(.sidebar)
        }
    }

    private func catalogRow(
        title: String,
        category: String,
        key: String,
        customized: Bool
    ) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(title)
                    .font(.body.weight(.medium))
                if customized {
                    Text("已修改")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(Color.accentColor)
                }
            }
            Text(category)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(key)
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private var editor: some View {
        if viewModel.selectedKey == nil {
            ContentUnavailableView("选择一个 Skill", systemImage: "books.vertical")
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    HStack(spacing: 16) {
                        readOnlyField("Key", value: viewModel.selectedKey ?? "")
                        readOnlyField("分类", value: viewModel.selectedCategory)
                    }
                    editField("名称") {
                        TextField(
                            "Skill 名称",
                            text: viewModel.language == .english
                                ? $viewModel.labelEN
                                : $viewModel.label
                        )
                            .textFieldStyle(.roundedBorder)
                    }
                    editField("说明") {
                        TextField(
                            "用途说明",
                            text: viewModel.language == .english
                                ? $viewModel.summaryEN
                                : $viewModel.summary,
                            axis: .vertical
                        )
                            .lineLimit(2...4)
                            .textFieldStyle(.roundedBorder)
                    }
                    disclosurePicker
                    switch disclosureLayer {
                    case .overview:
                        progressiveOverview
                    case .instructions:
                        markdownContent
                    case .references:
                        progressiveReferences
                    }
                }
                .padding(18)
            }
        }
    }

    private var disclosurePicker: some View {
        Picker("渐进披露层级", selection: $disclosureLayer) {
            ForEach(AgentSkillDisclosureLayer.allCases) { layer in
                Text(layer.title).tag(layer)
            }
        }
        .pickerStyle(.segmented)
        .frame(maxWidth: 420)
    }

    @ViewBuilder
    private var progressiveOverview: some View {
        if let skill = viewModel.selectedProgressiveSkill {
            VStack(alignment: .leading, spacing: 16) {
                Label("渐进披露结构", systemImage: "point.3.connected.trianglepath.dotted")
                    .font(.headline)
                HStack(alignment: .top, spacing: 12) {
                    disclosureCard(
                        step: "1",
                        title: "Router 概要",
                        detail: "Run 启动时只注入名称、用途与绑定引用，避免完整正文长期占用上下文。"
                    )
                    disclosureCard(
                        step: "2",
                        title: "按需激活",
                        detail: "Agent 通过 agent_skill_activate 读取完整说明，且只能激活当前身份绑定的 Skill。"
                    )
                    disclosureCard(
                        step: "3",
                        title: "章节资料",
                        detail: "工作流、证据、质量风险和协作边界通过资源工具按需分页读取。"
                    )
                }
                VStack(alignment: .leading, spacing: 8) {
                    Text("运行时 Router 预览")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text("`\(skill.skillRef)` · **\(skill.label)**：\(skill.description)")
                        .font(.callout)
                        .textSelection(.enabled)
                    Text("正文 SHA-256：\(skill.instructionsSHA256)")
                        .font(.caption2.monospaced())
                        .foregroundStyle(.tertiary)
                        .textSelection(.enabled)
                }
                .padding(14)
                .background(Color(nsColor: .controlBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: 10))

                Text("此 Skill 包含 1 份可编辑完整说明和 \(skill.resources.count) 份详细参考资料。自定义正文会固定到新 Run 的快照中；参考资料仍受当前职业/项目类型绑定约束，不会扩大真实工具权限。")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func disclosureCard(step: String, title: String, detail: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(step)
                .font(.caption.bold())
                .foregroundStyle(Color.accentColor)
            Text(title).font(.subheadline.weight(.semibold))
            Text(detail)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(12)
        .frame(maxWidth: .infinity, minHeight: 130, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    @ViewBuilder
    private var progressiveReferences: some View {
        if let skill = viewModel.selectedProgressiveSkill {
            HSplitView {
                List(skill.resources, selection: $selectedResourcePath) { resource in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(resource.title).font(.body.weight(.medium))
                        Text(resource.summary)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(3)
                        Text(resource.relativePath)
                            .font(.caption2.monospaced())
                            .foregroundStyle(.tertiary)
                    }
                    .padding(.vertical, 5)
                    .tag(resource.relativePath)
                }
                .frame(minWidth: 280, idealWidth: 320)

                if let resource = skill.resources.first(where: {
                    $0.relativePath == selectedResourcePath
                }) ?? skill.resources.first {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text(resource.title).font(.headline)
                            Spacer()
                            Text("\(resource.sizeBytes) bytes")
                                .font(.caption2.monospaced())
                                .foregroundStyle(.tertiary)
                        }
                        MarkdownReaderView(markdown: resource.markdown)
                            .padding(18)
                            .frame(maxWidth: .infinity, minHeight: 500, alignment: .topLeading)
                            .background(Color(nsColor: .textBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: 9))
                    }
                    .frame(minWidth: 440)
                }
            }
            .frame(minHeight: 560)
        }
    }

    private var markdownContent: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(viewModel.kind == .profession ? "Skill 正文" : "项目规则正文")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Picker("Markdown 显示", selection: $contentMode) {
                    ForEach(AgentSkillContentMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: 210)
            }

            switch contentMode {
            case .preview:
                markdownPreview
            case .edit:
                markdownEditor
            case .split:
                HSplitView {
                    markdownEditor.frame(minWidth: 360)
                    markdownPreview.frame(minWidth: 360)
                }
            }
        }
    }

    private var markdownEditor: some View {
        TextEditor(
            text: viewModel.language == .english
                ? $viewModel.contentEN
                : $viewModel.content
        )
            .font(.system(.body, design: .monospaced))
            .scrollContentBackground(.hidden)
            .padding(10)
            .frame(minHeight: 520)
            .background(Color(nsColor: .textBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: 9))
            .overlay {
                RoundedRectangle(cornerRadius: 9)
                    .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
            }
    }

    private var markdownPreview: some View {
        MarkdownReaderView(
            markdown: viewModel.displayedContent.isEmpty
                ? "_暂无内容_"
                : viewModel.displayedContent
        )
        .padding(20)
        .frame(minHeight: 520)
        .background(Color(nsColor: .textBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 9))
        .overlay {
            RoundedRectangle(cornerRadius: 9)
                .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
        }
    }

    private func readOnlyField(_ title: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.callout.monospaced())
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func editField<Content: View>(
        _ title: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            content()
        }
    }

    private var footer: some View {
        HStack {
            Button("恢复内置版本", role: .destructive) { viewModel.reset() }
                .disabled(!viewModel.hasOverride || viewModel.isSaving)
            Spacer()
            Button("完成") { dismiss() }
            Button("保存") { viewModel.save() }
                .buttonStyle(.borderedProminent)
                .disabled(
                    viewModel.isSaving
                        || viewModel.label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || viewModel.labelEN.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || viewModel.summary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || viewModel.summaryEN.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || viewModel.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || viewModel.contentEN.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                )
        }
        .padding(14)
    }
}
