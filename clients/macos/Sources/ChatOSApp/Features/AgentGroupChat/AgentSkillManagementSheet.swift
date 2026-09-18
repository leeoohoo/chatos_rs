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
                    markdownContent
                }
                .padding(18)
            }
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
        ScrollView {
            MarkdownDocumentView(
                markdown: viewModel.displayedContent.isEmpty
                    ? "_暂无内容_"
                    : viewModel.displayedContent
            )
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .padding(20)
        }
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
