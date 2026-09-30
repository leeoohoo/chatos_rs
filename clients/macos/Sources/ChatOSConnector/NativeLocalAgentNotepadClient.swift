import ChatOSCore
import Foundation

struct LocalNotepadNoteRecord: Decodable, Sendable, Equatable {
    let noteID: String
    let ownerUserID: String
    let title: String
    let folder: String
    let tags: [String]
    let file: String
    let version: UInt64
    let createdAtUnixMs: Int64
    let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case title, folder, tags, file, version
        case noteID = "note_id"
        case ownerUserID = "owner_user_id"
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

struct LocalNotepadNoteDetailRecord: Decodable, Sendable, Equatable {
    let note: LocalNotepadNoteRecord
    let content: String
}

struct LocalNotepadImageRecord: Decodable, Sendable, Equatable {
    let imageID: String
    let noteID: String
    let ownerUserID: String
    let name: String
    let mimeType: String
    let size: UInt64
    let sha256: String
    let dataURL: String
    let createdAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case name, size, sha256
        case imageID = "image_id"
        case noteID = "note_id"
        case ownerUserID = "owner_user_id"
        case mimeType = "mime_type"
        case dataURL = "data_url"
        case createdAtUnixMs = "created_at_unix_ms"
    }
}

struct NativeLocalAgentNotepadClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    func initialize(ownerUserID: String) async throws -> UInt64 {
        let response: InitializedResult = try await host.request(OwnerCommand(
            type: "initialize_notepad",
            ownerUserID: ownerUserID
        ))
        try require(response.type, expected: "notepad_initialized")
        return response.noteCount
    }

    func listFolders(ownerUserID: String) async throws -> [String] {
        let response: FoldersResult = try await host.request(OwnerCommand(
            type: "list_notepad_folders",
            ownerUserID: ownerUserID
        ))
        try require(response.type, expected: "notepad_folders")
        return response.folders
    }

    func createFolder(ownerUserID: String, folder: String) async throws {
        let response: FolderMutationResult = try await host.request(FolderCommand(
            type: "create_notepad_folder",
            ownerUserID: ownerUserID,
            folder: folder
        ))
        try require(response.type, expected: "notepad_folder_mutation")
    }

    func renameFolder(ownerUserID: String, from: String, to: String) async throws {
        let response: FolderMutationResult = try await host.request(RenameFolderCommand(
            type: "rename_notepad_folder",
            ownerUserID: ownerUserID,
            from: from,
            to: to
        ))
        try require(response.type, expected: "notepad_folder_mutation")
    }

    func deleteFolder(ownerUserID: String, folder: String, recursive: Bool) async throws {
        let response: FolderMutationResult = try await host.request(DeleteFolderCommand(
            type: "delete_notepad_folder",
            ownerUserID: ownerUserID,
            folder: folder,
            recursive: recursive
        ))
        try require(response.type, expected: "notepad_folder_mutation")
    }

    func listNotes(
        ownerUserID: String,
        query: String?,
        limit: UInt32
    ) async throws -> [LocalNotepadNoteRecord] {
        let response: NotesResult = try await host.request(ListNotesCommand(
            type: "list_notepad_notes",
            ownerUserID: ownerUserID,
            query: query,
            limit: limit
        ))
        try require(response.type, expected: "notepad_notes")
        return response.notes
    }

    func createNote(
        ownerUserID: String,
        draft: NotepadNoteDraft
    ) async throws -> LocalNotepadNoteDetailRecord {
        let response: NoteResult = try await host.request(CreateNoteCommand(
            type: "create_notepad_note",
            ownerUserID: ownerUserID,
            folder: draft.folder,
            title: draft.title,
            content: draft.content,
            tags: draft.tags
        ))
        try require(response.type, expected: "notepad_note")
        return response.detail
    }

    func getNote(ownerUserID: String, noteID: String) async throws -> LocalNotepadNoteDetailRecord {
        let response: NoteResult = try await host.request(NoteIdentityCommand(
            type: "get_notepad_note",
            ownerUserID: ownerUserID,
            noteID: noteID
        ))
        try require(response.type, expected: "notepad_note")
        return response.detail
    }

    func updateNote(
        ownerUserID: String,
        noteID: String,
        expectedVersion: UInt64,
        update: NotepadNoteUpdate
    ) async throws -> LocalNotepadNoteDetailRecord {
        let response: NoteResult = try await host.request(UpdateNoteCommand(
            type: "update_notepad_note",
            ownerUserID: ownerUserID,
            noteID: noteID,
            expectedVersion: expectedVersion,
            title: update.title,
            content: update.content,
            folder: update.folder,
            tags: update.tags
        ))
        try require(response.type, expected: "notepad_note")
        return response.detail
    }

    func deleteNote(
        ownerUserID: String,
        noteID: String,
        expectedVersion: UInt64
    ) async throws {
        let response: NoteDeletedResult = try await host.request(DeleteNoteCommand(
            type: "delete_notepad_note",
            ownerUserID: ownerUserID,
            noteID: noteID,
            expectedVersion: expectedVersion
        ))
        try require(response.type, expected: "notepad_note_deleted")
    }

    func putImage(
        ownerUserID: String,
        noteID: String,
        image: NotepadImageUpload
    ) async throws -> LocalNotepadImageRecord {
        let response: ImageResult = try await host.request(PutImageCommand(
            type: "put_notepad_image",
            ownerUserID: ownerUserID,
            noteID: noteID,
            name: image.name,
            mimeType: image.mimeType,
            dataBase64: image.data.base64EncodedString()
        ))
        try require(response.type, expected: "notepad_image")
        return response.image
    }

    private func require(_ actual: String, expected: String) throws {
        guard actual == expected else { throw NativeLocalAgentHostError.invalidResponse }
    }
}

