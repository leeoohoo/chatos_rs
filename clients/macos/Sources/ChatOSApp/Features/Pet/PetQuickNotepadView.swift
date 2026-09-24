import AppKit
import ChatOSCore
import SwiftUI

struct PetQuickNotepadView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: NotepadViewModel
    @State private var collapsedFolderIDs: Set<String> = []
    @State private var creationPrompt: PetQuickNotepadCreationPrompt?
    @State private var creationName = ""
    let onBack: () -> Void
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            HStack(spacing: 0) {
                sidebar
                    .frame(width: 230)
                Divider()
                editor
            }
        }
        .task {
            viewModel.interfaceLanguage = model.interfaceLanguage
            await viewModel.refresh()
        }
        .onChange(of: model.interfaceLanguage) { _, language in
            viewModel.interfaceLanguage = language
        }
        .onDisappear {
            Task {
                if viewModel.isDirty { _ = await viewModel.save() }
            }
        }
        .alert(
            creationPrompt?.title(language: model.interfaceLanguage) ?? "",
            isPresented: creationPromptPresented
        ) {
            TextField(
                creationPrompt?.placeholder(language: model.interfaceLanguage) ?? "",
                text: $creationName
            )
            Button(model.localized("取消", english: "Cancel"), role: .cancel) {
                creationPrompt = nil
            }
            Button(model.localized("创建", english: "Create")) {
                submitCreationPrompt()
            }
            .disabled(creationName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        } message: {
            Text(creationPrompt?.message(language: model.interfaceLanguage) ?? "")
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button { close(afterSaving: onBack) } label: {
                Image(systemName: "chevron.left")
                    .frame(width: 26, height: 26)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)

            Image(systemName: "note.text")
                .foregroundStyle(Color.orange)
            VStack(alignment: .leading, spacing: 2) {
                Text(model.localized("快速记事本", english: "Quick Notepad"))
                    .font(.system(size: 14, weight: .semibold))
                Text(model.localized(
                    "与完整记事本实时同步",
                    english: "Synced with your full notepad"
                ))
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if viewModel.isLoading || viewModel.isLoadingNote
                || viewModel.isSaving || viewModel.isUploadingImage {
                ProgressView().controlSize(.small)
            }
            Button { close(afterSaving: onClose) } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .semibold))
                    .frame(width: 26, height: 26)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)
        }
        .padding(13)
    }

    private var sidebar: some View {
        VStack(spacing: 0) {
            VStack(spacing: 8) {
                HStack(spacing: 7) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(.secondary)
                    TextField(
                        model.localized("搜索笔记", english: "Search notes"),
                        text: $viewModel.searchQuery
                    )
                    .textFieldStyle(.plain)
                }
                .padding(.horizontal, 9)
                .frame(height: 30)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))

                Button {
                    createNote()
                } label: {
                    Label(
                        model.localized("新建笔记", english: "New Note"),
                        systemImage: "square.and.pencil"
                    )
                    .font(.system(size: 11, weight: .semibold))
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            }
            .padding(10)

            Divider()

            if viewModel.tree.isEmpty, !viewModel.isLoading {
                ContentUnavailableView(
                    model.localized("暂无笔记", english: "No Notes"),
                    systemImage: "note.text",
                    description: Text(model.localized(
                        "新建一条笔记开始记录",
                        english: "Create a note to get started"
                    ))
                )
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 5) {
                        Button {
                            viewModel.selectFolder("")
                        } label: {
                            Label(
                                model.localized("根目录", english: "Root"),
                                systemImage: "tray"
                            )
                            .font(.system(size: 11, weight: .medium))
                            .padding(8)
                            .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
                            .contentShape(Rectangle())
                            .background(
                                viewModel.selectedTreeNodeID == "folder:"
                                    ? Color.accentColor.opacity(0.12)
                                    : Color.clear,
                                in: RoundedRectangle(cornerRadius: 8)
                            )
                        }
                        .buttonStyle(.plain)
                        .contextMenu { creationMenu(folder: "") }

                        ForEach(visibleTreeNodes) { item in
                            HStack(spacing: 5) {
                                if case .folder = item.node.kind,
                                   !(item.node.children?.isEmpty ?? true) {
                                    Button {
                                        toggleFolder(item.node.id)
                                    } label: {
                                        Image(systemName: collapsedFolderIDs.contains(item.node.id)
                                              ? "chevron.right"
                                              : "chevron.down")
                                            .font(.system(size: 8, weight: .semibold))
                                            .foregroundStyle(.secondary)
                                            .frame(width: 14, height: 30)
                                            .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
                                } else {
                                    Color.clear.frame(width: 14, height: 1)
                                }

                                treeRow(item.node)
                            }
                            .padding(.leading, CGFloat(item.depth) * 18)
                            .overlay(alignment: .leading) {
                                if item.depth > 0 {
                                    Rectangle()
                                        .fill(Color(nsColor: .separatorColor).opacity(0.7))
                                        .frame(width: 1)
                                        .padding(.leading, CGFloat(item.depth) * 18 - 9)
                                }
                            }
                        }
                    }
                    .padding(7)
                }
                .contextMenu { creationMenu(folder: viewModel.selectedFolder) }
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .onChange(of: viewModel.searchQuery) { viewModel.scheduleSearch() }
    }

    @ViewBuilder
    private func treeRow(_ node: NotepadTreeNode) -> some View {
        switch node.kind {
        case let .folder(folder):
            Button {
                viewModel.selectFolder(folder)
            } label: {
                Label(node.title, systemImage: "folder")
                    .font(.system(size: 11, weight: .medium))
                    .padding(8)
                    .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
                    .contentShape(Rectangle())
                    .background(
                        viewModel.selectedTreeNodeID == node.id
                            ? Color.accentColor.opacity(0.12)
                            : Color.clear,
                        in: RoundedRectangle(cornerRadius: 8)
                    )
            }
            .buttonStyle(.plain)
            .contextMenu { creationMenu(folder: folder) }

        case let .note(note):
            Button {
                Task {
                    await viewModel.selectNote(note.id)
                    if viewModel.selectedNoteID == note.id {
                        viewModel.editorMode = .preview
                    }
                }
            } label: {
                HStack(alignment: .top, spacing: 7) {
                    Image(systemName: "doc.text")
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                        .padding(.top, 1)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(note.title.isEmpty
                             ? model.localized("未命名笔记", english: "Untitled Note")
                             : note.title)
                            .font(.system(size: 11, weight: .medium))
                            .lineLimit(2)
                        if let subtitle = node.subtitle {
                            Text(subtitle)
                                .font(.system(size: 9))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                }
                .padding(8)
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                .contentShape(Rectangle())
                .background(
                    viewModel.selectedNoteID == note.id
                        ? Color.accentColor.opacity(0.12)
                        : Color.clear,
                    in: RoundedRectangle(cornerRadius: 8)
                )
            }
            .buttonStyle(.plain)
            .contextMenu { creationMenu(folder: note.folder) }
        }
    }

    private var visibleTreeNodes: [PetQuickNotepadVisibleNode] {
        flatten(viewModel.tree, depth: 0)
    }

    private func flatten(
        _ nodes: [NotepadTreeNode],
        depth: Int
    ) -> [PetQuickNotepadVisibleNode] {
        nodes.flatMap { node in
            var result = [PetQuickNotepadVisibleNode(node: node, depth: depth)]
            if case .folder = node.kind,
               !collapsedFolderIDs.contains(node.id),
               let children = node.children {
                result.append(contentsOf: flatten(children, depth: depth + 1))
            }
            return result
        }
    }

    private func toggleFolder(_ id: String) {
        if collapsedFolderIDs.contains(id) {
            collapsedFolderIDs.remove(id)
        } else {
            collapsedFolderIDs.insert(id)
        }
    }

    @ViewBuilder
    private var editor: some View {
        if viewModel.selectedNoteID == nil {
            ContentUnavailableView(
                model.localized("选择或新建笔记", english: "Select or Create a Note"),
                systemImage: "square.and.pencil"
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            VStack(spacing: 8) {
                HStack(spacing: 8) {
                    HStack(spacing: 7) {
                        Image(systemName: "textformat")
                            .font(.system(size: 10))
                            .foregroundStyle(.secondary)
                        TextField(
                            model.localized("输入标题", english: "Enter a title"),
                            text: $viewModel.title
                        )
                        .textFieldStyle(.plain)
                        .font(.system(size: 14, weight: .medium))
                    }
                    .padding(.horizontal, 9)
                    .frame(height: 30)
                    .background(
                        Color(nsColor: .controlBackgroundColor).opacity(0.5),
                        in: RoundedRectangle(cornerRadius: 7)
                    )
                    .overlay {
                        RoundedRectangle(cornerRadius: 7)
                            .stroke(Color(nsColor: .separatorColor).opacity(0.45), lineWidth: 1)
                    }
                    if viewModel.isDirty {
                        Text(model.localized("未保存", english: "Unsaved"))
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundStyle(.orange)
                    }
                    Button {
                        viewModel.editorMode = viewModel.editorMode == .edit ? .preview : .edit
                    } label: {
                        Label(
                            viewModel.editorMode == .edit
                                ? model.localized("预览", english: "Preview")
                                : model.localized("编辑", english: "Edit"),
                            systemImage: viewModel.editorMode == .edit ? "eye" : "pencil"
                        )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    Button {
                        Task { _ = await viewModel.save() }
                    } label: {
                        Label(
                            model.localized("保存", english: "Save"),
                            systemImage: "square.and.arrow.down"
                        )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(
                        !viewModel.isDirty || viewModel.isSaving || viewModel.isUploadingImage
                    )
                }

                Divider()
                    .opacity(0.55)

                if viewModel.editorMode == .edit {
                    NotepadMarkdownEditor(text: $viewModel.content) { image, placeholder in
                        Task { await viewModel.uploadPastedImage(image, placeholder: placeholder) }
                    }
                        .padding(.horizontal, 3)
                        .padding(.vertical, 4)
                        .background(Color.clear)
                } else {
                    MarkdownReaderView(
                        markdown: viewModel.content.isEmpty
                            ? model.localized("_暂无内容_", english: "_No content_")
                            : viewModel.content
                    )
                    .padding(.horizontal, 3)
                    .padding(.vertical, 4)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(nsColor: .windowBackgroundColor))
        }
    }

    private func createNote() {
        Task {
            if viewModel.isDirty, !(await viewModel.save()) { return }
            let created = await viewModel.createNote(
                title: model.localized("新笔记", english: "New Note"),
                folder: viewModel.selectedFolder
            )
            if created { viewModel.editorMode = .edit }
        }
    }

    @ViewBuilder
    private func creationMenu(folder: String) -> some View {
        Button {
            showCreationPrompt(.folder(parent: folder))
        } label: {
            Label(
                model.localized("新建子文件夹", english: "New Subfolder"),
                systemImage: "folder.badge.plus"
            )
        }
        Button {
            showCreationPrompt(.note(folder: folder))
        } label: {
            Label(
                model.localized("新建笔记", english: "New Note"),
                systemImage: "square.and.pencil"
            )
        }
    }

    private var creationPromptPresented: Binding<Bool> {
        Binding(
            get: { creationPrompt != nil },
            set: { if !$0 { creationPrompt = nil } }
        )
    }

    private func showCreationPrompt(_ prompt: PetQuickNotepadCreationPrompt) {
        creationName = ""
        creationPrompt = prompt
    }

    private func submitCreationPrompt() {
        guard let prompt = creationPrompt else { return }
        let name = creationName
        creationPrompt = nil
        Task {
            if viewModel.isDirty, !(await viewModel.save()) { return }
            switch prompt {
            case let .folder(parent):
                _ = await viewModel.createFolder(name: name, parent: parent)
            case let .note(folder):
                let created = await viewModel.createNote(title: name, folder: folder)
                if created { viewModel.editorMode = .edit }
            }
        }
    }

    private func close(afterSaving action: @escaping () -> Void) {
        Task {
            if viewModel.isDirty, !(await viewModel.save()) { return }
            action()
        }
    }
}

private struct PetQuickNotepadVisibleNode: Identifiable {
    let node: NotepadTreeNode
    let depth: Int

    var id: String { node.id }
}

private enum PetQuickNotepadCreationPrompt {
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
        case .folder: language == .english ? "Folder name" : "文件夹名称"
        case .note: language == .english ? "Note title" : "笔记标题"
        }
    }
}
