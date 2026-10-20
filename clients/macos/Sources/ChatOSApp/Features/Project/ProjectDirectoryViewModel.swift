import ChatOSCore
import Foundation

enum ProjectDirectoryMemoryPolicy {
    static let maximumNavigationHistory = 128

    static func isCachedPath(_ cachedPath: String, insideSubtree path: String) -> Bool {
        cachedPath == path || cachedPath.hasPrefix(path + "/")
    }

    static func trimNavigationHistory<Value>(_ values: inout [Value]) {
        let overflow = values.count - maximumNavigationHistory
        if overflow > 0 { values.removeFirst(overflow) }
    }

    static func staleExpandedSubtreeRoots(
        expandedPaths: Set<String>,
        parentPath: String,
        availableDirectoryPaths: Set<String>,
        isListingTruncated: Bool
    ) -> Set<String> {
        guard !isListingTruncated else { return [] }
        return Set(expandedPaths.filter { path in
            directParentPath(of: path) == parentPath && !availableDirectoryPaths.contains(path)
        })
    }

    private static func directParentPath(of path: String) -> String? {
        guard let slash = path.lastIndex(of: "/") else { return nil }
        return String(path[..<slash])
    }
}

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
    @Published private(set) var isMutating = false
    @Published private(set) var errorMessage: String?

    let rootPath: String?
    private let service: any ProjectFilesystemServicing
    private let mutationUseCase: ProjectFilesystemMutationUseCase
    private let codeNavigationService: any ProjectCodeNavigationServicing
    private let expansionStateStore: any ProjectDirectoryExpansionStateStoring
    private var childrenByPath: [String: [ProjectFileEntry]] = [:]
    private var expandedPaths: Set<String>
    private var searchTask: Task<Void, Never>?
    private var navigationTask: Task<Void, Never>?
    private var navigationHistory: [NavigationPoint] = []
    private var mutationSequence = 0
    private var activeMutation: MutationRequest?

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
        self.mutationUseCase = ProjectFilesystemMutationUseCase(service: service)
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
        // A missing root is a valid not-yet-resolved state and already has a dedicated empty
        // presentation in ProjectDirectoryView. It is not a failed filesystem operation.
        guard let rootPath = rootPath?.trimmedNonEmpty else { return }
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
                evictCachedSubtree(at: entry.path)
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
            ProjectDirectoryMemoryPolicy.trimNavigationHistory(&navigationHistory)
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
        guard let path = selectedFile?.path,
              let request = beginMutation(kind: .save, path: path) else { return }
        defer { finishMutation(request) }
        let content = draft
        isSaving = true
        errorMessage = nil
        do {
            let result = try await mutationUseCase.save(path: path, content: content)
            guard acceptsMutation(request) else { return }
            guard selectedPath == path else { return }
            if let file = result.refreshedFile {
                selectedFile = file
                draft = file.content
            } else {
                selectedFile?.content = content
                draft = content
                errorMessage = "文件已保存，但最新内容暂时无法重新读取。"
            }
            selectedLine = nil
            clearCodeNavigation(clearHistory: false)
            isEditing = false
        } catch {
            guard acceptsMutation(request) else { return }
            errorMessage = error.localizedDescription
        }
    }

    func create(name: String, directory: Bool) async {
        guard let parent = creationParentPath,
              let request = beginMutation(kind: .create, path: parent) else { return }
        defer { finishMutation(request) }
        errorMessage = nil
        do {
            let result = try await mutationUseCase.create(
                parentPath: parent,
                name: name,
                kind: directory ? .directory : .file
            )
            guard acceptsMutation(request) else { return }
            expandedPaths.insert(parent)
            persistExpandedPaths()
            if let listing = result.refreshedListing {
                applyListing(listing, for: parent)
            } else {
                errorMessage = "项目文件已创建，但目录暂时无法刷新。"
            }
        } catch {
            guard acceptsMutation(request) else { return }
            errorMessage = error.localizedDescription
        }
    }

    func deleteSelected() async {
        guard let selectedPath, selectedPath != rootPath,
              let request = beginMutation(kind: .delete, path: selectedPath) else { return }
        defer { finishMutation(request) }
        let selected = allEntries.first(where: { $0.path == selectedPath })
        let parent = parentPath(of: selectedPath)
        do {
            let result = try await mutationUseCase.delete(
                path: selectedPath,
                parentPath: parent,
                isDirectory: selected?.isDirectory == true
            )
            guard acceptsMutation(request) else { return }
            expandedPaths = expandedPaths.filter { path in
                path != selectedPath && !path.hasPrefix(selectedPath + "/")
            }
            persistExpandedPaths()
            if self.selectedPath == selectedPath {
                self.selectedPath = nil
                selectedFile = nil
                selectedLine = nil
                clearCodeNavigation(clearHistory: true)
            }
            if let parent, let listing = result.refreshedListing {
                applyListing(listing, for: parent)
            } else {
                errorMessage = "项目文件已删除，但目录暂时无法刷新。"
            }
        } catch {
            guard acceptsMutation(request) else { return }
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

    private enum MutationKind: Equatable, Sendable {
        case save
        case create
        case delete
    }

    private struct MutationRequest: Equatable, Sendable {
        let id: Int
        let kind: MutationKind
        let path: String
    }

    private func beginMutation(kind: MutationKind, path: String) -> MutationRequest? {
        guard activeMutation == nil else {
            errorMessage = "另一个项目文件操作仍在进行，请稍候。"
            return nil
        }
        mutationSequence += 1
        let request = MutationRequest(id: mutationSequence, kind: kind, path: path)
        activeMutation = request
        isMutating = true
        return request
    }

    private func acceptsMutation(_ request: MutationRequest) -> Bool {
        activeMutation == request
    }

    private func finishMutation(_ request: MutationRequest) {
        guard acceptsMutation(request) else { return }
        activeMutation = nil
        isMutating = false
        if request.kind == .save { isSaving = false }
    }

    private func applyListing(_ listing: ProjectDirectoryListing, for path: String) {
        guard listing.path == path, shouldRetainChildren(of: path) else { return }
        reconcileExpandedPaths(with: listing)
        childrenByPath[path] = listing.entries.sorted(by: entrySort)
        rebuildVisibleEntries()
    }

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
            guard shouldRetainChildren(of: path) else {
                evictCachedSubtree(at: path)
                rebuildVisibleEntries()
                isLoading = false
                return
            }
            reconcileExpandedPaths(with: listing)
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
                guard shouldRetainChildren(of: path) else {
                    evictCachedSubtree(at: path)
                    rebuildVisibleEntries()
                    isLoading = false
                    return
                }
                reconcileExpandedPaths(with: listing)
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
        guard shouldRetainChildren(of: path) else {
            evictCachedSubtree(at: path)
            return
        }
        let expandedChildren = (childrenByPath[path] ?? []).filter {
            $0.isDirectory && expandedPaths.contains($0.path)
        }
        for entry in expandedChildren {
            do {
                let listing = try await service.listEntries(
                    path: entry.path,
                    forceRefresh: forceRefresh
                )
                guard expandedPaths.contains(entry.path) else {
                    evictCachedSubtree(at: entry.path)
                    continue
                }
                reconcileExpandedPaths(with: listing)
                childrenByPath[entry.path] = listing.entries.sorted(by: entrySort)
                await restoreExpandedDescendants(of: entry.path, forceRefresh: forceRefresh)
            } catch {
                // 保留用户的展开偏好；目录暂时不可访问时，下次进入仍可重试。
                continue
            }
        }
    }

    private func shouldRetainChildren(of path: String) -> Bool {
        path == rootPath || expandedPaths.contains(path)
    }

    private func evictCachedSubtree(at path: String) {
        let keys = childrenByPath.keys.filter {
            ProjectDirectoryMemoryPolicy.isCachedPath($0, insideSubtree: path)
        }
        for key in keys { childrenByPath.removeValue(forKey: key) }
    }

    private func reconcileExpandedPaths(with listing: ProjectDirectoryListing) {
        let directoryPaths = Set(listing.entries.lazy.filter(\.isDirectory).map(\.path))
        let staleRoots = ProjectDirectoryMemoryPolicy.staleExpandedSubtreeRoots(
            expandedPaths: expandedPaths,
            parentPath: listing.path,
            availableDirectoryPaths: directoryPaths,
            isListingTruncated: listing.isTruncated
        )
        guard !staleRoots.isEmpty else { return }
        expandedPaths = expandedPaths.filter { path in
            !staleRoots.contains { staleRoot in
                isCachedPath(path, insideSubtree: staleRoot)
            }
        }
        persistExpandedPaths()
    }

    private func isCachedPath(_ path: String, insideSubtree subtree: String) -> Bool {
        ProjectDirectoryMemoryPolicy.isCachedPath(path, insideSubtree: subtree)
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
