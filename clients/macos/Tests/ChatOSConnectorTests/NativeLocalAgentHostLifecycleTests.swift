@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import SQLite3
import XCTest

final class NativeLocalAgentHostLifecycleTests: XCTestCase {
    func testNotRunningRequestSignalsRecoveryBeforeReturningTheError() async throws {
        let recoveryRequested = expectation(description: "Host recovery requested")
        let observer = NotificationCenter.default.addObserver(
            forName: .nativeLocalAgentHostRecoveryRequested,
            object: nil,
            queue: nil
        ) { _ in
            recoveryRequested.fulfill()
        }
        defer { NotificationCenter.default.removeObserver(observer) }

        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-recovery-signal-\(UUID().uuidString)")
        let lifecycle = NativeLocalAgentHostLifecycle(configuration: .init(
            executableURL: root.appendingPathComponent("missing-host"),
            databaseURL: root.appendingPathComponent("agent.sqlite3"),
            startupTimeout: .milliseconds(100)
        ))
        let command = try JSONSerialization.data(withJSONObject: ["type": "health"])

        do {
            _ = try await lifecycle.request(command: command)
            XCTFail("A request without a running Host must fail after requesting recovery")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentHostError, .notRunning)
        }
        await fulfillment(of: [recoveryRequested], timeout: 1)
    }

    func testCredentialRefreshReusesWorkerAndDefersChangedCredentialsWithoutStoppingIt() async throws {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let executable = ProcessInfo.processInfo.environment["CHATOS_LOCAL_AGENT_HOST_PATH"]
            .map { URL(fileURLWithPath: $0) }
            ?? repository.appendingPathComponent("target-shared/debug/chatos_local_agent_host")
        guard FileManager.default.isExecutableFile(atPath: executable.path) else {
            if ProcessInfo.processInfo.environment["CHATOS_REQUIRE_NATIVE_HOST_TESTS"] == "1" {
                XCTFail("Required native Host integration executable is missing")
                throw NativeLocalAgentHostError.notRunning
            }
            throw XCTSkip("Build chatos_local_agent_host before running the native lifecycle test")
        }
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-refresh-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let lifecycle = NativeLocalAgentHostLifecycle(configuration: .init(
            executableURL: executable, databaseURL: root.appendingPathComponent("agent.sqlite3")
        ))
        do {
            try await lifecycle.startForBootstrap(ownerUserID: "user-1")
            let bootstrapPID = await lifecycle.processIdentifier
            let workspace = NativeLocalAgentWorkspaceService(host: lifecycle)
            await workspace.configure(ownerUserID: "user-1")
            let initialRelations = try await workspace.fetchWorkspaceRelations()
            XCTAssertEqual(initialRelations.contacts.map(\.id), [
                NativeLocalAgentWorkspaceService.mainContact.id,
                NativeLocalAgentWorkspaceService.pluginInstallerContact.id,
            ])
            XCTAssertEqual(initialRelations.conversations.count, 2)
            let credentialStore = NativeLocalAgentModelCredentialStore(rootURL: root.appendingPathComponent("credentials"))
            let modelSnapshots = ["test", "historical"].map { model in
                LocalAgentModelConfigSnapshot(ownerUserID: "user-1", modelConfigRef: model,
                    modelConfigRevision: "revision-1", credentialRef: "env:\(credentialStore.environmentVariable(modelConfigRef: model))",
                    baseURL: "https://example.invalid/v1", model: model, provider: "test", supportsResponses: true)
            }
            for model in ["test", "historical"] {
                try credentialStore.saveWithoutUserInteraction("test-only-value", ownerUserID: "user-1", modelConfigRef: model)
            }
            var credentials = try NativeLocalAgentCredentialEnvironment.restoredModels(modelSnapshots, ownerUserID: "user-1", store: credentialStore)
            credentials["CHATOS_MEMORY_ACCESS_TOKEN"] = "test-memory-value"
            try await lifecycle.refreshCredentials(ownerUserID: "user-1", credentialEnvironment: credentials)
            let workerPID = await lifecycle.processIdentifier
            XCTAssertNotNil(workerPID)
            XCTAssertNotEqual(workerPID, bootstrapPID)
            for _ in 0..<3 {
                var refreshed = try NativeLocalAgentCredentialEnvironment.restoredModels(modelSnapshots, ownerUserID: "user-1", store: credentialStore)
                // Online catalog only selects test; historical stays for frozen Runs.
                refreshed["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "test-only-value"
                refreshed["CHATOS_MEMORY_ACCESS_TOKEN"] = "test-memory-value"
                XCTAssertEqual(refreshed, credentials)
                try await lifecycle.refreshCredentials(ownerUserID: "user-1", credentialEnvironment: refreshed)
                let reusedPID = await lifecycle.processIdentifier
                XCTAssertEqual(reusedPID, workerPID)
                let restoredRelations = try await workspace.fetchWorkspaceRelations()
                XCTAssertEqual(restoredRelations.contacts, initialRelations.contacts)
                XCTAssertEqual(
                    restoredRelations.conversations.map(\.id).sorted(),
                    initialRelations.conversations.map(\.id).sorted()
                )
            }
            var rotatedModel = credentials
            rotatedModel["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "rotated-value"
            var rotatedMemory = credentials
            rotatedMemory["CHATOS_MEMORY_ACCESS_TOKEN"] = "rotated-memory-value"
            var missingMemory = credentials
            missingMemory.removeValue(forKey: "CHATOS_MEMORY_ACCESS_TOKEN")
            var addedModel = credentials
            addedModel["CHATOS_LOCAL_AGENT_MODEL_NEW"] = "new-test-value"
            for changed in [rotatedModel, rotatedMemory, missingMemory, addedModel] {
                do {
                    try await lifecycle.refreshCredentials(ownerUserID: "user-1", credentialEnvironment: changed)
                    XCTFail("Changed credentials must not replace an active worker")
                } catch {
                    XCTAssertTrue(error is NativeLocalAgentCredentialRefreshDeferred)
                }
                let stillRunningPID = await lifecycle.processIdentifier
                XCTAssertEqual(stillRunningPID, workerPID)
            }
            do {
                try await lifecycle.refreshCredentials(ownerUserID: "user-2", credentialEnvironment: credentials)
                XCTFail("Refresh must not switch accounts")
            } catch {
                XCTAssertEqual(error as? NativeLocalAgentHostError, .ownerMismatch)
            }
            let client = NativeLocalAgentConversationClient(host: lifecycle)
            _ = try await client.list(ownerUserID: "user-1")
            await lifecycle.stop()
            try await lifecycle.refreshCredentials(ownerUserID: "user-1", credentialEnvironment: ["CHATOS_LOCAL_AGENT_MODEL_TEST": "rotated-value"])
            let nextPID = await lifecycle.processIdentifier
            XCTAssertNotNil(nextPID)
            XCTAssertNotEqual(nextPID, workerPID)
            await lifecycle.stop()
        } catch {
            await lifecycle.stop()
            throw error
        }
    }

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
            if ProcessInfo.processInfo.environment["CHATOS_REQUIRE_NATIVE_HOST_TESTS"] == "1" {
                XCTFail("Required native Host integration executable is missing")
                throw NativeLocalAgentHostError.notRunning
            }
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
            defaultConversationModelConfigRef: "model-1",
            capabilitySnapshot: capabilities,
            capabilitySnapshotsByModelConfigRef: ["model-1": capabilities],
            pluginInstallerCapabilitySnapshotsByModelConfigRef: ["model-1": capabilities]
        )
        try await runtimeSettings.configure(ownerUserID: "user-1", bootstrap: bootstrap)
        try await conversationService.configure(
            ownerUserID: "user-1",
            bootstrap: bootstrap
        )
        let acknowledgement = try await conversationService.sendNewTurn(.init(
            sessionID: "conversation-1",
            turnID: "turn-1",
            content: "hello locally",
            explicitCapabilitySelection: .plugin(
                key: "document-plugin",
                componentKey: "document-mcp",
                displayName: "Document"
            )
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
        XCTAssertEqual(
            runInput["explicit_capability_selection"],
            .object([
                "kind": .string("plugin"),
                "plugin_key": .string("document-plugin"),
                "component_key": .string("document-mcp"),
                "display_name": .string("Document"),
            ])
        )
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

        let healthyProcessIdentifierValue = await lifecycle.processIdentifier
        let healthyProcessIdentifier = try XCTUnwrap(healthyProcessIdentifierValue)
        let idleCursor = try await runtime.latestEventCursor(ownerUserID: "user-2")
        do {
            _ = try await runtime.waitEvents(
                ownerUserID: "user-2",
                afterCursor: idleCursor,
                timeoutMilliseconds: 1_000,
                payloadMode: .routing
            )
            XCTFail("The client deadline must expire before an empty one-second event wait")
        } catch let error as NativeLocalAgentHostError {
            XCTAssertEqual(error, .requestTimedOut)
        }
        let processIdentifierAfterRequestTimeout = await lifecycle.processIdentifier
        XCTAssertEqual(processIdentifierAfterRequestTimeout, healthyProcessIdentifier)
        _ = try await conversations.list(ownerUserID: "user-2")

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

    func testFrameCodecChunksAndReassemblesLogicalMessages() throws {
        let pipe = Pipe()
        let payload = Data(
            repeating: 0x5A,
            count: LocalAgentHostFrameCodec.maximumFrameBytes + 257
        )
        let written = expectation(description: "chunked message written")
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try LocalAgentHostFrameCodec.write(payload, to: pipe.fileHandleForWriting)
                try pipe.fileHandleForWriting.close()
            } catch {
                XCTFail("chunked write failed: \(error)")
            }
            written.fulfill()
        }

        XCTAssertEqual(try LocalAgentHostFrameCodec.read(from: pipe.fileHandleForReading), payload)
        wait(for: [written], timeout: 2)
    }

    func testActualHostChunksLargeRequestsAndResponses() async throws {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let executable = ProcessInfo.processInfo.environment["CHATOS_LOCAL_AGENT_HOST_PATH"]
            .map { URL(fileURLWithPath: $0) }
            ?? repository.appendingPathComponent("target-shared/debug/chatos_local_agent_host")
        guard FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw XCTSkip("Build chatos_local_agent_host before running the native lifecycle test")
        }
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-large-ipc-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let databaseURL = root.appendingPathComponent("local-agent.sqlite3")
        let lifecycle = NativeLocalAgentHostLifecycle(configuration: .init(
            executableURL: executable,
            databaseURL: databaseURL
        ))

        func request(_ command: [String: Any]) async throws -> [String: Any] {
            let data = try JSONSerialization.data(withJSONObject: command)
            let result = try await lifecycle.request(command: data)
            return try XCTUnwrap(
                JSONSerialization.jsonObject(with: result) as? [String: Any]
            )
        }

        do {
            try await lifecycle.startForBootstrap(ownerUserID: "user-1")
            let response = try await request([
                "type": "health",
                "padding": String(
                    repeating: "x",
                    count: LocalAgentHostFrameCodec.maximumFrameBytes + 1_024
                ),
            ])
            XCTAssertEqual(response["type"] as? String, "health")
            let followup = try await request(["type": "health"])
            XCTAssertEqual(followup["type"] as? String, "health")
            await lifecycle.stop()

            var database: OpaquePointer?
            XCTAssertEqual(sqlite3_open_v2(
                databaseURL.path,
                &database,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
                nil
            ), SQLITE_OK)
            let openedDatabase = try XCTUnwrap(database)
            var statement: OpaquePointer?
            let sql = """
                INSERT INTO local_agent_runs(
                  run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key,
                  model_config_ref, model_config_revision, capability_policy_revision,
                  input_json, status, iteration, max_iterations, version,
                  terminal_outcome_json, created_at_unix_ms, updated_at_unix_ms
                ) VALUES(
                  'large-response-run', 'user-1', 'conversation', 'large-response-conversation',
                  'main_chat', 'model-1', 'revision-1', 'policy-1', '{}', 'succeeded',
                  1, 4, 1, ?, 1, 1
                )
                """
            XCTAssertEqual(
                sqlite3_prepare_v2(openedDatabase, sql, -1, &statement, nil),
                SQLITE_OK
            )
            let prepared = try XCTUnwrap(statement)
            let largeContent = String(
                repeating: "y",
                count: LocalAgentHostFrameCodec.maximumFrameBytes + 1_024
            )
            let terminalOutcome = try JSONSerialization.data(withJSONObject: [
                "content": largeContent,
            ])
            let terminalOutcomeText = String(decoding: terminalOutcome, as: UTF8.self)
            let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
            XCTAssertEqual(terminalOutcomeText.withCString {
                sqlite3_bind_text(prepared, 1, $0, -1, transient)
            }, SQLITE_OK)
            XCTAssertEqual(sqlite3_step(prepared), SQLITE_DONE)
            sqlite3_finalize(prepared)
            sqlite3_close(openedDatabase)

            try await lifecycle.startForBootstrap(ownerUserID: "user-1")
            let run = try await NativeLocalAgentRuntimeClient(host: lifecycle).run(
                ownerUserID: "user-1",
                runID: "large-response-run"
            )
            guard case let .object(outcome)? = run.terminalOutcome,
                  case let .string(roundTrippedContent)? = outcome["content"] else {
                return XCTFail("Large terminal outcome did not round-trip")
            }
            XCTAssertEqual(roundTrippedContent.count, largeContent.count)
            await lifecycle.stop()
        } catch {
            await lifecycle.stop()
            throw error
        }
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
            memoryTimeoutMilliseconds: 12_345,
            maximumConcurrentWorkItems: 7
        )
        let arguments = ManagedLocalAgentHostProcess.arguments(
            configuration: configuration,
            database: database,
            ownerUserID: "user-1"
        )
        XCTAssertTrue(arguments.contains("https://gateway.example/api/memory"))
        XCTAssertTrue(arguments.contains("local_agent"))
        XCTAssertTrue(arguments.contains("12345"))
        XCTAssertTrue(arguments.contains("--max-concurrent-work-items"))
        XCTAssertTrue(arguments.contains("7"))
        XCTAssertFalse(arguments.contains("memory-secret"))
        XCTAssertFalse(arguments.contains("--disable-workers"))
        XCTAssertEqual(arguments.last, "--stdio")

        let bootstrapArguments = ManagedLocalAgentHostProcess.arguments(
            configuration: configuration,
            database: database,
            ownerUserID: "user-1",
            workersEnabled: false
        )
        XCTAssertEqual(bootstrapArguments.suffix(2), ["--disable-workers", "--stdio"])

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
            Data(count: LocalAgentHostFrameCodec.maximumMessageBytes + 1),
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
