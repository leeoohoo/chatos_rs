@testable import ChatOSConnector
import Foundation
import XCTest

final class NativeLocalAgentHostLifecycleTests: XCTestCase {
    func testStartsHealthChecksAndSwitchesActualRustHost() async throws {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let configured = ProcessInfo.processInfo.environment["CHATOS_LOCAL_AGENT_HOST_PATH"]
            .map { URL(fileURLWithPath: $0) }
        let executable = configured ?? repository
            .appendingPathComponent("target-shared/debug/chatos_local_agent_host")
        guard FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw XCTSkip("Build chatos_local_agent_host before running the native lifecycle test")
        }
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-local-agent-native-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let lifecycle = NativeLocalAgentHostLifecycle(configuration: .init(
            executableURL: executable,
            databaseURL: root.appendingPathComponent("local-agent.sqlite3"),
            startupTimeout: .seconds(10)
        ))

        try await lifecycle.start(ownerUserID: "user-1")
        var owner = await lifecycle.activeOwnerUserID
        XCTAssertEqual(owner, "user-1")
        let result: TestConversationResult = try await lifecycle.request(
            TestCreateConversationCommand(
                type: "create_conversation",
                conversationID: "conversation-1",
                ownerUserID: "user-1",
                title: "Local conversation"
            )
        )
        XCTAssertEqual(result.type, "conversation")
        XCTAssertEqual(result.conversation.conversation.conversationId, "conversation-1")
        XCTAssertEqual(result.conversation.conversation.version, 1)
        await XCTAssertThrowsErrorAsync {
            let _: TestConversationResult = try await lifecycle.request(
                TestCreateConversationCommand(
                    type: "create_conversation",
                    conversationID: "conversation-2",
                    ownerUserID: "another-user",
                    title: "Wrong owner"
                )
            )
        }
        try await lifecycle.start(ownerUserID: "user-2")
        owner = await lifecycle.activeOwnerUserID
        XCTAssertEqual(owner, "user-2")
        await lifecycle.stop()
        owner = await lifecycle.activeOwnerUserID
        XCTAssertNil(owner)
    }

    func testFrameCodecUsesBigEndianLengthPrefix() throws {
        let pipe = Pipe()
        let payload = Data(#"{"command":{"type":"health"}}"#.utf8)
        try LocalAgentHostFrameCodec.write(payload, to: pipe.fileHandleForWriting)
        try pipe.fileHandleForWriting.close()

        let encoded = pipe.fileHandleForReading.readDataToEndOfFile()
        XCTAssertEqual(encoded.prefix(4), Data([0, 0, 0, UInt8(payload.count)]))
        let replay = Pipe()
        try replay.fileHandleForWriting.write(contentsOf: encoded)
        try replay.fileHandleForWriting.close()
        XCTAssertEqual(try LocalAgentHostFrameCodec.read(from: replay.fileHandleForReading), payload)
    }

    func testFrameCodecRejectsEmptyOversizedAndTruncatedFrames() throws {
        let sink = Pipe()
        XCTAssertThrowsError(try LocalAgentHostFrameCodec.write(Data(), to: sink.fileHandleForWriting))
        XCTAssertThrowsError(try LocalAgentHostFrameCodec.write(
            Data(count: LocalAgentHostFrameCodec.maximumFrameBytes + 1),
            to: sink.fileHandleForWriting
        ))

        let truncated = Pipe()
        try truncated.fileHandleForWriting.write(contentsOf: Data([0, 0, 0, 4, 1, 2]))
        try truncated.fileHandleForWriting.close()
        XCTAssertThrowsError(try LocalAgentHostFrameCodec.read(from: truncated.fileHandleForReading))
    }
}

private struct TestCreateConversationCommand: Encodable, Sendable {
    let type: String
    let conversationID: String
    let ownerUserID: String
    let title: String
}

private struct TestConversationResult: Decodable, Sendable {
    let type: String
    let conversation: TestConversationDetail
}

private struct TestConversationDetail: Decodable, Sendable {
    let conversation: TestConversationRecord
}

private struct TestConversationRecord: Decodable, Sendable {
    let conversationId: String
    let version: UInt64
}

private func XCTAssertThrowsErrorAsync(
    _ expression: () async throws -> Void,
    file: StaticString = #filePath,
    line: UInt = #line
) async {
    do {
        try await expression()
        XCTFail("Expected expression to throw", file: file, line: line)
    } catch {
    }
}
