import ChatOSCore
import Foundation

enum PetFileOpenMode: Sendable, Equatable {
    case preview
    case edit
}

enum PetFileAccess: Sendable, Equatable {
    case workspace
    case userSelectedLocal
}

struct PetFileOpenRequest: Sendable, Equatable {
    var path: String
    var targetLine: Int?
    var mode: PetFileOpenMode
    var access: PetFileAccess

    init(
        path: String,
        targetLine: Int? = nil,
        mode: PetFileOpenMode = .preview,
        access: PetFileAccess = .workspace
    ) {
        self.path = path
        self.targetLine = targetLine
        self.mode = mode
        self.access = access
    }
}

struct ResolvedPetFileLink: Sendable, Equatable {
    var path: String
    var targetLine: Int?
}

enum PetFileLinkResolver {
    static func resolve(_ url: URL, projectRootPath: String?) -> ResolvedPetFileLink? {
        let scheme = url.scheme?.lowercased()
        let rawTarget: String
        switch scheme {
        case "file":
            rawTarget = url.path
        case "local":
            rawTarget = url.absoluteString
        case nil, "":
            rawTarget = url.relativeString
        default:
            let candidate = url.absoluteString
            guard !candidate.contains("://"), pathAndLine(candidate).line != nil else {
                return nil
            }
            rawTarget = candidate
        }

        guard var decoded = rawTarget.removingPercentEncoding?.trimmingCharacters(
            in: .whitespacesAndNewlines
        ), !decoded.isEmpty, !decoded.hasPrefix("#") else {
            return nil
        }

        let fragmentLine = fragmentLine(decoded)
        if let fragmentIndex = decoded.firstIndex(of: "#") {
            decoded = String(decoded[..<fragmentIndex])
        }
        if let queryIndex = decoded.firstIndex(of: "?") {
            decoded = String(decoded[..<queryIndex])
        }
        let parsed = pathAndLine(decoded)
        var path = parsed.path

        if !path.hasPrefix("/") && !path.lowercased().hasPrefix("local://") {
            guard let root = projectRootPath?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !root.isEmpty else { return nil }
            let relative = path
                .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                .replacingOccurrences(of: "./", with: "", options: .anchored)
            if root.lowercased().hasPrefix("local://") {
                path = root.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                    + "/" + relative
            } else {
                path = URL(fileURLWithPath: root)
                    .appendingPathComponent(relative)
                    .standardizedFileURL.path
            }
        }

        return ResolvedPetFileLink(path: path, targetLine: fragmentLine ?? parsed.line)
    }

    private static func fragmentLine(_ target: String) -> Int? {
        guard let hashIndex = target.firstIndex(of: "#") else { return nil }
        let fragment = target[target.index(after: hashIndex)...]
        let normalized = fragment.hasPrefix("L") || fragment.hasPrefix("l")
            ? fragment.dropFirst()
            : fragment[...]
        return Int(normalized.prefix(while: \Character.isNumber))
    }

    private static func pathAndLine(_ target: String) -> (path: String, line: Int?) {
        var path = target
        var trailingNumbers: [Int] = []
        for _ in 0..<2 {
            guard let colon = path.lastIndex(of: ":"),
                  let value = Int(path[path.index(after: colon)...]) else { break }
            trailingNumbers.insert(value, at: 0)
            path = String(path[..<colon])
        }
        return (path, trailingNumbers.first)
    }
}

enum PetFileTabLoadState: Equatable {
    case loading
    case ready
    case failed(String)
}

struct PetFileTab: Identifiable, Equatable {
    let id: String
    var path: String
    var access: PetFileAccess
    var name: String
    var targetLine: Int?
    var loadState: PetFileTabLoadState
    var file: ProjectFileContent?
    var originalContent: String
    var draft: String
    var isEditing: Bool
    var isSaving: Bool
    var errorMessage: String?

    var isDirty: Bool {
        guard file?.isBinary == false else { return false }
        return draft != originalContent
    }
}

struct PetFileCloseRequest: Identifiable, Equatable {
    enum Target: Equatable {
        case tab(String)
        case workbench
    }

    let id = UUID()
    let target: Target
}

struct PetFileSaveConflict: Identifiable, Equatable {
    var id: String { tabID }
    let tabID: String
    let diskFile: ProjectFileContent
}

@MainActor
final class PetFileWorkbenchStore: ObservableObject {
    @Published private(set) var tabs: [PetFileTab] = []
    @Published var selectedTabID: String?
    @Published private(set) var isPresented = false
    @Published var pendingCloseRequest: PetFileCloseRequest?
    @Published var saveConflict: PetFileSaveConflict?

    private let workspaceService: any ProjectFilesystemServicing
    private let localFileService: any ProjectFilesystemServicing
    private var loadTasks: [String: Task<Void, Never>] = [:]

