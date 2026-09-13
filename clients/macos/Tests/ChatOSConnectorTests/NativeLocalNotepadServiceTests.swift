// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native local Notepad service")
struct NativeLocalNotepadServiceTests {
    @Test("maps records and sends exact revision-safe folder and note mutations")
    func mapsAndMutatesLocalRecords() async throws {
        let transport = NotepadServiceTransport()
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)
        let service = NativeLocalNotepadService(
            accountSession: NotepadAccountSession(client: client)
        )

        #expect(try await service.listFolders() == ["design", "design/research"])
        let notes = try await service.listNotes(query: "cinematic", limit: 20)
        #expect(notes.map(\.id) == ["note:visual-direction"])
        #expect(notes.first?.title == "Visual direction")
        #expect(notes.first?.createdAt != nil)

        let updated = try await service.updateNote(
            id: "note:visual-direction",
            update: NotepadNoteUpdate(title: "Approved direction", tags: ["approved"])
        )
        #expect(updated.note.title == "Approved direction")
        #expect(updated.note.folder == "design/research")
        #expect(updated.content == "Use a cinematic layout.")

        try await service.deleteNote(id: "note:visual-direction")
        try await service.createFolder("design/final")
        try await service.renameFolder(from: "design/final", to: "visual/final")
        try await service.deleteFolder("visual/final", recursive: true)

        let commands = try await transport.requests().map(Self.command)
        #expect(commands.map { $0["type"] as? String } == [
            "list_notepad",
            "list_notepad",
            "get_notepad",
            "put_notepad",
            "get_notepad",
            "delete_notepad",
            "put_notepad",
            "rename_notepad_folder",
            "delete_notepad_folder",
        ])

        let updatePayload = try #require(commands[3]["payload"] as? [String: Any])
        #expect(updatePayload["record_id"] as? String == "note:visual-direction")
        #expect(updatePayload["expected_revision"] as? UInt64 == 4)
        let updateDraft = try #require(updatePayload["draft"] as? [String: Any])
        #expect(updateDraft["title"] as? String == "Approved direction")
        #expect(updateDraft["folder"] as? String == "design/research")
        #expect(updateDraft["content"] as? String == "Use a cinematic layout.")
        #expect(updateDraft["tags"] as? [String] == ["approved"])

        let deletePayload = try #require(commands[5]["payload"] as? [String: Any])
        #expect(deletePayload["expected_revision"] as? UInt64 == 4)
        let folderDraft = try #require(
            (commands[6]["payload"] as? [String: Any])?["draft"] as? [String: Any]
        )
        #expect(folderDraft["kind"] as? String == "folder")
        #expect(folderDraft["folder"] as? String == "design/final")
        let renamePayload = try #require(commands[7]["payload"] as? [String: Any])
        #expect(renamePayload["folder"] as? String == "design/final")
        #expect(renamePayload["replacement"] as? String == "visual/final")
        let folderDeletePayload = try #require(commands[8]["payload"] as? [String: Any])
        #expect(folderDeletePayload["folder"] as? String == "visual/final")
        #expect(folderDeletePayload["recursive"] as? Bool == true)
    }

    @Test("does not access Notepad state without an active account")
    func requiresActiveAccountScope() async {
        let service = NativeLocalNotepadService(accountSession: MissingNotepadAccountSession())

        await #expect(throws: NotepadServiceTestError.self) {
            _ = try await service.listNotes(query: nil, limit: 20)
        }
    }

    private static func command(_ request: Data) throws -> [String: Any] {
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        return try #require(object["command"] as? [String: Any])
    }
}

private struct NotepadAccountSession: NativeLocalAgentAccountSessionAccess {
    let client: NativeLocalAgentIPCClient

    func client(accountID: String) async throws -> NativeLocalAgentIPCClient { client }
    func activeClient() async throws -> NativeLocalAgentIPCClient { client }
    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] { [] }
    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {}
}

private struct MissingNotepadAccountSession: NativeLocalAgentAccountSessionAccess {
    func client(accountID: String) async throws -> NativeLocalAgentIPCClient {
        throw NotepadServiceTestError.noActiveAccount
    }

    func activeClient() async throws -> NativeLocalAgentIPCClient {
        throw NotepadServiceTestError.noActiveAccount
    }

    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] {
        throw NotepadServiceTestError.noActiveAccount
    }

    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {}
}

private actor NotepadServiceTransport: LocalAgentFrameTransport {
    private var recordedRequests: [Data] = []

    func exchange(_ request: Data) async throws -> Data {
        recordedRequests.append(request)
        let object = try Self.object(request)
        let command = try Self.dictionary(object["command"])
        let type = try Self.string(command["type"])
        let payload = try Self.dictionary(command["payload"])
        let owner = try Self.string(object["owner_user_id"])
        let response: [String: Any]

        switch type {
        case "list_notepad":
            response = [
                "type": "notepad_records",
                "payload": [
                    "records": [Self.note(owner: owner, revision: 4)],
                    "next_cursor": NSNull(),
                ],
            ]
        case "get_notepad":
            response = ["type": "notepad", "payload": Self.note(owner: owner, revision: 4)]
        case "put_notepad":
            let draft = try Self.dictionary(payload["draft"])
            let expectedRevision = payload["expected_revision"] as? UInt64
            response = [
                "type": "notepad",
                "payload": Self.record(
                    id: try Self.string(payload["record_id"]),
                    owner: owner,
                    draft: draft,
                    revision: (expectedRevision ?? 0) + 1
                ),
            ]
        case "delete_notepad", "rename_notepad_folder", "delete_notepad_folder":
            response = ["type": "success"]
        default:
            throw NotepadServiceTestError.unexpectedCommand(type)
        }

        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": try Self.string(object["request_id"]),
            "response": response,
        ])
    }

    func requests() -> [Data] { recordedRequests }

    private static func note(owner: String, revision: UInt64) -> [String: Any] {
        record(
            id: "note:visual-direction",
            owner: owner,
            draft: [
                "kind": "note",
                "folder": "design/research",
                "title": "Visual direction",
                "content": "Use a cinematic layout.",
                "tags": ["design", "reference"],
            ],
            revision: revision
        )
    }

    private static func record(
        id: String,
        owner: String,
        draft: [String: Any],
        revision: UInt64
    ) -> [String: Any] {
        [
            "record_id": id,
            "owner_user_id": owner,
            "draft": draft,
            "revision": revision,
            "created_at": "2026-09-14T00:00:00Z",
            "updated_at": "2026-09-14T00:01:00Z",
        ]
    }

    private static func object(_ data: Data) throws -> [String: Any] {
        try dictionary(JSONSerialization.jsonObject(with: data))
    }

    private static func dictionary(_ value: Any?) throws -> [String: Any] {
        guard let value = value as? [String: Any] else {
            throw NotepadServiceTestError.invalidRequest
        }
        return value
    }

    private static func string(_ value: Any?) throws -> String {
        guard let value = value as? String else {
            throw NotepadServiceTestError.invalidRequest
        }
        return value
    }
}

private enum NotepadServiceTestError: Error {
    case invalidRequest
    case noActiveAccount
    case unexpectedCommand(String)
}
