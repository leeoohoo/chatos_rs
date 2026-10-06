@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import XCTest

final class NativeLocalAgentHostLifecycleTests: XCTestCase {
    func testStandardErrorDrainStopsMonitoringAtEOF() async throws {
        let pipe = Pipe()
        let reader = pipe.fileHandleForReading
        let probe = NativeProcessPipeReaderProbe()
        NativeProcessPipeReader.install(
            on: reader,
            onData: { probe.append($0) },
            onEOF: { probe.reachEOF() }
        )

        try pipe.fileHandleForWriting.write(contentsOf: Data("diagnostic".utf8))
        try pipe.fileHandleForWriting.close()

        for _ in 0..<100 where reader.readabilityHandler != nil {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertNil(reader.readabilityHandler)
        XCTAssertEqual(probe.data, Data("diagnostic".utf8))
        XCTAssertEqual(probe.eofCount, 1)
        try? reader.close()
    }

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
            startupTimeout: .seconds(10),
            requestTimeoutMilliseconds: 500
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
        let latestModels = try await controlPlane.latestModels(ownerUserID: "user-1")
        XCTAssertEqual(latestModels, [model])
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
        let latestCapabilities = try await controlPlane.latestCapabilities(
            ownerUserID: "user-1",
            profileKey: "main_chat"
        )
        XCTAssertEqual(latestCapabilities, capabilities)
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
        let runtime = NativeLocalAgentRuntimeClient(host: lifecycle)
        let runPage = try await runtime.listRuns(
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
        let latestEventCursor = try await runtime.latestEventCursor(ownerUserID: "user-1")
        XCTAssertGreaterThan(latestEventCursor, 0)
        let waitingForEvents = Task {
            try await runtime.waitEvents(
                ownerUserID: "user-1",
                afterCursor: latestEventCursor,
                timeoutMilliseconds: 300,
                payloadMode: .routing
            )
        }
        try await Task.sleep(for: .milliseconds(50))
        let concurrentRequestStartedAt = ContinuousClock.now
        _ = try await runtime.listRuns(ownerUserID: "user-1", scope: "all")
        XCTAssertLessThan(
            concurrentRequestStartedAt.duration(to: .now),
            .milliseconds(200),
            "A long event wait must not block unrelated Host requests"
        )
        _ = try await waitingForEvents.value
        let localHistory = try await conversationService.fetchHistory(.init(
            sessionID: "conversation-1",
            requestGeneration: 1
        ))
        XCTAssertEqual(localHistory.turns.first?.id, "turn-1")
        XCTAssertEqual(localHistory.turns.first?.userMessage.text, "hello locally")
        let taskLookup = try XCTUnwrap(localHistory.turns.first?.messageTaskLookup)
        XCTAssertEqual(taskLookup.sessionID, "conversation-1")
        XCTAssertEqual(taskLookup.turnID, "turn-1")
        XCTAssertEqual(taskLookup.sourceUserMessageID, acknowledgement.userMessageID)
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
        let processIdentifier = await lifecycle.processIdentifier
        let exitedProcessID = try XCTUnwrap(processIdentifier)
        let exitNotification = expectation(description: "unexpected Host exit notification")
        let observer = NotificationCenter.default.addObserver(
            forName: .nativeLocalAgentHostDidExit,
            object: nil,
            queue: nil
        ) { notification in
            guard notification.userInfo?["process_identifier"] as? Int32 == exitedProcessID else {
                return
            }
            exitNotification.fulfill()
        }
        defer { NotificationCenter.default.removeObserver(observer) }
        XCTAssertEqual(kill(exitedProcessID, SIGTERM), 0)
        await fulfillment(of: [exitNotification], timeout: 2)
        let isRunningAfterExit = await lifecycle.isRunning
        let ownerAfterExit = await lifecycle.activeOwnerUserID
        XCTAssertFalse(isRunningAfterExit)
        XCTAssertNil(ownerAfterExit)

        try await lifecycle.start(ownerUserID: "user-2")
        owner = await lifecycle.activeOwnerUserID
        XCTAssertEqual(owner, "user-2")

        let timeoutProcessIdentifier = await lifecycle.processIdentifier
        let stalledProcessIdentifier = try XCTUnwrap(timeoutProcessIdentifier)
        let timeoutNotification = expectation(description: "stalled Host exit notification")
        let timeoutObserver = NotificationCenter.default.addObserver(
            forName: .nativeLocalAgentHostDidExit,
            object: nil,
            queue: nil
        ) { notification in
            guard notification.userInfo?["process_identifier"] as? Int32
                    == stalledProcessIdentifier else { return }
            timeoutNotification.fulfill()
        }
        defer { NotificationCenter.default.removeObserver(timeoutObserver) }
        XCTAssertEqual(kill(stalledProcessIdentifier, SIGSTOP), 0)
        do {
            _ = try await conversations.list(ownerUserID: "user-2")
            XCTFail("A stalled Host request must time out")
        } catch let error as NativeLocalAgentHostError {
            XCTAssertEqual(error, .requestTimedOut)
        }
        await fulfillment(of: [timeoutNotification], timeout: 2)
        XCTAssertEqual(kill(stalledProcessIdentifier, SIGCONT), 0)
        let isRunningAfterTimeout = await lifecycle.isRunning
        let ownerAfterTimeout = await lifecycle.activeOwnerUserID
        XCTAssertFalse(isRunningAfterTimeout)
        XCTAssertNil(ownerAfterTimeout)

        try await lifecycle.start(ownerUserID: "user-3")
        owner = await lifecycle.activeOwnerUserID
        XCTAssertEqual(owner, "user-3")

        let responsiveProcessIdentifierValue = await lifecycle.processIdentifier
        let responsiveProcessIdentifier = try XCTUnwrap(responsiveProcessIdentifierValue)
        XCTAssertEqual(kill(responsiveProcessIdentifier, SIGSTOP), 0)
        let stalledRequest = Task {
            try await conversations.list(ownerUserID: "user-3")
        }
        try await Task.sleep(for: .milliseconds(50))
        let clock = ContinuousClock()
        let stopStartedAt = clock.now
        await lifecycle.stop()
        let stopElapsed = stopStartedAt.duration(to: clock.now)
        XCTAssertLessThan(stopElapsed, .milliseconds(250))
        XCTAssertEqual(kill(responsiveProcessIdentifier, SIGCONT), 0)
        _ = await stalledRequest.result
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

    func testFrameCodecReadUsesDeadline() throws {
        let pipe = Pipe()
        defer {
            try? pipe.fileHandleForWriting.close()
            try? pipe.fileHandleForReading.close()
        }
        let deadline = LocalAgentHostFrameCodec.deadline(timeoutMilliseconds: 25)
        XCTAssertThrowsError(
            try LocalAgentHostFrameCodec.read(
                from: pipe.fileHandleForReading,
                deadline: deadline
            )
        ) { error in
            XCTAssertEqual(error as? NativeLocalAgentHostError, .requestTimedOut)
        }
    }

    func testFrameCodecInfiniteReadStopsWhenTransportIsCancelled() async throws {
        let pipe = Pipe()
        defer {
            try? pipe.fileHandleForWriting.close()
            try? pipe.fileHandleForReading.close()
        }
        let probe = LocalAgentHostReadCancellationProbe()
        let completion = expectation(description: "infinite response read is cancelled")
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                _ = try LocalAgentHostFrameCodec.read(
                    from: pipe.fileHandleForReading,
                    deadline: UInt64.max,
                    shouldCancel: { probe.shouldCancel }
                )
                probe.finish(error: nil)
            } catch {
                probe.finish(error: error)
            }
            completion.fulfill()
        }

        try await Task.sleep(for: .milliseconds(50))
        probe.cancel()
        await fulfillment(of: [completion], timeout: 1)
        XCTAssertTrue(probe.error is CancellationError)
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

private final class NativeProcessPipeReaderProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var received = Data()
    private var reachedEOFCount = 0

    func append(_ data: Data) {
        lock.withLock { received.append(data) }
    }

    func reachEOF() {
        lock.withLock { reachedEOFCount += 1 }
    }

    var data: Data { lock.withLock { received } }
    var eofCount: Int { lock.withLock { reachedEOFCount } }
}

private final class LocalAgentHostReadCancellationProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false
    private var finishedError: Error?

    var shouldCancel: Bool { lock.withLock { cancelled } }
    var error: Error? { lock.withLock { finishedError } }

    func cancel() {
        lock.withLock { cancelled = true }
    }

    func finish(error: Error?) {
        lock.withLock { finishedError = error }
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
