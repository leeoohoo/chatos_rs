import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct NotepadSheet: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: NotepadViewModel
    @State private var prompt: NotepadPrompt?
    @State private var promptText = ""
    @State private var deleteTarget: NotepadDeleteTarget?
    let onClose: () -> Void

    init(service: any NotepadServicing, onClose: @escaping () -> Void) {
        _viewModel = StateObject(wrappedValue: NotepadViewModel(service: service))
        self.onClose = onClose
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            HSplitView {
                sidebar
                    .frame(minWidth: 250, idealWidth: 310, maxWidth: 380)
                editor
                    .frame(minWidth: 620)
            }
        }
        .frame(minWidth: 980, idealWidth: 1_180, minHeight: 650, idealHeight: 760)
        .environment(\.locale, model.interfaceLocale)
        .task {
            viewModel.interfaceLanguage = model.interfaceLanguage
            await viewModel.load()
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(8))
                guard !Task.isCancelled else { break }
                await viewModel.syncExternalChanges()
            }
        }
        .onChange(of: model.interfaceLanguage) { _, language in
            viewModel.interfaceLanguage = language
        }
        .onChange(of: viewModel.searchQuery) { viewModel.scheduleSearch() }
        .alert(prompt?.title(language: model.interfaceLanguage) ?? "", isPresented: promptPresented) {
            TextField(prompt?.placeholder(language: model.interfaceLanguage) ?? "", text: $promptText)
            Button("取消", role: .cancel) { prompt = nil }
            Button("创建") { submitPrompt() }
                .disabled(promptText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        } message: {
            Text(prompt?.message(language: model.interfaceLanguage) ?? "")
        }
        .confirmationDialog(
            deleteTarget?.title(language: model.interfaceLanguage)
                ?? model.localized("确认删除", english: "Confirm Deletion"),
            isPresented: deletePresented,
            titleVisibility: .visible
        ) {
            Button("删除", role: .destructive) { performDelete() }
            Button("取消", role: .cancel) { deleteTarget = nil }
        } message: {
            Text(deleteTarget?.message(language: model.interfaceLanguage) ?? "")
        }
        .interactiveDismissDisabled(viewModel.isDirty || viewModel.isSaving)
    }

    private var header: some View {
        HStack(spacing: 12) {
            Label("记事本", systemImage: "note.text")
                .appFont(.headline)
            if viewModel.isLoading || viewModel.isLoadingNote || viewModel.isSaving {
                ProgressView().controlSize(.small)
            }
            Spacer()
            Button("刷新", systemImage: "arrow.clockwise") {
                Task { await viewModel.refresh() }
            }
            .disabled(viewModel.isLoading)
            Button("关闭") { close() }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(.bar)
    }

    private var sidebar: some View {
        VStack(spacing: 0) {
            VStack(spacing: 10) {
                HStack(spacing: 8) {
                    Button("新建文件夹", systemImage: "folder.badge.plus") {
                        showPrompt(.folder(parent: viewModel.selectedFolder))
                    }
                    Button("新建笔记", systemImage: "square.and.pencil") {
                        showPrompt(.note(folder: viewModel.selectedFolder))
                    }
                    .buttonStyle(.borderedProminent)
                }
                .controlSize(.small)

                TextField("搜索标题或文件夹", text: $viewModel.searchQuery)
                    .textFieldStyle(.roundedBorder)

                HStack {
                    Text(model.localized(
                        "当前目录：\(viewModel.selectedFolder.isEmpty ? "根目录" : viewModel.selectedFolder)",
                        english: "Current folder: \(viewModel.selectedFolder.isEmpty ? "Root" : viewModel.selectedFolder)"
                    ))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Spacer()
                }
            }
            .padding(12)

            Divider()

            List {
                Button {
                    viewModel.selectFolder("")
                } label: {
                    Label("根目录", systemImage: "tray")
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .listRowBackground(
                    viewModel.selectedTreeNodeID == "folder:"
                        ? Color.accentColor.opacity(0.12)
                        : Color.clear
                )
                .contextMenu { creationMenu(folder: "") }

                OutlineGroup(viewModel.tree, children: \.children) { node in
                    treeRow(node)
                }
            }
            .listStyle(.sidebar)
            .overlay {
                if !viewModel.isLoading, viewModel.tree.isEmpty {
                    ContentUnavailableView(
                        "暂无笔记",
                        systemImage: "note.text",
                        description: Text("创建文件夹或笔记开始记录。")
                    )
                }
            }
        }
    }

    @ViewBuilder
    private func treeRow(_ node: NotepadTreeNode) -> some View {
        switch node.kind {
        case let .folder(folder):
            Button {
                viewModel.selectFolder(folder)
            } label: {
                Label(node.title, systemImage: "folder")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .listRowBackground(
                viewModel.selectedTreeNodeID == node.id
                    ? Color.accentColor.opacity(0.12)
                    : Color.clear
            )
            .contextMenu {
                creationMenu(folder: folder)
                Divider()
                Button("删除文件夹", systemImage: "trash", role: .destructive) {
                    deleteTarget = .folder(folder)
                }
            }
        case let .note(note):
            Button {
                Task { await viewModel.selectNote(note.id) }
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Label(node.title, systemImage: "doc.text")
                        .lineLimit(1)
                    if let subtitle = node.subtitle {
                        Text(subtitle)
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .listRowBackground(
                viewModel.selectedTreeNodeID == node.id
                    ? Color.accentColor.opacity(0.12)
                    : Color.clear
            )
            .contextMenu {
                Button("复制文本", systemImage: "doc.on.doc") { copyText(viewModel.content) }
                    .disabled(viewModel.selectedNoteID != note.id)
                Button("导出 Markdown", systemImage: "square.and.arrow.down") {
                    exportMarkdown(title: note.title, content: viewModel.content)
                }
                .disabled(viewModel.selectedNoteID != note.id)
                Divider()
                Button("删除笔记", systemImage: "trash", role: .destructive) {
                    deleteTarget = .note(note)
                }
            }
        }
    }

    @ViewBuilder
    private var editor: some View {
        VStack(spacing: 0) {
            editorToolbar
            Divider()

            if let error = viewModel.errorMessage {
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                    Text(error).appFont(.caption).textSelection(.enabled)
                    Spacer()
                    Button("关闭", systemImage: "xmark") { viewModel.clearError() }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                }
                .padding(10)
                .background(Color.orange.opacity(0.08))
                Divider()
            }

            if viewModel.selectedNoteID != nil {
                editorContent
            } else {
                ContentUnavailableView(
                    "选择一条笔记",
                    systemImage: "note.text",
                    description: Text("从左侧打开笔记，或新建一条笔记。")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private var editorToolbar: some View {
        HStack(spacing: 10) {
            Text(viewModel.selectedNoteID == nil
                 ? model.localized("未选择笔记", english: "No Note Selected")
                 : model.localized("编辑笔记", english: "Edit Note"))
                .appFont(.subheadline.weight(.semibold))
            if viewModel.isDirty {
                Text("未保存")
                    .appFont(.caption2.weight(.semibold))
                    .foregroundStyle(.orange)
            }
            Spacer()
            Picker("显示模式", selection: $viewModel.editorMode) {
                ForEach(NotepadEditorMode.allCases) { mode in
                    Text(mode.title(language: model.interfaceLanguage)).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 190)
            Button("复制", systemImage: "doc.on.doc") { copyText(viewModel.content) }
                .disabled(viewModel.selectedNoteID == nil)
            Button("导出", systemImage: "square.and.arrow.down") {
                exportMarkdown(title: viewModel.title, content: viewModel.content)
            }
            .disabled(viewModel.selectedNoteID == nil)
            Button("保存", systemImage: "square.and.arrow.down") {
                Task { _ = await viewModel.save() }
            }
            .keyboardShortcut("s", modifiers: .command)
            .buttonStyle(.borderedProminent)
            .disabled(!viewModel.isDirty || viewModel.isSaving)
            Button("删除", systemImage: "trash", role: .destructive) {
                if let note = viewModel.selectedNote { deleteTarget = .note(note) }
            }
            .disabled(viewModel.selectedNoteID == nil)
        }
        .controlSize(.small)
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.bar)
    }

    private var editorContent: some View {
        VStack(spacing: 12) {
            TextField("标题", text: $viewModel.title)
                .textFieldStyle(.roundedBorder)
            TextField("标签（使用逗号分隔）", text: $viewModel.tagsText)
                .textFieldStyle(.roundedBorder)

            switch viewModel.editorMode {
            case .edit:
                markdownEditor
            case .preview:
                markdownPreview
            case .split:
                HSplitView {
                    markdownEditor.frame(minWidth: 280)
                    markdownPreview.frame(minWidth: 280)
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var markdownEditor: some View {
        TextEditor(text: $viewModel.content)
            .appFont(.system(.body, design: .monospaced))
            .scrollContentBackground(.hidden)
            .padding(8)
            .background(Color(nsColor: .textBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: 7))
            .overlay(RoundedRectangle(cornerRadius: 7).stroke(.separator))
    }

    private var markdownPreview: some View {
        ScrollView {
            MarkdownDocumentView(
                markdown: viewModel.content.isEmpty
                    ? model.localized("_暂无内容_", english: "_No content_")
                    : viewModel.content
            )
                .frame(maxWidth: .infinity, alignment: .topLeading)
                .padding(12)
        }
        .background(Color(nsColor: .textBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 7))
        .overlay(RoundedRectangle(cornerRadius: 7).stroke(.separator))
    }

    private var promptPresented: Binding<Bool> {
        Binding(
            get: { prompt != nil },
            set: { if !$0 { prompt = nil } }
        )
    }

    private var deletePresented: Binding<Bool> {
        Binding(
            get: { deleteTarget != nil },
            set: { if !$0 { deleteTarget = nil } }
        )
    }

    @ViewBuilder
    private func creationMenu(folder: String) -> some View {
        Button("新建子文件夹", systemImage: "folder.badge.plus") {
            viewModel.selectFolder(folder)
            showPrompt(.folder(parent: folder))
        }
        Button("新建笔记", systemImage: "square.and.pencil") {
            viewModel.selectFolder(folder)
            showPrompt(.note(folder: folder))
        }
    }

    private func showPrompt(_ value: NotepadPrompt) {
        promptText = ""
        prompt = value
    }

    private func submitPrompt() {
        guard let prompt else { return }
        let value = promptText
        self.prompt = nil
        Task {
            switch prompt {
            case let .folder(parent):
                _ = await viewModel.createFolder(name: value, parent: parent)
            case let .note(folder):
                _ = await viewModel.createNote(title: value, folder: folder)
            }
        }
    }

    private func performDelete() {
        guard let target = deleteTarget else { return }
        deleteTarget = nil
        Task {
            switch target {
            case let .folder(folder): _ = await viewModel.deleteFolder(folder)
            case let .note(note): _ = await viewModel.deleteNote(note.id)
            }
        }
    }

    private func close() {
        Task {
            if viewModel.isDirty, !(await viewModel.save()) { return }
            onClose()
        }
    }

    private func copyText(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    private func exportMarkdown(title: String, content: String) {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "md") ?? .plainText]
        panel.canCreateDirectories = true
        let cleaned = title
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "/", with: "-")
        panel.nameFieldStringValue = (
            cleaned.isEmpty
                ? model.localized("未命名笔记", english: "Untitled Note")
                : cleaned
        ) + ".md"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try content.write(to: url, atomically: true, encoding: .utf8)
        } catch {
            NSSound.beep()
        }
    }
}

private enum NotepadPrompt {
    case folder(parent: String)
    case note(folder: String)

    func title(language: ChatOSLanguage) -> String {
        switch self {
        case .folder: language == .english ? "New Folder" : "新建文件夹"
        case .note: language == .english ? "New Note" : "新建笔记"
        }
    }

    func message(language: ChatOSLanguage) -> String {
        switch self {
        case let .folder(parent):
            if language == .english {
                return parent.isEmpty ? "Create a folder in Root." : "Create a subfolder in “\(parent)”."
            }
            return parent.isEmpty ? "在根目录下创建文件夹。" : "在“\(parent)”下创建子文件夹。"
        case let .note(folder):
            if language == .english {
                return folder.isEmpty ? "Create a note in Root." : "Create a note in “\(folder)”."
            }
            return folder.isEmpty ? "在根目录下创建笔记。" : "在“\(folder)”下创建笔记。"
        }
    }

    func placeholder(language: ChatOSLanguage) -> String {
        switch self {
        case .folder: language == .english ? "Folder Name" : "文件夹名称"
        case .note: language == .english ? "Note Title" : "笔记标题"
        }
    }
}

private enum NotepadDeleteTarget {
    case folder(String)
    case note(NotepadNote)

    func title(language: ChatOSLanguage) -> String {
        switch self {
        case .folder: language == .english ? "Delete Folder?" : "删除文件夹？"
        case .note: language == .english ? "Delete Note?" : "删除笔记？"
        }
    }

    func message(language: ChatOSLanguage) -> String {
        switch self {
        case let .folder(folder):
            return language == .english
                ? "“\(folder)” and all notes inside it will be permanently deleted."
                : "“\(folder)”以及其中的全部笔记都会被删除，此操作无法撤销。"
        case let .note(note):
            let title = note.title.isEmpty
                ? (language == .english ? "Untitled Note" : "未命名笔记")
                : note.title
            return language == .english
                ? "“\(title)” will be permanently deleted."
                : "“\(title)”将被永久删除。"
        }
    }
}