private struct OwnerCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    enum CodingKeys: String, CodingKey { case type; case ownerUserID = "owner_user_id" }
}

private struct FolderCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let folder: String
    enum CodingKeys: String, CodingKey { case type, folder; case ownerUserID = "owner_user_id" }
}

private struct RenameFolderCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let from: String
    let to: String
    enum CodingKeys: String, CodingKey { case type, from, to; case ownerUserID = "owner_user_id" }
}

private struct DeleteFolderCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let folder: String
    let recursive: Bool
    enum CodingKeys: String, CodingKey { case type, folder, recursive; case ownerUserID = "owner_user_id" }
}

private struct ListNotesCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let query: String?
    let limit: UInt32
    enum CodingKeys: String, CodingKey { case type, query, limit; case ownerUserID = "owner_user_id" }
}

private struct CreateNoteCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let folder: String
    let title: String
    let content: String
    let tags: [String]
    enum CodingKeys: String, CodingKey {
        case type, folder, title, content, tags
        case ownerUserID = "owner_user_id"
    }
}

private struct NoteIdentityCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let noteID: String
    enum CodingKeys: String, CodingKey { case type; case ownerUserID = "owner_user_id"; case noteID = "note_id" }
}

private struct UpdateNoteCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let noteID: String
    let expectedVersion: UInt64
    let title: String?
    let content: String?
    let folder: String?
    let tags: [String]?
    enum CodingKeys: String, CodingKey {
        case type, title, content, folder, tags
        case ownerUserID = "owner_user_id"
        case noteID = "note_id"
        case expectedVersion = "expected_version"
    }
}

private struct DeleteNoteCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let noteID: String
    let expectedVersion: UInt64
    enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case noteID = "note_id"
        case expectedVersion = "expected_version"
    }
}

private struct PutImageCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let noteID: String
    let name: String
    let mimeType: String
    let dataBase64: String
    enum CodingKeys: String, CodingKey {
        case type, name
        case ownerUserID = "owner_user_id"
        case noteID = "note_id"
        case mimeType = "mime_type"
        case dataBase64 = "data_base64"
    }
}

private struct InitializedResult: Decodable, Sendable {
    let type: String
    let noteCount: UInt64
    enum CodingKeys: String, CodingKey { case type; case noteCount = "note_count" }
}

private struct FoldersResult: Decodable, Sendable { let type: String; let folders: [String] }
private struct FolderMutationResult: Decodable, Sendable { let type: String }
private struct NotesResult: Decodable, Sendable { let type: String; let notes: [LocalNotepadNoteRecord] }
private struct NoteResult: Decodable, Sendable { let type: String; let detail: LocalNotepadNoteDetailRecord }
private struct NoteDeletedResult: Decodable, Sendable { let type: String }
private struct ImageResult: Decodable, Sendable { let type: String; let image: LocalNotepadImageRecord }
