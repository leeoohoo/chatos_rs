@testable import ChatOSConnector
import ChatOSCore
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
        let conversations = NativeLocalAgentConversationClient(host: lifecycle)
        let created = try await conversations.create(
            ownerUserID: "user-1",
            conversationID: "conversation-1",
            title: "Local conversation"
        )
        XCTAssertEqual(created.conversation.conversationID, "conversation-1")
        XCTAssertEqual(created.conversation.version, 1)
        let page = try await conversations.list(ownerUserID: "user-1")
        XCTAssertEqual(page.conversations.map(\.conversationID), ["conversation-1"])
        let history = try await conversations.history(
            ownerUserID: "user-1",
            conversationID: "conversation-1"
        )
        XCTAssertEqual(history.conversation, created.conversation)
        XCTAssertTrue(history.messages.isEmpty)
        let controlPlane = NativeLocalAgentControlPlaneClient(host: lifecycle)
        let model = LocalAgentModelConfigSnapshot(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            modelConfigRevision: "model-revision-1",
            credentialRef: "env:CHATOS_LOCAL_AGENT_MODEL_MODEL_1",
            baseURL: "https://example.invalid/v1",
            model: "model-1",
            provider: "openai",
            supportsResponses: true,
            thinkingLevel: "medium"
        )
        let publishedModel = try await controlPlane.publishModel(model)
        XCTAssertEqual(publishedModel, model)
        let loadedModel = try await controlPlane.model(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            modelConfigRevision: "model-revision-1"
        )
        XCTAssertEqual(loadedModel, model)
        let capabilities = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: "user-1",
            profileKey: "main_chat",
            capabilityPolicyRevision: "capability-revision-1",
            instructions: "Answer locally."
        )
        let publishedCapabilities = try await controlPlane.publishCapabilities(capabilities)
        XCTAssertEqual(publishedCapabilities, capabilities)
        let loadedCapabilities = try await controlPlane.capabilities(
            ownerUserID: "user-1",
            profileKey: "main_chat",
            capabilityPolicyRevision: "capability-revision-1"
        )
        XCTAssertEqual(loadedCapabilities, capabilities)
        try await lifecycle.restart(
            ownerUserID: "user-1",
            credentialEnvironment: ["CHATOS_LOCAL_AGENT_MODEL_MODEL_1": "test-only-secret"]
        )
        owner = await lifecycle.activeOwnerUserID
        XCTAssertEqual(owner, "user-1")
        let persistedModel = try await controlPlane.model(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            modelConfigRevision: "model-revision-1"
        )
        XCTAssertEqual(persistedModel, model)
        let runtimeSettings = NativeLocalAgentConversationRuntimeSettingsService(host: lifecycle)
        let conversationService = NativeLocalAgentConversationService(
            host: lifecycle,
            attachmentRootURL: root.appendingPathComponent("attachments"),
            runtimeSettings: runtimeSettings
        )
        let modelOption = ConversationModelOption(
            id: "model-1",
            displayName: "Test model",
            modelName: model.model,
            provider: model.provider,
            thinkingLevel: model.thinkingLevel,
            supportsReasoning: true,
            thinkingLevels: ["none", "medium", "high"]
        )
        let bootstrap = NativeLocalAgentBootstrapResult(
            modelSnapshots: [model],
            modelOptions: [modelOption],
            capabilitySnapshot: capabilities
        )
        try await runtimeSettings.configure(ownerUserID: "user-1", bootstrap: bootstrap)
        try await conversationService.configure(
            ownerUserID: "user-1",
            bootstrap: bootstrap
        )
        let acknowledgement = try await conversationService.sendNewTurn(.init(
            sessionID: "conversation-1",
            turnID: "turn-1",
            content: "hello locally"
        ))
        XCTAssertTrue(acknowledgement.accepted)
        let runPage = try await NativeLocalAgentRuntimeClient(host: lifecycle).listRuns(
            ownerUserID: "user-1",
            scope: "all"
        )
        let turnRun = try XCTUnwrap(runPage.runs.first(where: {
            $0.ownerEntityID == "turn-1"
        }))
        guard case let .object(runInput) = turnRun.input,
              case let .object(runtimeInput)? = runInput["runtime_settings"] else {
            return XCTFail("Turn Run must freeze local runtime settings")
        }
        XCTAssertEqual(runtimeInput["selected_thinking_level"], .string("medium"))
        XCTAssertEqual(runtimeInput["reasoning_enabled"], .bool(true))
        let localHistory = try await conversationService.fetchHistory(.init(
            sessionID: "conversation-1",
            requestGeneration: 1
        ))
        XCTAssertEqual(localHistory.turns.first?.id, "turn-1")
        XCTAssertEqual(localHistory.turns.first?.userMessage.text, "hello locally")
        let eventStream = await conversationService.events(sessionID: "conversation-1")
        var eventIterator = eventStream.makeAsyncIterator()
        let event = try await eventIterator.next()
        XCTAssertEqual(event?.sessionID, "conversation-1")
        XCTAssertEqual(event?.kind, .reconcile)
        XCTAssertEqual(event?.eventName, "local_conversation_changed")
        XCTAssertGreaterThan(event?.eventSequence ?? 0, 0)
        await XCTAssertThrowsErrorAsync {
            _ = try await conversations.create(
                ownerUserID: "another-user",
                conversationID: "conversation-2",
                title: "Wrong owner"
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

    func testLaunchArgumentsAndEnvironmentKeepMemoryTokenOffCommandLine() throws {
        let database = URL(fileURLWithPath: "/tmp/chatos-local-agent-test.sqlite3")
        let configuration = NativeLocalAgentHostConfiguration(
            executableURL: URL(fileURLWithPath: "/tmp/chatos_local_agent_host"),
            databaseURL: database,
            readOnlyToolNames: ["read_b", "read_a"],
            approvalExemptToolNames: ["approve_b", "approve_a"],
            memoryBaseURL: URL(string: "https://gateway.example/api/memory")!,
            memorySourceID: "local_agent",
            memoryTimeoutMilliseconds: 12_345
        )
        let arguments = ManagedLocalAgentHostProcess.arguments(
            configuration: configuration,
            database: database,
            ownerUserID: "user-1"
        )
        XCTAssertTrue(arguments.contains("https://gateway.example/api/memory"))
        XCTAssertTrue(arguments.contains("local_agent"))
        XCTAssertTrue(arguments.contains("12345"))
        XCTAssertFalse(arguments.contains("memory-secret"))
        XCTAssertEqual(arguments.last, "--stdio")

        let environment = ManagedLocalAgentHostProcess.safeEnvironment(
            credentialEnvironment: [
                "CHATOS_LOCAL_AGENT_MODEL_MODEL_1": "model-secret",
                "CHATOS_MEMORY_ACCESS_TOKEN": "memory-secret",
            ]
        )
        XCTAssertEqual(environment["CHATOS_MEMORY_ACCESS_TOKEN"], "memory-secret")
        XCTAssertEqual(environment["CHATOS_LOCAL_AGENT_MODEL_MODEL_1"], "model-secret")
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
