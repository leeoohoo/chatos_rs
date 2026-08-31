import AppKit
import ChatOSCore
import SwiftUI

struct ProjectDirectoryView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: ProjectDirectoryViewModel
    @StateObject private var gitViewModel: ProjectGitViewModel
    @State private var createKind: CreateKind?
    @State private var createName = ""
    @State private var sidebarMode: ProjectDirectorySidebarMode = .files

    init(
        projectID: String,
        rootPath: String?,
        service: any ProjectFilesystemServicing,
        codeNavigationService: any ProjectCodeNavigationServicing,
        gitService: any ProjectGitServicing
    ) {
        _viewModel = StateObject(wrappedValue: ProjectDirectoryViewModel(
            projectID: projectID,
            rootPath: rootPath,
            service: service,
            codeNavigationService: codeNavigationService
        ))
        _gitViewModel = StateObject(wrappedValue: ProjectGitViewModel(
            projectRoot: rootPath,
            service: gitService
        ))
    }

    var body: some View {
        HSplitView {
            sidebar
                .frame(minWidth: 290, idealWidth: 360, maxWidth: 520)
            editor
                .frame(minWidth: 520)
        }
        .workspaceFill()
        .task { await viewModel.load() }
        .onChange(of: sidebarMode) { _, mode in
            guard mode == .git else { return }
            Task { await gitViewModel.load() }
        }
        .onChange(of: gitViewModel.worktreeRevision) { _, _ in
            Task { await viewModel.refresh() }
        }
        .onChange(of: viewModel.searchText) { _, _ in viewModel.scheduleSearch() }
        .alert(model.localized("项目目录操作失败", english: "Project directory operation failed"), isPresented: errorPresented) {
            Button(model.localized("好", english: "OK")) { viewModel.dismissError() }
        } message: {
            Text(viewModel.errorMessage ?? model.localized("未知错误", english: "Unknown error"))
        }
        .sheet(item: $createKind) { kind in createSheet(kind) }
    }

    private var sidebar: some View {
        VStack(spacing: 0) {
            Picker(model.localized("项目目录工具", english: "Project directory tools"), selection: $sidebarMode) {
                Label(model.localized("文件", english: "Files"), systemImage: "folder")
                    .tag(ProjectDirectorySidebarMode.files)
                Label(gitTabTitle, systemImage: "arrow.triangle.branch")
                    .tag(ProjectDirectorySidebarMode.git)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(10)
            .background(.bar)
            Divider()

            switch sidebarMode {
            case .files:
                fileBrowser
            case .git:
                ProjectGitWorkbenchView(
                    viewModel: gitViewModel,
                    onOpenFile: { change in
                        sidebarMode = .files
                        Task { await viewModel.openGitChange(change) }
                    }
                )
            }
        }
        .workspaceFill()
    }

    @ViewBuilder
    private var editor: some View {
        if sidebarMode == .git, let diff = gitViewModel.selectedDiff {
            ProjectGitDiffView(
                diff: diff,
                onClose: { gitViewModel.clearDiff() },
                onOpenFile: {
                    guard let change = gitViewModel.snapshot.changes.first(where: {
                        $0.path == diff.path
                    }) else { return }
                    sidebarMode = .files
                    Task { await viewModel.openGitChange(change) }
                }
            )
        } else {
            ProjectFileEditorView(viewModel: viewModel)
        }
    }

    private var fileBrowser: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                TextField(
                    model.localized("搜索项目文件", english: "Search project files"),
                    text: $viewModel.searchText
                )
                    .textFieldStyle(.roundedBorder)
                Menu {
                    Button(model.localized("新建文件", english: "New File"), systemImage: "doc.badge.plus") {
                        createKind = .file
                    }
                    Button(model.localized("新建文件夹", english: "New Folder"), systemImage: "folder.badge.plus") {
                        createKind = .directory
                    }
                } label: { Image(systemName: "plus") }
                    .menuStyle(.borderlessButton)
                Button { Task { await viewModel.refresh() } } label: { Image(systemName: "arrow.clockwise") }
                    .buttonStyle(.plain)
                    .help(model.localized("刷新", english: "Refresh"))
            }
            .padding(12)
            Divider()
            if viewModel.rootPath == nil {
                ContentUnavailableView(
                    model.localized("没有项目目录", english: "No project directory"),
                    systemImage: "folder.badge.questionmark",
                    description: Text(model.localized(
                        "请先为项目连接本机目录。",
                        english: "Connect a local directory to this project first."
                    ))
                )
                    .workspaceFill()
            } else if !viewModel.searchText.isEmpty {
                searchResults
            } else {
                tree
            }
        }
        .workspaceFill()
        .background(AppPalette.canvas)
    }

    private var gitTabTitle: String {
        gitViewModel.changeCount > 0 ? "Git \(gitViewModel.changeCount)" : "Git"
    }

    private var tree: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 1) {
                ForEach(viewModel.visibleEntries) { item in
                    ProjectFileTreeRow(item: item, isSelected: viewModel.selectedPath == item.entry.path) {
                        Task { await viewModel.toggle(item.entry) }
                    }
                }
            }
            .padding(.vertical, 6)
        }
        .workspaceFill(alignment: .topLeading)
        .overlay { if viewModel.isLoading && viewModel.visibleEntries.isEmpty { ProgressView() } }
    }

    private var searchResults: some View {
        List {
            if !viewModel.contentSearchResults.isEmpty {
                Section(model.localized("文件内容", english: "File Contents")) {
                    ForEach(viewModel.contentSearchResults) { match in
                        Button {
                            Task { await viewModel.selectContentSearchResult(match) }
                        } label: {
                            VStack(alignment: .leading, spacing: 5) {
                                HStack(spacing: 6) {
                                    Image(systemName: "text.magnifyingglass")
                                        .foregroundStyle(.secondary)
                                    Text(URL(fileURLWithPath: match.path).lastPathComponent)
                                        .appFont(.subheadline.weight(.semibold))
                                    Spacer()
                                    Text(model.localized(
                                        "第 \(match.line) 行",
                                        english: "Line \(match.line)"
                                    ))
                                        .appFont(.caption2.monospacedDigit())
                                        .foregroundStyle(AppPalette.ai)
                                }
                                Text(match.text.trimmingCharacters(in: .whitespaces))
                                    .appFont(.caption.monospaced())
                                    .foregroundStyle(.secondary)
                                    .lineLimit(2)
                                Text(match.displayPath ?? match.path)
                                    .appFont(.caption2.monospaced())
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(1)
                            }
                            .padding(.vertical, 3)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            if !viewModel.searchResults.isEmpty {
                Section(model.localized("文件和目录", english: "Files and Folders")) {
                    ForEach(viewModel.searchResults) { entry in
                        Button { Task { await viewModel.selectSearchResult(entry) } } label: {
                            VStack(alignment: .leading, spacing: 3) {
                                Label(entry.name, systemImage: entry.isDirectory ? "folder" : "doc.text")
                                Text(entry.displayPath ?? entry.path)
                                    .appFont(.caption2.monospaced())
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .workspaceFill()
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .background(AppPalette.canvas)
        .overlay {
            if viewModel.isSearching {
                ProgressView(model.localized(
                    "正在搜索文件内容…",
                    english: "Searching file contents…"
                ))
                    .padding(16)
                    .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 10))
                    .overlay { RoundedRectangle(cornerRadius: 10).stroke(AppPalette.border.opacity(0.8)) }
            } else if viewModel.searchResults.isEmpty && viewModel.contentSearchResults.isEmpty {
                ContentUnavailableView.search(text: viewModel.searchText)
            }
        }
    }

    private func createSheet(_ kind: CreateKind) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(kind == .file
                 ? model.localized("新建文件", english: "New File")
                 : model.localized("新建文件夹", english: "New Folder"))
                .appFont(.title2.weight(.semibold))
            TextField(
                kind == .file
                    ? model.localized("例如 README.md", english: "For example, README.md")
                    : model.localized("文件夹名称", english: "Folder name"),
                text: $createName
            )
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button(model.localized("取消", english: "Cancel")) {
                    createKind = nil
                    createName = ""
                }
                Button(model.localized("创建", english: "Create")) {
                    let name = createName.trimmingCharacters(in: .whitespacesAndNewlines)
                    createKind = nil
                    createName = ""
                    Task { await viewModel.create(name: name, directory: kind == .directory) }
                }
                .buttonStyle(.borderedProminent)
                .disabled(createName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(24)
        .frame(width: 420)
    }

    private var errorPresented: Binding<Bool> {
        Binding(get: { viewModel.errorMessage != nil }, set: { if !$0 { viewModel.dismissError() } })
    }
}

private enum CreateKind: String, Identifiable { case file, directory; var id: String { rawValue } }

private enum ProjectDirectorySidebarMode: String, Hashable {
    case files
    case git
}

private struct ProjectFileTreeRow: View {
    let item: ProjectDirectoryViewModel.VisibleEntry
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 7) {
                if item.entry.isDirectory {
                    Image(systemName: item.isExpanded ? "chevron.down" : "chevron.right")
                        .appFont(.caption2.weight(.bold))
                        .foregroundStyle(.secondary)
                } else {
                    Color.clear.frame(width: 10, height: 10)
                }
                Image(systemName: item.entry.isDirectory ? "folder.fill" : fileIcon)
                    .foregroundStyle(item.entry.isDirectory ? Color.accentColor : .secondary)
                Text(item.entry.name).lineLimit(1)
                Spacer(minLength: 4)
            }
            .padding(.leading, CGFloat(item.depth) * 18 + 9)
            .padding(.trailing, 8)
            .frame(height: 28)
            .contentShape(Rectangle())
            .background(isSelected ? Color.accentColor.opacity(0.16) : .clear, in: RoundedRectangle(cornerRadius: 5))
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 5)
    }

    private var fileIcon: String {
        switch URL(fileURLWithPath: item.entry.name).pathExtension.lowercased() {
        case "swift": "swift"
        case "md", "markdown": "doc.richtext"
        case "json", "yaml", "yml", "toml": "curlybraces"
        case "png", "jpg", "jpeg", "gif", "webp": "photo"
        default: "doc.text"
        }
    }
}
