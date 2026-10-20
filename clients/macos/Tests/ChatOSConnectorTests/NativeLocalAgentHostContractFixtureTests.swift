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

        let failure = try JSONDecoder.localAgent.decode(
            NativeLocalAgentHealthResponse.self,
            from: fixture("health-failure.json")
        )
        XCTAssertFalse(failure.ok)
        XCTAssertNil(failure.result)
        XCTAssertEqual(failure.error?.code, "storage_unavailable")
        XCTAssertEqual(failure.error?.retryable, true)
    }

    private func fixture(_ name: String) throws -> Data {
        let clients = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return try Data(contentsOf: clients
            .appendingPathComponent("contracts/local-agent/v44/fixtures")
            .appendingPathComponent(name))
    }

    private func normalized(_ data: Data) throws -> Data {
        try JSONSerialization.data(
            withJSONObject: JSONSerialization.jsonObject(with: data),
            options: [.sortedKeys]
        )
    }
}
