import ChatOSCore
import Combine
import Foundation

enum NotepadEditorMode: String, CaseIterable, Identifiable {
    case edit = "编辑"
    case preview = "预览"
    case split = "分栏"

    var id: Self { self }

    func title(language: ChatOSLanguage) -> String {
        guard language == .english else { return rawValue }
        return switch self {
        case .edit: "Edit"
        case .preview: "Preview"
        case .split: "Split"
        }
    }
}

struct NotepadTreeNode: Identifiable {
    enum Kind {
        case folder(String)
        case note(NotepadNote)
    }

    var id: String
    var title: String
    var subtitle: String?
    var kind: Kind
    var children: [NotepadTreeNode]?
}

@MainActor
final class NotepadViewModel: ObservableObject {
    @Published private(set) var folders: [String] = []
    @Published private(set) var notes: [NotepadNote] = []
    @Published private(set) var selectedNoteID: String?
    @Published private(set) var selectedTreeNodeID = "folder:"
    @Published var selectedFolder = ""
    @Published var searchQuery = ""
    @Published var title = ""
    @Published var tagsText = ""
    @Published var content = ""
    @Published var editorMode: NotepadEditorMode = .preview
    @Published private(set) var isLoading = false
    @Published private(set) var isLoadingNote = false
    @Published private(set) var isSaving = false
    @Published private(set) var errorMessage: String?
    @Published var interfaceLanguage: ChatOSLanguage = .simplifiedChinese

    private let service: any NotepadServicing
    private var initialized = false
    private var searchTask: Task<Void, Never>?
    private var savedTitle = ""
    private var savedTagsText = ""
    private var savedContent = ""

    init(service: any NotepadServicing) {
        self.service = service
    }

    deinit {
        searchTask?.cancel()
    }

    var selectedNote: NotepadNote? {
        selectedNoteID.flatMap { id in notes.first(where: { $0.id == id }) }
    }

    var isDirty: Bool {
        selectedNoteID != nil
            && (title != savedTitle || tagsText != savedTagsText || content != savedContent)
    }

    var tree: [NotepadTreeNode] {
        let allFolders = normalizedFolders()
        return childNodes(parent: "", allFolders: allFolders)
    }

    func load(force: Bool = false) async {
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            if !initialized || force {
                try await service.initialize()
                initialized = true
            }
            async let loadedFolders = service.listFolders()
            async let loadedNotes = service.listNotes(
                query: searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
                limit: 500
            )
            folders = normalizeFolderList(try await loadedFolders)
            notes = sortNotes(try await loadedNotes)
            if selectedNoteID == nil, let first = notes.first {
                await selectNote(first.id, savingCurrent: false)
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func scheduleSearch() {
        searchTask?.cancel()
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(260))
            guard !Task.isCancelled else { return }
            await self?.reloadNotes()
        }
    }

    func refresh() async {
        await load(force: true)
        if let selectedNoteID {
            await selectNote(selectedNoteID, savingCurrent: false, force: true)
        }
    }

    func syncExternalChanges() async {
        guard !isDirty, !isSaving, !isLoadingNote else { return }
        await load()
        if let selectedNoteID {
            await selectNote(selectedNoteID, savingCurrent: false, force: true)
        }
    }

    func selectFolder(_ folder: String) {
        selectedFolder = normalizeFolder(folder)
        selectedTreeNodeID = "folder:\(selectedFolder)"
    }

