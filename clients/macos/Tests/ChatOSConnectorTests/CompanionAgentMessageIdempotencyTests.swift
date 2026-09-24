@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class CompanionAgentMessageIdempotencyTests: XCTestCase {
    func testRepeatedCompanionMessageReturnsOriginalWithoutDuplicateWrite() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("companion-agent-idempotency-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = try SQLiteAgentGroupChatStore(
            databaseURL: directory.appendingPathComponent("group-chat.db")
        )
        let room = try await store.createRoom(
            ownerUserID: "owner-1",
            projectID: "project-1",
            draft: .init(name: "测试团队")
        )
        let draft = ProjectAgentMessageDraft(
            senderKind: .human,
            senderID: "owner-1",
            content: "同一条 Companion 消息",
            causationID: "companion:client-message-1"
        )

        let first = try await store.postMessageIdempotently(
            ownerUserID: "owner-1",
            roomID: room.id,
            draft: draft
        )
        let replay = try await store.postMessageIdempotently(
            ownerUserID: "owner-1",
            roomID: room.id,
            draft: draft
        )
        let messages = try await store.listMessages(
            ownerUserID: "owner-1",
            roomID: room.id
        )

        XCTAssertFalse(first.deduplicated)
        XCTAssertTrue(replay.deduplicated)
        XCTAssertEqual(replay.message.id, first.message.id)
        XCTAssertEqual(messages.map(\.id), [first.message.id])
    }
}