    init(
        service: any ProjectFilesystemServicing,
        localFileService: any ProjectFilesystemServicing = NativePetLocalFileService()
    ) {
        self.workspaceService = service
        self.localFileService = localFileService
    }

    deinit {
        loadTasks.values.forEach { $0.cancel() }
    }

    var selectedTab: PetFileTab? {
        guard let selectedTabID else { return nil }
        return tabs.first(where: { $0.id == selectedTabID })
    }

    var hasDirtyTabs: Bool {
        tabs.contains(where: \.isDirty)
    }

    func open(_ request: PetFileOpenRequest) {
        let path = request.path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !path.isEmpty else { return }
        isPresented = true

        if let index = tabs.firstIndex(where: { $0.path == path }) {
            tabs[index].targetLine = request.targetLine
            if request.access == .userSelectedLocal {
                tabs[index].access = .userSelectedLocal
            }
            if request.mode == .edit,
               tabs[index].file?.isWritable == true,
               tabs[index].file?.isBinary == false {
                tabs[index].isEditing = true
            }
            selectedTabID = tabs[index].id
            if case .failed = tabs[index].loadState {
                load(tabID: tabs[index].id, requestedMode: request.mode)
            }
            return
        }

        let id = path
        tabs.append(PetFileTab(
            id: id,
            path: path,
            access: request.access,
            name: Self.displayName(for: path),
            targetLine: request.targetLine,
            loadState: .loading,
            file: nil,
            originalContent: "",
            draft: "",
            isEditing: false,
            isSaving: false,
            errorMessage: nil
        ))
        selectedTabID = id
        load(tabID: id, requestedMode: request.mode)
    }

    func selectTab(_ id: String) {
        guard tabs.contains(where: { $0.id == id }) else { return }
        selectedTabID = id
    }

    func beginEditing(tabID: String? = nil) {
        guard let index = index(for: tabID ?? selectedTabID),
              tabs[index].file?.isWritable == true,
              tabs[index].file?.isBinary == false else { return }
        tabs[index].isEditing = true
    }

    func cancelEditing(tabID: String? = nil) {
        guard let index = index(for: tabID ?? selectedTabID) else { return }
        tabs[index].draft = tabs[index].originalContent
        tabs[index].isEditing = false
        tabs[index].errorMessage = nil
    }

    func updateDraft(_ value: String, tabID: String? = nil) {
        guard let index = index(for: tabID ?? selectedTabID) else { return }
        tabs[index].draft = value
    }

    func retry(tabID: String? = nil) {
        guard let id = tabID ?? selectedTabID,
              let index = index(for: id) else { return }
        tabs[index].loadState = .loading
        tabs[index].errorMessage = nil
        load(tabID: id, requestedMode: tabs[index].isEditing ? .edit : .preview)
    }

    func reload(tabID: String? = nil) {
        guard let id = tabID ?? selectedTabID,
              let index = index(for: id) else { return }
        tabs[index].loadState = .loading
        tabs[index].errorMessage = nil
        load(tabID: id, requestedMode: tabs[index].isEditing ? .edit : .preview)
    }

    @discardableResult
    func save(tabID: String? = nil, force: Bool = false) async -> Bool {
        guard let id = tabID ?? selectedTabID,
              let initialIndex = index(for: id),
              let originalFile = tabs[initialIndex].file,
              !originalFile.isBinary,
              originalFile.isWritable else { return false }

        let draftToSave = tabs[initialIndex].draft
        let service = service(for: tabs[initialIndex].access)
        if draftToSave == tabs[initialIndex].originalContent {
            tabs[initialIndex].errorMessage = nil
            return true
        }

        tabs[initialIndex].isSaving = true
        tabs[initialIndex].errorMessage = nil
        defer {
            if let currentIndex = index(for: id) {
                tabs[currentIndex].isSaving = false
            }
        }

        do {
            let diskFile = try await service.readFile(path: originalFile.path)
            guard let currentIndex = index(for: id) else { return false }
            if !force, diskFile.content != tabs[currentIndex].originalContent {
                saveConflict = PetFileSaveConflict(tabID: id, diskFile: diskFile)
                return false
            }

            try await service.writeFile(path: originalFile.path, content: draftToSave)
            let refreshed = try await service.readFile(path: originalFile.path)
            guard let refreshedIndex = index(for: id) else { return false }
            tabs[refreshedIndex].file = refreshed
            tabs[refreshedIndex].loadState = .ready
            tabs[refreshedIndex].originalContent = refreshed.content
            if tabs[refreshedIndex].draft == draftToSave {
                tabs[refreshedIndex].draft = refreshed.content
            }
            tabs[refreshedIndex].errorMessage = nil
            return true
        } catch is CancellationError {
            return false
        } catch {
            if let currentIndex = index(for: id) {
                tabs[currentIndex].errorMessage = error.localizedDescription
            }
            return false
        }
    }

