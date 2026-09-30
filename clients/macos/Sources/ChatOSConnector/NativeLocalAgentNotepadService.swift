import ChatOSCore
import Foundation

public actor NativeLocalAgentNotepadService: NotepadServicing {
    private let client: NativeLocalAgentNotepadClient?
    private var ownerUserID: String?
    private var noteVersions: [String: UInt64] = [:]

    public init(host: (any LocalAgentHostClientServicing)?) {
        client = host.map(NativeLocalAgentNotepadClient.init(host:))
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
        noteVersions.removeAll()
    }

    public func reset() {
        ownerUserID = nil
        noteVersions.removeAll()
    }

    public func initialize() async throws {
        let context = try requireContext()
        _ = try await context.client.initialize(ownerUserID: context.ownerUserID)
    }

    public func listFolders() async throws -> [String] {
        let context = try requireContext()
        return try await context.client.listFolders(ownerUserID: context.ownerUserID)
    }

    public func createFolder(_ folder: String) async throws {
        let context = try requireContext()
        try await context.client.createFolder(ownerUserID: context.ownerUserID, folder: folder)
    }

    public func renameFolder(from: String, to: String) async throws {
        let context = try requireContext()
        try await context.client.renameFolder(ownerUserID: context.ownerUserID, from: from, to: to)
        noteVersions.removeAll()
    }

    public func deleteFolder(_ folder: String, recursive: Bool) async throws {
        let context = try requireContext()
        try await context.client.deleteFolder(
            ownerUserID: context.ownerUserID,
            folder: folder,
            recursive: recursive
        )
        noteVersions.removeAll()
    }

    public func listNotes(query: String?, limit: Int) async throws -> [NotepadNote] {
        let context = try requireContext()
        let records = try await context.client.listNotes(
            ownerUserID: context.ownerUserID,
            query: query,
            limit: UInt32(clamping: min(max(limit, 1), 500))
        )
        for record in records { noteVersions[record.noteID] = record.version }
        return records.map(Self.domainNote)
    }

    public func createNote(_ draft: NotepadNoteDraft) async throws -> NotepadNoteDetail {
        let context = try requireContext()
        let detail = try await context.client.createNote(ownerUserID: context.ownerUserID, draft: draft)
        return cache(detail)
    }

    public func fetchNote(id: String) async throws -> NotepadNoteDetail {
        let context = try requireContext()
        let detail = try await context.client.getNote(ownerUserID: context.ownerUserID, noteID: id)
        return cache(detail)
    }

    public func updateNote(id: String, update: NotepadNoteUpdate) async throws -> NotepadNoteDetail {
        let context = try requireContext()
        let version = try await version(for: id, context: context)
        let detail = try await context.client.updateNote(
            ownerUserID: context.ownerUserID,
            noteID: id,
            expectedVersion: version,
            update: update
        )
        return cache(detail)
    }

    public func uploadImage(
        _ image: NotepadImageUpload,
        noteID: String
    ) async throws -> NotepadImageAsset {
        let context = try requireContext()
        let record = try await context.client.putImage(
            ownerUserID: context.ownerUserID,
            noteID: noteID,
            image: image
        )
        guard let url = URL(string: record.dataURL), record.size <= UInt64(Int.max) else {
            throw NativeLocalAgentNotepadError.invalidResponse
        }
        return NotepadImageAsset(
            url: url,
            mimeType: record.mimeType,
            name: record.name,
            size: Int(record.size)
        )
    }

    public func deleteNote(id: String) async throws {
        let context = try requireContext()
        let version = try await version(for: id, context: context)
        try await context.client.deleteNote(
            ownerUserID: context.ownerUserID,
            noteID: id,
            expectedVersion: version
        )
        noteVersions[id] = nil
    }

    private func version(for noteID: String, context: Context) async throws -> UInt64 {
        if let version = noteVersions[noteID] { return version }
        let detail = try await context.client.getNote(
            ownerUserID: context.ownerUserID,
            noteID: noteID
        )
        noteVersions[noteID] = detail.note.version
        return detail.note.version
    }

    private func cache(_ detail: LocalNotepadNoteDetailRecord) -> NotepadNoteDetail {
        noteVersions[detail.note.noteID] = detail.note.version
        return NotepadNoteDetail(note: Self.domainNote(detail.note), content: detail.content)
    }

    private func requireContext() throws -> Context {
        guard let client, let ownerUserID else {
            throw NativeLocalAgentNotepadError.notConfigured
        }
        return Context(client: client, ownerUserID: ownerUserID)
    }

    private static func domainNote(_ record: LocalNotepadNoteRecord) -> NotepadNote {
        NotepadNote(
            id: record.noteID,
            title: record.title,
            folder: record.folder,
            tags: record.tags,
            createdAt: Date(timeIntervalSince1970: Double(record.createdAtUnixMs) / 1_000),
            updatedAt: Date(timeIntervalSince1970: Double(record.updatedAtUnixMs) / 1_000),
            file: record.file
        )
    }

    private struct Context: Sendable {
        let client: NativeLocalAgentNotepadClient
        let ownerUserID: String
    }
}

public enum NativeLocalAgentNotepadError: LocalizedError, Equatable {
    case notConfigured
    case invalidResponse

    public var errorDescription: String? {
        switch self {
        case .notConfigured:
            "本地记事本尚未连接到当前账号。"
        case .invalidResponse:
            "本地记事本返回了无效数据。"
        }
    }
}