    func selectNote(
        _ id: String,
        savingCurrent: Bool = true,
        force: Bool = false
    ) async {
        let normalizedID = id.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedID.isEmpty else { return }
        if !force, selectedNoteID == normalizedID { return }
        if savingCurrent, isDirty, !(await save()) { return }

        isLoadingNote = true
        errorMessage = nil
        defer { isLoadingNote = false }
        do {
            apply(try await service.fetchNote(id: normalizedID))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createFolder(name: String, parent: String? = nil) async -> Bool {
        let base = normalizeFolder(parent ?? selectedFolder)
        let input = normalizeFolder(name)
        guard !input.isEmpty else { return false }
        let folder = base.isEmpty || input == base || input.hasPrefix(base + "/")
            ? input
            : base + "/" + input
        return await mutate {
            try await service.createFolder(folder)
            selectedFolder = folder
            try await reloadResources()
        }
    }

    func createNote(title: String, folder: String? = nil) async -> Bool {
        let targetFolder = normalizeFolder(folder ?? selectedFolder)
        return await mutate {
            let detail = try await service.createNote(.init(
                folder: targetFolder,
                title: title.trimmingCharacters(in: .whitespacesAndNewlines)
            ))
            selectedFolder = targetFolder
            upsert(detail.note)
            apply(detail)
            try await reloadFolders()
        }
    }

    func save() async -> Bool {
        guard let selectedNoteID else { return true }
        isSaving = true
        errorMessage = nil
        defer { isSaving = false }
        do {
            let detail = try await service.updateNote(
                id: selectedNoteID,
                update: .init(
                    title: title.trimmingCharacters(in: .whitespacesAndNewlines),
                    content: content,
                    tags: parseTags(tagsText)
                )
            )
            upsert(detail.note)
            apply(detail)
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func deleteNote(_ id: String) async -> Bool {
        await mutate {
            try await service.deleteNote(id: id)
            notes.removeAll(where: { $0.id == id })
            if selectedNoteID == id { resetEditor() }
            try await reloadResources()
        }
    }

    func deleteFolder(_ folder: String) async -> Bool {
        let normalized = normalizeFolder(folder)
        guard !normalized.isEmpty else { return false }
        return await mutate {
            try await service.deleteFolder(normalized, recursive: true)
            if selectedFolder == normalized || selectedFolder.hasPrefix(normalized + "/") {
                selectedFolder = ""
            }
            if let selected = selectedNote,
               selected.folder == normalized || selected.folder.hasPrefix(normalized + "/") {
                resetEditor()
            }
            try await reloadResources()
        }
    }

    func clearError() { errorMessage = nil }

    private func mutate(_ operation: () async throws -> Void) async -> Bool {
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            try await operation()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    private func reloadResources() async throws {
        async let loadedFolders = service.listFolders()
        async let loadedNotes = service.listNotes(
            query: searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
            limit: 500
        )
        folders = normalizeFolderList(try await loadedFolders)
        notes = sortNotes(try await loadedNotes)
    }

    private func reloadFolders() async throws {
        folders = normalizeFolderList(try await service.listFolders())
    }

    private func reloadNotes() async {
        do {
            notes = sortNotes(try await service.listNotes(
                query: searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
                limit: 500
            ))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func apply(_ detail: NotepadNoteDetail) {
        selectedNoteID = detail.note.id
        selectedTreeNodeID = "note:\(detail.note.id)"
        selectedFolder = normalizeFolder(detail.note.folder)
        title = detail.note.title
        tagsText = detail.note.tags.joined(separator: ", ")
        content = detail.content
        savedTitle = title
        savedTagsText = tagsText
        savedContent = content
        upsert(detail.note)
    }

    private func upsert(_ note: NotepadNote) {
        notes.removeAll(where: { $0.id == note.id })
        notes.append(note)
        notes = sortNotes(notes)
    }

    private func resetEditor() {
        selectedNoteID = nil
        selectedTreeNodeID = "folder:\(selectedFolder)"
        title = ""
        tagsText = ""
        content = ""
        savedTitle = ""
        savedTagsText = ""
        savedContent = ""
    }

    private func normalizedFolders() -> [String] {
        var values = Set(normalizeFolderList(folders))
        for note in notes {
            var path = normalizeFolder(note.folder)
            while !path.isEmpty {
                values.insert(path)
                path = parentFolder(of: path)
            }
        }
        return values.sorted { $0.localizedStandardCompare($1) == .orderedAscending }
    }

    private func childNodes(parent: String, allFolders: [String]) -> [NotepadTreeNode] {
        let folderNodes = allFolders
            .filter { parentFolder(of: $0) == parent }
            .map { folder -> NotepadTreeNode in
                let children = childNodes(parent: folder, allFolders: allFolders)
                return NotepadTreeNode(
                    id: "folder:\(folder)",
                    title: folder.split(separator: "/").last.map(String.init) ?? folder,
                    subtitle: folder,
                    kind: .folder(folder),
                    children: children.isEmpty ? nil : children
                )
            }
        let noteNodes = notes
            .filter { normalizeFolder($0.folder) == parent }
            .map { note in
                NotepadTreeNode(
                    id: "note:\(note.id)",
                    title: note.title.isEmpty
                        ? (interfaceLanguage == .english ? "Untitled Note" : "未命名笔记")
                        : note.title,
                    subtitle: note.updatedAt.map(dateFormatter.string),
                    kind: .note(note),
                    children: nil
                )
            }
        return folderNodes + noteNodes
    }

    private func normalizeFolderList(_ values: [String]) -> [String] {
        Array(Set(values.map(normalizeFolder).filter { !$0.isEmpty }))
            .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
    }

    private func normalizeFolder(_ value: String) -> String {
        value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\\", with: "/")
            .split(separator: "/", omittingEmptySubsequences: true)
            .filter { $0 != "." && $0 != ".." }
            .joined(separator: "/")
    }

    private func parentFolder(of value: String) -> String {
        let parts = normalizeFolder(value).split(separator: "/")
        return parts.dropLast().joined(separator: "/")
    }

    private func parseTags(_ value: String) -> [String] {
        var seen = Set<String>()
        return value
            .components(separatedBy: CharacterSet(charactersIn: ",，\n"))
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty && seen.insert($0).inserted }
    }

    private func sortNotes(_ values: [NotepadNote]) -> [NotepadNote] {
        values.sorted {
            let lhs = $0.updatedAt ?? $0.createdAt ?? .distantPast
            let rhs = $1.updatedAt ?? $1.createdAt ?? .distantPast
            if lhs != rhs { return lhs > rhs }
            return $0.title.localizedStandardCompare($1.title) == .orderedAscending
        }
    }

    private var dateFormatter: DateFormatter {
        let formatter = DateFormatter()
        formatter.dateStyle = .short
        formatter.timeStyle = .short
        formatter.locale = interfaceLanguage.locale
        return formatter
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
