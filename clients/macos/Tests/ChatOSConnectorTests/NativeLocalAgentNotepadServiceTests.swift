@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentNotepadServiceTests: XCTestCase {
    func testUsesOwnerScopedCommandsAndCarriesOptimisticVersion() async throws {
        let host = NotepadHostStub()
        let service = NativeLocalAgentNotepadService(host: host)
        await service.configure(ownerUserID: "user-1")

        try await service.initialize()
        try await service.createFolder("work/ideas")
        let created = try await service.createNote(.init(
            folder: "work/ideas",
            title: "Local",
            content: "body",
            tags: ["rust"]
        ))
        XCTAssertEqual(created.note.id, "note-1")
        XCTAssertEqual(created.content, "body")

        let updated = try await service.updateNote(
            id: "note-1",
            update: .init(title: "Updated")
        )
        XCTAssertEqual(updated.note.title, "Updated")
        var command = try await host.lastCommand()
        XCTAssertEqual(command.type, "update_notepad_note")
        XCTAssertEqual(command.ownerUserID, "user-1")
        XCTAssertEqual(command.expectedVersion, 1)

        try await service.deleteNote(id: "note-1")
        command = try await host.lastCommand()
        XCTAssertEqual(command.type, "delete_notepad_note")
        XCTAssertEqual(command.expectedVersion, 2)
    }

    func testResetRejectsAccessAndImageReturnsLocalDataURL() async throws {
        let host = NotepadHostStub()
        let service = NativeLocalAgentNotepadService(host: host)
        await service.configure(ownerUserID: "user-1")
        let image = try await service.uploadImage(
            .init(data: Data([0x89, 0x50]), mimeType: "image/png", name: "paste.png"),
            noteID: "note-1"
        )
        XCTAssertEqual(image.url.scheme, "data")
        XCTAssertEqual(image.name, "paste.png")

        await service.reset()
        do {
            _ = try await service.listFolders()
            XCTFail("reset service should reject access")
        } catch let error as NativeLocalAgentNotepadError {
            XCTAssertEqual(error, .notConfigured)
        }
    }
}

private actor NotepadHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []
    private var version: UInt64 = 1

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "initialize_notepad":
            return try json(["type": "notepad_initialized", "note_count": 0])
        case "create_notepad_folder":
            return try json([
                "type": "notepad_folder_mutation",
                "folder": "work/ideas",
                "affected_notes": 0,
            ])
        case "create_notepad_note":
            version = 1
            return try noteResult(title: "Local", content: "body")
        case "update_notepad_note":
            version += 1
            return try noteResult(title: "Updated", content: "body")
        case "delete_notepad_note":
            return try json(["type": "notepad_note_deleted", "note_id": "note-1"])
        case "put_notepad_image":
            return try json([
                "type": "notepad_image",
                "image": [
                    "image_id": "image-1",
                    "note_id": "note-1",
                    "owner_user_id": "user-1",
                    "name": "paste.png",
                    "mime_type": "image/png",
                    "size": 2,
                    "sha256": "abc",
                    "data_url": "data:image/png;base64,iVA=",
                    "created_at_unix_ms": 1,
                ],
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> NotepadCommandSnapshot {
        guard let command = commands.last else { throw CocoaError(.fileNoSuchFile) }
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: command) as? [String: Any])
        return NotepadCommandSnapshot(
            type: object["type"] as? String,
            ownerUserID: object["owner_user_id"] as? String,
            expectedVersion: (object["expected_version"] as? NSNumber)?.uint64Value
        )
    }

    private func noteResult(title: String, content: String) throws -> Data {
        try json([
            "type": "notepad_note",
            "detail": [
                "note": [
                    "note_id": "note-1",
                    "owner_user_id": "user-1",
                    "title": title,
                    "folder": "work/ideas",
                    "tags": ["rust"],
                    "file": "note-1.md",
                    "version": version,
                    "created_at_unix_ms": 1,
                    "updated_at_unix_ms": 2,
                ],
                "content": content,
            ],
        ])
    }

    private func json(_ value: [String: Any]) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}

private struct NotepadCommandSnapshot: Sendable {
    let type: String?
    let ownerUserID: String?
    let expectedVersion: UInt64?
}