    func requestCloseTab(_ id: String) {
        guard let index = index(for: id) else { return }
        if tabs[index].isDirty {
            selectedTabID = id
            pendingCloseRequest = PetFileCloseRequest(target: .tab(id))
        } else {
            removeTab(id)
        }
    }

    func requestDismiss() {
        if hasDirtyTabs {
            pendingCloseRequest = PetFileCloseRequest(target: .workbench)
        } else {
            isPresented = false
        }
    }

    func cancelPendingClose() {
        pendingCloseRequest = nil
    }

    func discardPendingClose() {
        guard let request = pendingCloseRequest else { return }
        pendingCloseRequest = nil
        switch request.target {
        case let .tab(id):
            removeTab(id)
        case .workbench:
            for index in tabs.indices where tabs[index].isDirty {
                tabs[index].draft = tabs[index].originalContent
                tabs[index].isEditing = false
            }
            isPresented = false
        }
    }

    func savePendingClose() async {
        guard let request = pendingCloseRequest else { return }
        pendingCloseRequest = nil
        switch request.target {
        case let .tab(id):
            let saved = await save(tabID: id)
            if saved, index(for: id).map({ !tabs[$0].isDirty }) == true {
                removeTab(id)
            }
        case .workbench:
            let dirtyIDs = tabs.filter(\.isDirty).map(\.id)
            for id in dirtyIDs {
                guard await save(tabID: id),
                      index(for: id).map({ !tabs[$0].isDirty }) == true else { return }
            }
            isPresented = false
        }
    }

    func cancelSaveConflict() {
        saveConflict = nil
    }

    func reloadConflictingFile() {
        guard let conflict = saveConflict,
              let index = index(for: conflict.tabID) else {
            saveConflict = nil
            return
        }
        tabs[index].file = conflict.diskFile
        tabs[index].originalContent = conflict.diskFile.content
        tabs[index].draft = conflict.diskFile.content
        tabs[index].loadState = .ready
        tabs[index].errorMessage = nil
        saveConflict = nil
    }

    func overwriteConflictingFile() async {
        guard let conflict = saveConflict else { return }
        saveConflict = nil
        _ = await save(tabID: conflict.tabID, force: true)
    }

    func openExternally(_ mode: ProjectFileExternalOpenMode) async {
        guard let tab = selectedTab else { return }
        let service = service(for: tab.access)
        do {
            try await service.openExternally(path: tab.path, mode: mode)
        } catch {
            guard let index = index(for: tab.id) else { return }
            tabs[index].errorMessage = error.localizedDescription
        }
    }

    private func load(tabID: String, requestedMode: PetFileOpenMode) {
        loadTasks[tabID]?.cancel()
        guard let tab = tabs.first(where: { $0.id == tabID }) else { return }
        let path = tab.path
        let service = service(for: tab.access)
        loadTasks[tabID] = Task { [weak self] in
            do {
                let file = try await service.readFile(path: path)
                guard !Task.isCancelled, let self,
                      let index = self.index(for: tabID) else { return }
                self.tabs[index].name = file.name
                self.tabs[index].file = file
                self.tabs[index].originalContent = file.content
                self.tabs[index].draft = file.content
                self.tabs[index].loadState = .ready
                self.tabs[index].isEditing = requestedMode == .edit
                    && file.isWritable
                    && !file.isBinary
                self.tabs[index].errorMessage = nil
                self.loadTasks[tabID] = nil
            } catch is CancellationError {
                self?.loadTasks[tabID] = nil
            } catch {
                guard let self, let index = self.index(for: tabID) else { return }
                self.tabs[index].loadState = .failed(error.localizedDescription)
                self.tabs[index].errorMessage = error.localizedDescription
                self.loadTasks[tabID] = nil
            }
        }
    }

    private func removeTab(_ id: String) {
        guard let removedIndex = index(for: id) else { return }
        loadTasks[id]?.cancel()
        loadTasks[id] = nil
        tabs.remove(at: removedIndex)
        if selectedTabID == id {
            if tabs.indices.contains(removedIndex) {
                selectedTabID = tabs[removedIndex].id
            } else {
                selectedTabID = tabs.last?.id
            }
        }
        if tabs.isEmpty {
            isPresented = false
        }
    }

    private func index(for id: String?) -> Int? {
        guard let id else { return nil }
        return tabs.firstIndex(where: { $0.id == id })
    }

    private func service(for access: PetFileAccess) -> any ProjectFilesystemServicing {
        switch access {
        case .workspace:
            workspaceService
        case .userSelectedLocal:
            localFileService
        }
    }

    private static func displayName(for path: String) -> String {
        if let components = URLComponents(string: path), components.scheme != nil {
            return components.path.split(separator: "/").last.map(String.init) ?? path
        }
        return URL(fileURLWithPath: path).lastPathComponent
    }
}
