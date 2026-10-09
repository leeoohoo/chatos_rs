import ChatOSCore
import Foundation
import XCTest

final class ConversationGuidanceMessageTests: XCTestCase {
    func testGuidanceSurvivesCodableRoundTripAndOriginalLookupRemainsStable() throws {
        let turn = sampleTurn()
        let decoded = try JSONDecoder().decode(ConversationTurn.self, from: JSONEncoder().encode(turn))
        XCTAssertEqual(decoded, turn)
        XCTAssertEqual(decoded.additionalUserMessages.map(\.id), ["guidance-1"])
        XCTAssertEqual(decoded.resolvedMessageTaskLookup.sourceUserMessageID, "user-1")
    }

    func testOldHistoryWithoutGuidanceKeyStillDecodes() throws {
        var object = try XCTUnwrap(JSONSerialization.jsonObject(
            with: JSONEncoder().encode(sampleTurn())
        ) as? [String: Any])
        object.removeValue(forKey: "additionalUserMessages")
        let decoded = try JSONDecoder().decode(
            ConversationTurn.self, from: JSONSerialization.data(withJSONObject: object)
        )
        XCTAssertTrue(decoded.additionalUserMessages.isEmpty)
        XCTAssertEqual(decoded.userMessage.id, "user-1")
    }

    private func sampleTurn() -> ConversationTurn {
        ConversationTurn(
            id: "turn-1", sessionID: "session-1", sequence: 1, revision: 2,
            userMessage: .init(id: "user-1", role: .user, text: "原要求", createdAt: .distantPast),
            additionalUserMessages: [
                .init(id: "guidance-1", role: .user, text: "补充要求", createdAt: .distantFuture)
            ],
            status: .streaming, startedAt: .distantPast
        )
    }
}
