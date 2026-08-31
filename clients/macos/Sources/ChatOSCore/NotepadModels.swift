import Foundation

public struct NotepadNote: Identifiable, Hashable, Sendable {
    public var id: String
    public var title: String
    public var folder: String
    public var tags: [String]
    public var createdAt: Date?
    public var updatedAt: Date?
    public var file: String

    public init(
        id: String,
        title: String,
        folder: String,
        tags: [String],
        createdAt: Date?,
        updatedAt: Date?,
        file: String
    ) {
        self.id = id
        self.title = title
        self.folder = folder
        self.tags = tags
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.file = file
    }
}

public struct NotepadNoteDetail: Hashable, Sendable {
    public var note: NotepadNote
    public var content: String

    public init(note: NotepadNote, content: String) {
        self.note = note
        self.content = content
    }
}

public struct NotepadNoteDraft: Hashable, Sendable {
    public var folder: String
    public var title: String
    public var content: String
    public var tags: [String]

    public init(
        folder: String = "",
        title: String = "",
        content: String = "",
        tags: [String] = []
    ) {
        self.folder = folder
        self.title = title
        self.content = content
        self.tags = tags
    }
}

public struct NotepadNoteUpdate: Hashable, Sendable {
    public var title: String?
    public var content: String?
    public var folder: String?
    public var tags: [String]?

    public init(
        title: String? = nil,
        content: String? = nil,
        folder: String? = nil,
        tags: [String]? = nil
    ) {
        self.title = title
        self.content = content
        self.folder = folder
        self.tags = tags
    }
}

public protocol NotepadServicing: Sendable {
    func initialize() async throws
    func listFolders() async throws -> [String]
    func createFolder(_ folder: String) async throws
    func renameFolder(from: String, to: String) async throws
    func deleteFolder(_ folder: String, recursive: Bool) async throws
    func listNotes(query: String?, limit: Int) async throws -> [NotepadNote]
    func createNote(_ draft: NotepadNoteDraft) async throws -> NotepadNoteDetail
    func fetchNote(id: String) async throws -> NotepadNoteDetail
    func updateNote(id: String, update: NotepadNoteUpdate) async throws -> NotepadNoteDetail
    func deleteNote(id: String) async throws
}
