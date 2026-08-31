import ChatOSCore
import Foundation

@MainActor
final class ProjectDirectoryViewModel: ObservableObject {
    enum NavigationRequestKind: String {
        case definition = "定义"
        case references = "引用"
    }

    struct VisibleEntry: Identifiable {
        var id: String { entry.path }
        let entry: ProjectFileEntry
        let depth: Int
        let isExpanded: Bool
    }

    @Published private(set) var visibleEntries: [VisibleEntry] = []
    @Published private(set) var searchResults: [ProjectFileEntry] = []
    @Published private(set) var contentSearchResults: [ProjectFileContentMatch] = []
    @Published private(set) var selectedFile: ProjectFileContent?
    @Published var selectedPath: String?
    @Published private(set) var selectedLine: Int?
    @Published private(set) var selectedSymbol: ProjectCodeSymbolSelection?
    @Published private(set) var navigationResult: ProjectCodeNavigationResult?
    @Published private(set) var navigationRequestKind: NavigationRequestKind?
    @Published private(set) var isNavigating = false
    @Published private(set) var navigationError: String?
    @Published var searchText = ""
    @Published var draft = ""
    @Published var isEditing = false
    @Published private(set) var isLoading = false
    @Published private(set) var isSearching = false
    @Published private(set) var isSaving = false
    @Published private(set) var errorMessage: String?

    let rootPath: String?
    private let service: any ProjectFilesystemServicing
    private let codeNavigationService: any ProjectCodeNavigationServicing
    private let expansionStateStore: any ProjectDirectoryExpansionStateStoring
    private var childrenByPath: [String: [ProjectFileEntry]] = [:]
    private var expandedPaths: Set<String>
    private var searchTask: Task<Void, Never>?
    private var navigationTask: Task<Void, Never>?
    private var navigationHistory: [NavigationPoint] = []

    init(
        projectID: String,
        rootPath: String?,
        service: any ProjectFilesystemServicing,
        codeNavigationService: any ProjectCodeNavigationServicing
    ) {
        let expansionStateStore = ProjectDirectoryExpansionStateStore(
            projectID: projectID,
            rootPath: rootPath
        )
        self.rootPath = rootPath
        self.service = service
        self.codeNavigationService = codeNavigationService
        self.expansionStateStore = expansionStateStore
        self.expandedPaths = expansionStateStore.loadExpandedPaths()
    }

    deinit {
        searchTask?.cancel()
        navigationTask?.cancel()
    }

    var canNavigateBack: Bool { !navigationHistory.isEmpty }

    func load() async {
        guard let rootPath = rootPath?.trimmedNonEmpty else {
            errorMessage = "这个项目还没有配置可访问的项目目录。"
            return
        }
        await loadChildrenWithConnectorRetry(of: rootPath)
    }

    func refresh() async {
        guard let rootPath = rootPath?.trimmedNonEmpty else { return }
        childrenByPath.removeAll()
        selectedFile = nil
        selectedPath = nil
        selectedLine = nil
        clearCodeNavigation(clearHistory: true)
        await loadChildren(of: rootPath, forceRefresh: true)
    }

    func toggle(_ entry: ProjectFileEntry) async {
        if entry.isDirectory {
            if expandedPaths.contains(entry.path) {
                expandedPaths.remove(entry.path)
                persistExpandedPaths()
                rebuildVisibleEntries()
            } else {
                expandedPaths.insert(entry.path)
                persistExpandedPaths()
                if childrenByPath[entry.path] == nil {
                    await loadChildren(of: entry.path, forceRefresh: false)
                } else {
                    rebuildVisibleEntries()
                }
            }
            return
        }
        await selectFile(entry)
    }

    func selectSearchResult(_ entry: ProjectFileEntry) async {
        if entry.isDirectory {
            searchText = ""
            searchResults = []
            contentSearchResults = []
            expandedPaths.insert(entry.path)
            persistExpandedPaths()
            await loadChildren(of: entry.path, forceRefresh: false)
        } else {
            await selectFile(entry, targetLine: nil)
        }
    }

