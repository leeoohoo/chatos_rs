@testable import ChatOSConnector
import Foundation
import XCTest

final class NativeLocalAgentHostContractFixtureTests: XCTestCase {
    func testSharedHealthRequestMatchesSwiftWireDTO() throws {
        let data = try fixture("health-request.json")
        let request = try JSONDecoder.localAgent.decode(
            NativeLocalAgentHealthRequest.self,
            from: data
        )
        XCTAssertEqual(request.protocolVersion, NativeLocalAgentHostProtocol.version)
        XCTAssertEqual(request.commandId, "contract-health-1")
        XCTAssertEqual(request.command.type, "health")
        let encoded = try JSONEncoder.localAgent.encode(request)
        XCTAssertEqual(try normalized(encoded), try normalized(data))
    }

    func testSharedHealthResponsesMatchSwiftWireDTO() throws {
        let success = try JSONDecoder.localAgent.decode(
            NativeLocalAgentHealthResponse.self,
            from: fixture("health-success.json")
        )
        XCTAssertTrue(success.ok)
        XCTAssertNil(success.error)
        XCTAssertEqual(success.result?.type, "health")
        XCTAssertEqual(success.result?.service, "chatos-local-agent-host")
        XCTAssertEqual(success.result?.storageReady, true)
        XCTAssertEqual(success.result?.recoveredClaims, 0)
        XCTAssertEqual(success.result?.maxFrameBytes, LocalAgentHostFrameCodec.maximumFrameBytes)
        XCTAssertEqual(success.result?.maxMessageBytes, LocalAgentHostFrameCodec.maximumMessageBytes)
        XCTAssertEqual(success.result?.scheduler.maximumConcurrentWorkItems, 4)
        XCTAssertEqual(success.result?.scheduler.effectiveMaximumConcurrentWorkItems, 3)
        XCTAssertEqual(success.result?.scheduler.pressureLevel, .elevated)
        XCTAssertEqual(success.result?.scheduler.activeWorkItems, 2)
        XCTAssertEqual(success.result?.scheduler.idleWorkers, 2)
        XCTAssertEqual(success.result?.scheduler.queuedReadyWorkItems, 3)

        let failure = try JSONDecoder.localAgent.decode(
            NativeLocalAgentHealthResponse.self,
            from: fixture("health-failure.json")
        )
        XCTAssertFalse(failure.ok)
        XCTAssertNil(failure.result)
        XCTAssertEqual(failure.error?.code, "storage_unavailable")
        XCTAssertEqual(failure.error?.retryable, true)
    }

    func testSharedConversationRunFixturesMatchSwiftWireDTOs() throws {
        let historyEnvelope = try JSONDecoder().decode(
            ContractEnvelope<ConversationHistoryFixtureResult>.self,
            from: fixture("conversation-history-success.json")
        )
        let history = try XCTUnwrap(historyEnvelope.result?.page)
        XCTAssertEqual(historyEnvelope.protocolVersion, NativeLocalAgentHostProtocol.version)
        XCTAssertEqual(history.conversation.conversationID, "conversation-1")
        XCTAssertEqual(history.turns.first?.runID, "run-1")
        XCTAssertEqual(history.messages.first?.messageID, "message-user-1")
        XCTAssertEqual(
            history.attachments.first?.authorizedLocalRef,
            "local-attachment:authority-1"
        )
        XCTAssertNil(history.nextBeforeOrdinal)

        let graphEnvelope = try JSONDecoder().decode(
            ContractEnvelope<MessageTaskGraphFixtureResult>.self,
            from: fixture("message-task-graph-success.json")
        )
        let graph = try XCTUnwrap(graphEnvelope.result?.graph)
        XCTAssertEqual(graph.sourceConversationID, history.conversation.conversationID)
        XCTAssertEqual(graph.sourceTurnID, history.turns.first?.turnID)
        XCTAssertEqual(graph.sourceUserMessageID, history.messages.first?.messageID)
        XCTAssertEqual(graph.nodes.first?.task.activeRunID, "run-1")

        let eventsEnvelope = try JSONDecoder().decode(
            ContractEnvelope<RunEventsFixtureResult>.self,
            from: fixture("run-events-latest-success.json")
        )
        let eventsResult = try XCTUnwrap(eventsEnvelope.result)
        XCTAssertEqual(eventsResult.events.map(\.runID), ["run-1", "run-1", "run-1"])
        XCTAssertEqual(eventsResult.events.map(\.cursor), [103, 102, 101])
        XCTAssertEqual(eventsResult.nextCursor, eventsResult.events.last?.cursor)
    }

    private func fixture(_ name: String) throws -> Data {
        let clients = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return try Data(contentsOf: clients
            .appendingPathComponent("contracts/local-agent/v49/fixtures")
            .appendingPathComponent(name))
    }

    private func normalized(_ data: Data) throws -> Data {
        try JSONSerialization.data(
            withJSONObject: JSONSerialization.jsonObject(with: data),
            options: [.sortedKeys]
        )
    }
}

private struct ContractEnvelope<Result: Decodable>: Decodable {
    let protocolVersion: Int
    let result: Result?

    private enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol_version"
        case result
    }
}

private struct ConversationHistoryFixtureResult: Decodable {
    let type: String
    let page: LocalAgentConversationHistoryPage
}

private struct MessageTaskGraphFixtureResult: Decodable {
    let type: String
    let graph: LocalAgentMessageTaskGraph
}

private struct RunEventsFixtureResult: Decodable {
    let type: String
    let events: [LocalAgentEventRecord]
    let nextCursor: Int64

    private enum CodingKeys: String, CodingKey {
        case type, events
        case nextCursor = "next_cursor"
    }
}
