import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

final class ConversationGuidanceTimelineTests: XCTestCase {
    func testGuidanceGetsIndependentStableBubbleBetweenReplies() {
        let turn = sampleTurn()
        let items = build(turn)
        XCTAssertEqual(items.map(\.id), [
            "turn-1", "turn-turn-1-reply-reply-1", "turn-turn-1-user-guidance-1",
            "turn-turn-1-user-guidance-2", "turn-turn-1-reply-reply-2"
        ])
        XCTAssertEqual(Set(items.map(\.id)).count, items.count)
        XCTAssertEqual(build(turn).map(\.id), items.map(\.id))
        guard case let .additionalUser(_, message) = items[2] else {
            return XCTFail("Expected separate guidance bubble")
        }
        XCTAssertEqual(message.text, "追加要求")
    }

    func testGuidanceShowsBeforeAnyReplyAndKeepsAttachmentOnlyMessage() {
        var turn = sampleTurn()
        turn.assistantReplies = []
        let items = build(turn)
        XCTAssertEqual(items.count, 3)
        guard case let .additionalUser(_, message) = items[2] else {
            return XCTFail("Expected attachment-only guidance bubble")
        }
        XCTAssertEqual(message.text, "")
        XCTAssertEqual(message.attachments.first?.id, "attachment-1")
    }

    func testGuidanceAfterFinalReplyIsNotDropped() {
        var turn = sampleTurn()
        turn.assistantReplies = Array(turn.assistantReplies.prefix(1))
        XCTAssertEqual(build(turn).last?.id, "turn-turn-1-user-guidance-2")
    }

    func testStorageOrdinalsDisambiguateSameTimestampWrites() {
        var turn = sampleTurn()
        for index in turn.additionalUserMessages.indices {
            turn.additionalUserMessages[index].storageOrdinal = UInt64(index + 3)
            turn.additionalUserMessages[index].createdAt = .distantPast
        }
        for index in turn.assistantReplies.indices {
            turn.assistantReplies[index].message.storageOrdinal = index == 0 ? 2 : 5
            turn.assistantReplies[index].message.createdAt = .distantPast
        }
        XCTAssertEqual(build(turn).map(\.id), [
            "turn-1", "turn-turn-1-reply-reply-1", "turn-turn-1-user-guidance-1",
            "turn-turn-1-user-guidance-2", "turn-turn-1-reply-reply-2"
        ])
    }

    private func build(_ turn: ConversationTurn) -> [ConversationTimelineItem] {
        ConversationTimelineItem.build(turns: [turn], promptsByTurnID: [:], unattachedPrompts: [])
    }

    private func sampleTurn() -> ConversationTurn {
        func message(_ id: String, _ time: Double, _ role: ChatMessage.Role, _ text: String) -> ChatMessage {
            .init(id: id, role: role, text: text, createdAt: Date(timeIntervalSince1970: time))
        }
        var attachmentOnly = message("guidance-2", 3, .user, "")
        attachmentOnly.attachments = [
            .init(id: "attachment-1", name: "brief.txt", mimeType: "text/plain", size: 1, kind: .file)
        ]
        return ConversationTurn(
            id: "turn-1", sessionID: "session-1", sequence: 1, revision: 2,
            userMessage: message("user-1", 1, .user, "原要求"),
            additionalUserMessages: [message("guidance-1", 3, .user, "追加要求"), attachmentOnly],
            assistantReplies: [
                .init(message: message("reply-1", 2, .assistant, "第一条回复")),
                .init(message: message("reply-2", 4, .assistant, "追加后回复"))
            ],
            status: .streaming, startedAt: Date(timeIntervalSince1970: 1)
        )
    }
}