    func selectContentSearchResult(_ match: ProjectFileContentMatch) async {
        let entry = ProjectFileEntry(
            name: URL(fileURLWithPath: match.path).lastPathComponent,
            path: match.path,
            displayPath: match.displayPath,
            isDirectory: false,
            isWritable: true
        )
        await selectFile(entry, targetLine: match.line)
    }

    func openGitChange(_ change: ProjectGitChange) async {
        let entry = ProjectFileEntry(
            name: URL(fileURLWithPath: change.path).lastPathComponent,
            path: change.absolutePath,
            displayPath: change.path,
            isDirectory: false,
            isWritable: true
        )
        await selectFile(entry)
    }

    func scheduleSearch() {
        searchTask?.cancel()
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let rootPath = rootPath?.trimmedNonEmpty, !query.isEmpty else {
            searchResults = []
            contentSearchResults = []
            isSearching = false
            return
        }
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(280))
            guard !Task.isCancelled, let self else { return }
            isSearching = true
            errorMessage = nil
            let service = service
            async let entries = captureDirectorySearch {
                try await service.searchEntries(path: rootPath, query: query, limit: 100)
            }
            async let content = captureDirectorySearch {
                try await service.searchContent(path: rootPath, query: query, limit: 200)
            }
            let values = await (entries, content)
            guard !Task.isCancelled,
                  searchText.trimmingCharacters(in: .whitespacesAndNewlines) == query else { return }
            var errors: [String] = []
            switch values.0 {
            case let .success(results): searchResults = results
            case let .failure(message):
                searchResults = []
                errors.append("文件名搜索：\(message)")
            }
            switch values.1 {
            case let .success(results): contentSearchResults = results
            case let .failure(message):
                contentSearchResults = []
                errors.append("内容搜索：\(message)")
            }
            errorMessage = errors.isEmpty ? nil : errors.joined(separator: "；")
            isSearching = false
        }
    }

    func beginEditing() {
        guard selectedFile?.isWritable == true, selectedFile?.isBinary == false else { return }
        isEditing = true
    }

    func selectSymbol(_ selection: ProjectCodeSymbolSelection?) {
        navigationTask?.cancel()
        guard let selection else {
            selectedSymbol = nil
            navigationResult = nil
            navigationRequestKind = nil
            navigationError = nil
            return
        }
        if selectedSymbol == selection { return }
        selectedSymbol = selection
        navigationResult = nil
        navigationRequestKind = nil
        navigationError = nil
        navigationTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(260))
            guard !Task.isCancelled, let self else { return }
            await requestNavigation(.references)
        }
    }

    func requestNavigation(_ kind: NavigationRequestKind) async {
        guard let rootPath = rootPath?.trimmedNonEmpty,
              let filePath = selectedFile?.path,
              let selection = selectedSymbol else {
            navigationError = "请先在代码中点击类、方法或变量。"
            return
        }
        isNavigating = true
        defer { isNavigating = false }
        navigationRequestKind = kind
        navigationError = nil
        do {
            let result: ProjectCodeNavigationResult
            switch kind {
            case .definition:
                result = try await codeNavigationService.definition(
                    projectRoot: rootPath,
                    filePath: filePath,
                    line: selection.line,
                    column: selection.column
                )
            case .references:
                result = try await codeNavigationService.references(
                    projectRoot: rootPath,
                    filePath: filePath,
                    line: selection.line,
                    column: selection.column
                )
            }
            guard selectedFile?.path == filePath, selectedSymbol == selection else { return }
            navigationResult = result
            if result.locations.isEmpty {
                navigationError = kind == .definition ? "没有找到可跳转的定义。" : "没有找到其他引用。"
            }
        } catch is CancellationError {
            return
        } catch {
            navigationResult = nil
            navigationError = error.localizedDescription
        }
    }

    func openNavigationLocation(_ location: ProjectCodeNavigationLocation) async {
        if let currentPath = selectedFile?.path {
            let point = NavigationPoint(
                path: currentPath,
                displayPath: selectedFile?.name ?? currentPath,
                line: selectedSymbol?.line ?? selectedLine ?? 1
            )
            if navigationHistory.last != point { navigationHistory.append(point) }
        }
        let entry = ProjectFileEntry(
            name: URL(fileURLWithPath: location.relativePath).lastPathComponent,
            path: location.path,
            displayPath: location.relativePath,
            isDirectory: false,
            isWritable: true
        )
        await selectFile(entry, targetLine: location.line, preserveNavigationHistory: true)
    }

    func navigateBack() async {
        guard let point = navigationHistory.popLast() else { return }
        let entry = ProjectFileEntry(
            name: URL(fileURLWithPath: point.displayPath).lastPathComponent,
            path: point.path,
            displayPath: point.displayPath,
            isDirectory: false,
            isWritable: true
        )
        await selectFile(entry, targetLine: point.line, preserveNavigationHistory: true)
    }

    func clearNavigationResults() {
        navigationTask?.cancel()
        selectedSymbol = nil
        navigationResult = nil
        navigationRequestKind = nil
        navigationError = nil
    }

    func cancelEditing() {
        draft = selectedFile?.content ?? ""
        isEditing = false
    }

    func save() async {
        guard let path = selectedFile?.path else { return }
        isSaving = true
        errorMessage = nil
        do {
            try await service.writeFile(path: path, content: draft)
            selectedFile = try await service.readFile(path: path)
            draft = selectedFile?.content ?? draft
            selectedLine = nil
            clearCodeNavigation(clearHistory: false)
            isEditing = false
        } catch {
            errorMessage = error.localizedDescription
        }
        isSaving = false
    }

    func create(name: String, directory: Bool) async {
        guard let parent = creationParentPath else { return }
        errorMessage = nil
        do {
            if directory {
                try await service.createDirectory(parentPath: parent, name: name)
            } else {
                try await service.createFile(parentPath: parent, name: name)
            }
            expandedPaths.insert(parent)
            persistExpandedPaths()
            await loadChildren(of: parent, forceRefresh: true)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func deleteSelected() async {
        guard let selectedPath, selectedPath != rootPath else { return }
        let selected = allEntries.first(where: { $0.path == selectedPath })
        do {
            try await service.deleteEntry(path: selectedPath, recursive: selected?.isDirectory == true)
            let parent = parentPath(of: selectedPath)
            expandedPaths = expandedPaths.filter { path in
                path != selectedPath && !path.hasPrefix(selectedPath + "/")
            }
            persistExpandedPaths()
            self.selectedPath = nil
            selectedFile = nil
            selectedLine = nil
            clearCodeNavigation(clearHistory: true)
            if let parent { await loadChildren(of: parent, forceRefresh: true) }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func openSelected(mode: ProjectFileExternalOpenMode) async {
        guard let selectedPath else { return }
        do { try await service.openExternally(path: selectedPath, mode: mode) }
        catch { errorMessage = error.localizedDescription }
    }

    func dismissError() { errorMessage = nil }

    private var allEntries: [ProjectFileEntry] { childrenByPath.values.flatMap { $0 } }

    private var creationParentPath: String? {
        guard let rootPath = rootPath?.trimmedNonEmpty else { return nil }
        guard let selectedPath else { return rootPath }
        if allEntries.first(where: { $0.path == selectedPath })?.isDirectory == true { return selectedPath }
        return parentPath(of: selectedPath) ?? rootPath
    }

    private func selectFile(
        _ entry: ProjectFileEntry,
        targetLine: Int? = nil,
        preserveNavigationHistory: Bool = false
    ) async {
        clearCodeNavigation(clearHistory: !preserveNavigationHistory)
        selectedPath = entry.path
        selectedLine = targetLine
        isLoading = true
        errorMessage = nil
        do {
            let file = try await service.readFile(path: entry.path)
            selectedFile = file
            draft = file.content
            isEditing = false
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    private func clearCodeNavigation(clearHistory: Bool) {
        navigationTask?.cancel()
        selectedSymbol = nil
        navigationResult = nil
        navigationRequestKind = nil
        navigationError = nil
        isNavigating = false
        if clearHistory { navigationHistory.removeAll() }
    }

    private func loadChildren(of path: String, forceRefresh: Bool) async {
        isLoading = true
        errorMessage = nil
        do {
            let listing = try await service.listEntries(path: path, forceRefresh: forceRefresh)
            childrenByPath[path] = listing.entries.sorted(by: entrySort)
            await restoreExpandedDescendants(of: path, forceRefresh: forceRefresh)
            rebuildVisibleEntries()
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    private func loadChildrenWithConnectorRetry(of path: String) async {
        isLoading = true
        errorMessage = nil
        for attempt in 0..<12 {
            do {
                let listing = try await service.listEntries(path: path, forceRefresh: attempt > 0)
                childrenByPath[path] = listing.entries.sorted(by: entrySort)
                await restoreExpandedDescendants(of: path, forceRefresh: attempt > 0)
                rebuildVisibleEntries()
                isLoading = false
                return
            } catch {
                let detail = error.localizedDescription
                let connectorStarting = detail.localizedCaseInsensitiveContains("active session lease")
                    || detail.localizedCaseInsensitiveContains("connector")
                if connectorStarting && attempt < 11 {
                    try? await Task.sleep(for: .milliseconds(350))
                    continue
                }
                errorMessage = localizedDirectoryError(detail)
                break
            }
        }
        isLoading = false
    }

    private func localizedDirectoryError(_ detail: String) -> String {
        if detail.localizedCaseInsensitiveContains("active session lease") {
            return "本机网关正在迁移项目目录连接，请稍后点击刷新。"
        }
        if detail.localizedCaseInsensitiveContains("connector") {
            return "本机网关尚未连接，项目目录暂时不可用。"
        }
        return detail
    }

    private func rebuildVisibleEntries() {
        guard let rootPath else { visibleEntries = []; return }
        var result: [VisibleEntry] = []
        func appendChildren(of path: String, depth: Int) {
            for entry in childrenByPath[path] ?? [] {
                result.append(VisibleEntry(
                    entry: entry,
                    depth: depth,
                    isExpanded: entry.isDirectory && expandedPaths.contains(entry.path)
                ))
                if entry.isDirectory, expandedPaths.contains(entry.path) {
                    appendChildren(of: entry.path, depth: depth + 1)
                }
            }
        }
        appendChildren(of: rootPath, depth: 0)
        visibleEntries = result
    }

    private func restoreExpandedDescendants(of path: String, forceRefresh: Bool) async {
        let expandedChildren = (childrenByPath[path] ?? []).filter {
            $0.isDirectory && expandedPaths.contains($0.path)
        }
        for entry in expandedChildren {
            do {
                let listing = try await service.listEntries(
                    path: entry.path,
                    forceRefresh: forceRefresh
                )
                childrenByPath[entry.path] = listing.entries.sorted(by: entrySort)
                await restoreExpandedDescendants(of: entry.path, forceRefresh: forceRefresh)
            } catch {
                // 保留用户的展开偏好；目录暂时不可访问时，下次进入仍可重试。
                continue
            }
        }
    }

    private func persistExpandedPaths() {
        expansionStateStore.saveExpandedPaths(expandedPaths)
    }

    private func entrySort(_ lhs: ProjectFileEntry, _ rhs: ProjectFileEntry) -> Bool {
        if lhs.isDirectory != rhs.isDirectory { return lhs.isDirectory }
        return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
    }

    private func parentPath(of path: String) -> String? {
        guard let slash = path.lastIndex(of: "/") else { return nil }
        return String(path[..<slash])
    }
}

private struct NavigationPoint: Equatable {
    let path: String
    let displayPath: String
    let line: Int
}

private enum DirectorySearchResult<Value: Sendable>: Sendable {
    case success(Value)
    case failure(String)
}

private func captureDirectorySearch<Value: Sendable>(
    _ operation: @escaping @Sendable () async throws -> Value
) async -> DirectorySearchResult<Value> {
    do {
        return .success(try await operation())
    } catch {
        return .failure(error.localizedDescription)
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
