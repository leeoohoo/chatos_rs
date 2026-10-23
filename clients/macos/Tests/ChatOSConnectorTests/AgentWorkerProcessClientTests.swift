import ChatOSAgentRuntime
import Darwin
import Foundation
import XCTest
@testable import ChatOSConnector

final class AgentWorkerProcessClientTests: XCTestCase {
    func testBuiltWorkerRunsAgentLoopAndPersistsThroughParentCallbacks() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_AGENT_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_AGENT_WORKER_PATH to run Agent worker integration.")
        }
        let scope = "agent-worker-process-test"
        let runID = UUID()
        var checkpoint = AgentRunCheckpoint(
            scope: scope,
            messages: [.init(role: .user, content: "完成隔离运行")]
        )
        checkpoint.id = runID
        let memoryScope = try AgentMemoryScope(
            tenantID: "agent-worker-test",
            todoID: "isolated-todo",
            runID: runID,
            runtimeScope: scope
        )
        let contextProvider = AgentMemoryContextProvider(
            scope: memoryScope,
            service: AgentWorkerTestMemory()
        )
        checkpoint = try contextProvider.bind(checkpoint)
        var policy = AgentRunPolicy()
        policy.maximumModelCalls = 2
        policy.maximumRequestRetries = 0
        policy.maximumNoProgressRounds = 2
        policy.requestTimeoutSeconds = 10
        policy.runTimeoutSeconds = 30
        let events = AgentWorkerRecordedEvents()
        let tool = AgentToolDefinition(
            name: "finish",
            description: "完成运行",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .terminal
        )

        let result = try await AgentWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            shutdownGraceSeconds: 5
        ).executeRun(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            model: AgentWorkerTestModel(),
            tools: [tool],
            execute: { call in
                XCTAssertEqual(call.name, "finish")
                return .init("worker-completed")
            },
            completionCheck: { nil },
            contextProvider: contextProvider,
            record: { saved, event in
                await events.append(checkpoint: saved, event: event)
            }
        )

        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.result, "worker-completed")
        XCTAssertEqual(result.modelCalls, 1)
        let kinds = await events.kinds()
        XCTAssertTrue(kinds.contains("model_request"))
        XCTAssertTrue(kinds.contains("tool_completed"))
        XCTAssertTrue(kinds.contains("completed"))
    }

    func testConcurrentRunsUseDistinctWorkerProcessesAndDoNotBlockEachOther() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_AGENT_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_AGENT_WORKER_PATH to run Agent worker integration.")
        }
        let executable = URL(fileURLWithPath: path)
        let gate = AgentWorkerProcessGate()
        let processes = AgentWorkerProcessIDs()
        let client = AgentWorkerProcessClient(
            executableURL: executable,
            shutdownGraceSeconds: 5,
            processObserver: { processID in
                Task { await processes.append(processID) }
            }
        )
        let first = Task {
            try await Self.runIsolatedWorker(
                client: client,
                scope: "agent-worker-first",
                model: AgentWorkerGatedModel(gate: gate, shouldWait: true, callID: "first")
            )
        }
        await gate.waitUntilBlocked()

        let second = try await Self.runIsolatedWorker(
            client: client,
            scope: "agent-worker-second",
            model: AgentWorkerGatedModel(gate: gate, shouldWait: false, callID: "second")
        )
        XCTAssertEqual(second.status, .completed)
        let processIDs = await processes.waitForCount(2)
        XCTAssertEqual(Set(processIDs).count, 2)

        await gate.release()
        let firstResult = try await first.value
        XCTAssertEqual(firstResult.status, .completed)
    }

    func testDirectModelTransportRunsProviderNetworkingInsideWorker() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_AGENT_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_AGENT_WORKER_PATH to run Agent worker integration.")
        }
        let requestProbe = AgentWorkerHTTPRequestProbe()
        let server = try AgentWorkerHTTPFixture(probe: requestProbe)
        defer { server.close() }
        let modelProbe = AgentWorkerDirectModelProbe()
        let configuration = AgentWorkerModelTransportConfiguration.responses(try .init(
            baseURL: server.baseURL,
            model: "worker-model",
            apiKey: "worker-secret",
            thinking: nil,
            maximumOutputTokens: 1_024,
            temperature: nil,
            promptCacheKey: "worker-direct-test"
        ))
        let result = try await Self.runIsolatedWorker(
            client: AgentWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                shutdownGraceSeconds: 5
            ),
            scope: "agent-worker-direct-model",
            model: AgentWorkerDirectModel(
                configuration: configuration,
                probe: modelProbe
            )
        )

        XCTAssertEqual(result.status, .completed)
        let modelSnapshot = await modelProbe.snapshot()
        XCTAssertEqual(modelSnapshot.authorizations, 3)
        XCTAssertEqual(modelSnapshot.parentModelCalls, 0)
        let recordedRequest = await requestProbe.request()
        let request = try XCTUnwrap(recordedRequest)
        let lowercasedRequest = request.lowercased()
        XCTAssertTrue(lowercasedRequest.contains("post /v1/responses "))
        XCTAssertTrue(lowercasedRequest.contains("authorization: bearer worker-secret"))
    }

    func testNetworkDisconnectReconnectsWithinItsWorkerRun() async throws {
        let executable = try workerExecutable()
        let requestProbe = AgentWorkerHTTPRequestProbe()
        let server = try AgentWorkerHTTPFixture(
            probe: requestProbe,
            disconnectCountBeforeResponse: 1
        )
        defer { server.close() }
        let authorizationProbe = AgentWorkerDirectModelProbe()
        let configuration = AgentWorkerModelTransportConfiguration.responses(try .init(
            baseURL: server.baseURL,
            model: "worker-reconnect-model",
            apiKey: "worker-reconnect-secret",
            thinking: nil,
            maximumOutputTokens: 1_024,
            temperature: nil,
            promptCacheKey: "worker-reconnect-test"
        ))
        let processes = AgentWorkerProcessIDs()
        var policy = testPolicy()
        policy.maximumRequestRetries = 1
        let result = try await Self.runIsolatedWorker(
            client: Self.isolatedClient(executable: executable, processes: processes),
            scope: "agent-worker-network-reconnect",
            model: AgentWorkerDirectModel(
                configuration: configuration,
                probe: authorizationProbe
            ),
            policy: policy
        )

        XCTAssertEqual(result.status, .completed)
        let requests = await requestProbe.waitForCount(2)
        XCTAssertEqual(requests.count, 2)
        let processIDs = await processes.waitForCount(1)
        XCTAssertEqual(Set(processIDs).count, 1)
    }

    func testAccountSwitchAfterProviderResponseRejectsResultBeforeToolExecution() async throws {
        let executable = try workerExecutable()
        let requestProbe = AgentWorkerHTTPRequestProbe()
        let responseGate = AgentWorkerHTTPResponseGate()
        let server = try AgentWorkerHTTPFixture(
            probe: requestProbe,
            responseGate: responseGate
        )
        defer {
            Task { await responseGate.release() }
            server.close()
        }
        let session = AgentWorkerAuthenticationSession()
        let configuration = AgentWorkerModelTransportConfiguration.responses(try .init(
            baseURL: server.baseURL,
            model: "worker-account-switch-model",
            apiKey: "old-account-secret",
            thinking: nil,
            maximumOutputTokens: 1_024,
            temperature: nil,
            promptCacheKey: "worker-account-switch-test"
        ))
        let executionCount = AgentWorkerExecutionCount()
        let scope = "agent-worker-account-switch"
        let task = Task {
            try await Self.runIsolatedWorker(
                client: Self.isolatedClient(
                    executable: executable,
                    processes: AgentWorkerProcessIDs()
                ),
                scope: scope,
                model: AgentWorkerSessionBoundModel(
                    configuration: configuration,
                    session: session
                ),
                execute: { _ in
                    await executionCount.increment()
                    return .init("must-not-execute")
                }
            )
        }

        await requestProbe.waitUntilRequest()
        await session.invalidate()
        await responseGate.release()
        let result = try await task.value
        XCTAssertEqual(result.status, .failed)
        XCTAssertTrue(result.pendingCalls.isEmpty)
        XCTAssertNil(result.inFlightCallID)
        let calls = await executionCount.value()
        XCTAssertEqual(calls, 0)
        let validationCount = await session.validationCount()
        XCTAssertEqual(validationCount, 3)
    }

    func testSuspendedNetworkRunDoesNotBlockAnotherConversationAndResumes() async throws {
        let executable = try workerExecutable()
        let requestProbe = AgentWorkerHTTPRequestProbe()
        let responseGate = AgentWorkerHTTPResponseGate()
        let server = try AgentWorkerHTTPFixture(
            probe: requestProbe,
            responseGate: responseGate
        )
        defer {
            Task { await responseGate.release() }
            server.close()
        }
        let configuration = AgentWorkerModelTransportConfiguration.responses(try .init(
            baseURL: server.baseURL,
            model: "worker-suspended-model",
            apiKey: "worker-suspended-secret",
            thinking: nil,
            maximumOutputTokens: 1_024,
            temperature: nil,
            promptCacheKey: "worker-suspended-test"
        ))
        let processes = AgentWorkerProcessIDs()
        let client = Self.isolatedClient(executable: executable, processes: processes)
        let suspended = Task {
            try await Self.runIsolatedWorker(
                client: client,
                scope: "agent-worker-suspended-network",
                model: AgentWorkerDirectModel(
                    configuration: configuration,
                    probe: AgentWorkerDirectModelProbe()
                )
            )
        }
        await requestProbe.waitUntilRequest()

        let independent = try await Self.runIsolatedWorker(
            client: client,
            scope: "agent-worker-independent-while-suspended",
            model: AgentWorkerTestModel()
        )
        XCTAssertEqual(independent.status, .completed)
        let processIDs = await processes.waitForCount(2)
        XCTAssertEqual(Set(processIDs).count, 2)

        await responseGate.release()
        let resumed = try await suspended.value
        XCTAssertEqual(resumed.status, .completed)
    }

    func testSixteenIndependentRunsCompleteWithDistinctWorkerProcesses() async throws {
        let executable = try workerExecutable()
        let processes = AgentWorkerProcessIDs()
        let client = Self.isolatedClient(executable: executable, processes: processes)
        let runCount = 16
        let results = try await withThrowingTaskGroup(
            of: AgentRunCheckpoint.self,
            returning: [AgentRunCheckpoint].self
        ) { group in
            for index in 0..<runCount {
                group.addTask {
                    try await Self.runIsolatedWorker(
                        client: client,
                        scope: "agent-worker-soak-\(index)",
                        model: AgentWorkerTestModel()
                    )
                }
            }
            var values: [AgentRunCheckpoint] = []
            for try await value in group { values.append(value) }
            return values
        }

        XCTAssertEqual(results.count, runCount)
        XCTAssertTrue(results.allSatisfy { $0.status == .completed })
        let processIDs = await processes.waitForCount(runCount)
        XCTAssertEqual(processIDs.count, runCount)
        XCTAssertEqual(Set(processIDs).count, runCount)
    }

    func testCancellingRunTerminatesOnlyItsWorkerProcess() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_AGENT_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_AGENT_WORKER_PATH to run Agent worker integration.")
        }
        let processes = AgentWorkerProcessIDs()
        let probe = AgentWorkerCancellationProbe()
        let client = AgentWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            shutdownGraceSeconds: 5,
            resourcePolicy: .init(),
            processObserver: { processID in
                Task { await processes.append(processID) }
            }
        )
        let task = Task {
            try await Self.runIsolatedWorker(
                client: client,
                scope: "agent-worker-cancelled",
                model: AgentWorkerCancellationModel(probe: probe)
            )
        }
        await probe.waitUntilStarted()
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("cancelled Agent Run unexpectedly completed")
        } catch is CancellationError {
            // Expected: cancelling one Run resolves without waiting for another Run or the worker.
        }
        let processIDs = await processes.waitForCount(1)
        let processID = try XCTUnwrap(processIDs.first)
        var isAlive = true
        for _ in 0..<100 {
            errno = 0
            isAlive = Darwin.kill(processID, 0) == 0 || errno == EPERM
            if !isAlive { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(isAlive)
    }

    func testKilledWorkerDoesNotQuarantineAnIndependentRun() async throws {
        let executable = try workerExecutable()
        let processes = AgentWorkerProcessIDs()
        let authorizationGate = AgentWorkerAuthorizationGate()
        let configuration = AgentWorkerModelTransportConfiguration.responses(try .init(
            baseURL: URL(string: "http://127.0.0.1:1/v1/")!,
            model: "failure-isolation-model",
            apiKey: "failure-isolation-secret",
            thinking: nil,
            maximumOutputTokens: 1_024,
            temperature: nil,
            promptCacheKey: "failure-isolation"
        ))
        let client = Self.isolatedClient(executable: executable, processes: processes)
        let interrupted = Task {
            try await Self.runIsolatedWorker(
                client: client,
                scope: "agent-worker-killed-before-independent-run",
                model: AgentWorkerBlockingAuthorizationModel(
                    configuration: configuration,
                    gate: authorizationGate
                )
            )
        }
        await authorizationGate.waitUntilWorkerAuthorization()
        let firstProcessIDs = await processes.waitForCount(1)
        let firstProcessID = try XCTUnwrap(firstProcessIDs.first)
        XCTAssertEqual(Darwin.kill(firstProcessID, SIGKILL), 0)
        await authorizationGate.release()
        try await assertInfrastructureInterruption(interrupted)

        let independent = try await Self.runIsolatedWorker(
            client: client,
            scope: "agent-worker-independent-after-kill",
            model: AgentWorkerTestModel()
        )
        XCTAssertEqual(independent.status, .completed)
        let processIDs = await processes.waitForCount(2)
        XCTAssertEqual(processIDs.count, 2)
        XCTAssertEqual(Set(processIDs).count, 2)
    }

    func testMemoryLimitTerminatesOnlyTheOffendingWorker() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_AGENT_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_AGENT_WORKER_PATH to run Agent worker integration.")
        }
        let executable = URL(fileURLWithPath: path)
        let healthyGate = AgentWorkerProcessGate()
        let healthyProcesses = AgentWorkerProcessIDs()
        let healthyClient = AgentWorkerProcessClient(
            executableURL: executable,
            shutdownGraceSeconds: 5,
            resourcePolicy: .init(),
            processObserver: { processID in
                Task { await healthyProcesses.append(processID) }
            }
        )
        let healthyTask = Task {
            try await Self.runIsolatedWorker(
                client: healthyClient,
                scope: "agent-worker-resource-healthy",
                model: AgentWorkerGatedModel(
                    gate: healthyGate,
                    shouldWait: true,
                    callID: "healthy"
                )
            )
        }
        await healthyGate.waitUntilBlocked()

        let limitedProbe = AgentWorkerCancellationProbe()
        let limitedProcesses = AgentWorkerProcessIDs()
        let limitedClient = AgentWorkerProcessClient(
            executableURL: executable,
            shutdownGraceSeconds: 5,
            resourcePolicy: .init(
                maximumPhysicalMemoryBytes: 1,
                maximumCPUTimeSeconds: nil,
                sampleIntervalSeconds: 0.01
            ),
            processObserver: { processID in
                Task { await limitedProcesses.append(processID) }
            }
        )
        do {
            _ = try await Self.runIsolatedWorker(
                client: limitedClient,
                scope: "agent-worker-resource-limited",
                model: AgentWorkerCancellationModel(probe: limitedProbe)
            )
            XCTFail("memory-limited Agent Run unexpectedly completed")
        } catch let error as AgentWorkerProcessClientError {
            guard case let .resourceLimitExceeded(resource, observed, limit) = error else {
                throw error
            }
            XCTAssertEqual(resource, "物理内存")
            XCTAssertGreaterThan(observed, limit)
        }

        let healthyIDs = await healthyProcesses.waitForCount(1)
        let limitedIDs = await limitedProcesses.waitForCount(1)
        XCTAssertEqual(healthyIDs.count, 1)
        XCTAssertEqual(limitedIDs.count, 1)
        XCTAssertNotEqual(healthyIDs.first, limitedIDs.first)
        let healthyPID = try XCTUnwrap(healthyIDs.first)
        errno = 0
        XCTAssertTrue(Darwin.kill(healthyPID, 0) == 0 || errno == EPERM)

        await healthyGate.release()
        let healthyResult = try await healthyTask.value
        XCTAssertEqual(healthyResult.status, .completed)
    }

    func testSIGKILLDuringWorkerAuthorizationFailsOnlyThatRun() async throws {
        let executable = try workerExecutable()
        let processes = AgentWorkerProcessIDs()
        let authorizationGate = AgentWorkerAuthorizationGate()
        let configuration = AgentWorkerModelTransportConfiguration.responses(try .init(
            baseURL: URL(string: "http://127.0.0.1:1/v1/")!,
            model: "authorization-kill-model",
            apiKey: "authorization-kill-secret",
            thinking: nil,
            maximumOutputTokens: 1_024,
            temperature: nil,
            promptCacheKey: "authorization-kill"
        ))
        let client = Self.isolatedClient(executable: executable, processes: processes)
        let task = Task {
            try await Self.runIsolatedWorker(
                client: client,
                scope: "agent-worker-authorization-kill",
                model: AgentWorkerBlockingAuthorizationModel(
                    configuration: configuration,
                    gate: authorizationGate
                )
            )
        }

        await authorizationGate.waitUntilWorkerAuthorization()
        let processIDs = await processes.waitForCount(1)
        let processID = try XCTUnwrap(processIDs.first)
        XCTAssertEqual(Darwin.kill(processID, SIGKILL), 0)
        await authorizationGate.release()
        try await assertInfrastructureInterruption(task)
    }

    func testSIGKILLDuringWorkerNetworkRequestFailsOnlyThatRun() async throws {
        let executable = try workerExecutable()
        let requestProbe = AgentWorkerHTTPRequestProbe()
        let responseGate = AgentWorkerHTTPResponseGate()
        let server = try AgentWorkerHTTPFixture(
            probe: requestProbe,
            responseGate: responseGate
        )
        defer {
            Task { await responseGate.release() }
            server.close()
        }
        let processes = AgentWorkerProcessIDs()
        let configuration = AgentWorkerModelTransportConfiguration.responses(try .init(
            baseURL: server.baseURL,
            model: "network-kill-model",
            apiKey: "network-kill-secret",
            thinking: nil,
            maximumOutputTokens: 1_024,
            temperature: nil,
            promptCacheKey: "network-kill"
        ))
        let task = Task {
            try await Self.runIsolatedWorker(
                client: Self.isolatedClient(executable: executable, processes: processes),
                scope: "agent-worker-network-kill",
                model: AgentWorkerDirectModel(
                    configuration: configuration,
                    probe: AgentWorkerDirectModelProbe()
                )
            )
        }

        await requestProbe.waitUntilRequest()
        let processIDs = await processes.waitForCount(1)
        let processID = try XCTUnwrap(processIDs.first)
        XCTAssertEqual(Darwin.kill(processID, SIGKILL), 0)
        await responseGate.release()
        try await assertInfrastructureInterruption(task)
    }

    func testSIGKILLDuringWriteToolIPCLeavesCheckpointForReviewWithoutReplay() async throws {
        let executable = try workerExecutable()
        let processes = AgentWorkerProcessIDs()
        let executionGate = AgentWorkerProcessGate()
        let records = AgentWorkerCheckpointProbe()
        let scope = "agent-worker-tool-kill"
        var checkpoint = AgentRunCheckpoint(
            scope: scope,
            messages: [.init(role: .user, content: scope)]
        )
        checkpoint.id = UUID()
        let policy = testPolicy()
        let writeTool = AgentToolDefinition(
            name: "write_state",
            description: "写入状态",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .write
        )
        let task = Task {
            try await Self.isolatedClient(
                executable: executable,
                processes: processes
            ).executeRun(
                checkpoint: checkpoint,
                scope: scope,
                policy: policy,
                model: AgentWorkerWriteModel(),
                tools: [writeTool],
                execute: { _ in
                    await executionGate.block()
                    return .init("write-finished")
                },
                completionCheck: { nil },
                contextProvider: nil,
                record: { saved, event in
                    await records.record(checkpoint: saved, event: event)
                }
            )
        }

        await executionGate.waitUntilBlocked()
        let processIDs = await processes.waitForCount(1)
        let processID = try XCTUnwrap(processIDs.first)
        XCTAssertEqual(Darwin.kill(processID, SIGKILL), 0)
        await executionGate.release()
        try await assertInfrastructureInterruption(task)

        let latestCheckpoint = await records.latestCheckpoint()
        let interrupted = try XCTUnwrap(latestCheckpoint)
        XCTAssertEqual(interrupted.inFlightCallID, "write-call")
        XCTAssertEqual(interrupted.pendingCalls.map(\.id), ["write-call"])
        let replayProbe = AgentWorkerExecutionCount()
        let resumed = try await AgentRuntime().run(
            checkpoint: interrupted,
            scope: scope,
            policy: policy,
            model: AgentWorkerWriteModel(),
            tools: [writeTool],
            execute: { _ in
                await replayProbe.increment()
                return .init("must-not-replay")
            },
            record: { _, _ in }
        )
        XCTAssertEqual(resumed.status, .needsReview)
        XCTAssertNotNil(resumed.inFlightCallID)
        let replayCount = await replayProbe.value()
        XCTAssertEqual(replayCount, 0)
    }

    private func workerExecutable() throws -> URL {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_AGENT_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_AGENT_WORKER_PATH to run Agent worker integration.")
        }
        return URL(fileURLWithPath: path)
    }

    private static func isolatedClient(
        executable: URL,
        processes: AgentWorkerProcessIDs
    ) -> AgentWorkerProcessClient {
        AgentWorkerProcessClient(
            executableURL: executable,
            shutdownGraceSeconds: 5,
            resourcePolicy: .init(),
            processObserver: { processID in
                Task { await processes.append(processID) }
            }
        )
    }

    private func assertInfrastructureInterruption(
        _ task: Task<AgentRunCheckpoint, Error>
    ) async throws {
        do {
            _ = try await task.value
            XCTFail("SIGKILLed Agent Run unexpectedly completed")
        } catch let error as AgentWorkerProcessClientError {
            guard case .invalidResponse = error else { throw error }
        }
    }

    private func testPolicy() -> AgentRunPolicy {
        var policy = AgentRunPolicy()
        policy.maximumModelCalls = 2
        policy.maximumRequestRetries = 0
        policy.maximumNoProgressRounds = 2
        policy.requestTimeoutSeconds = 10
        policy.runTimeoutSeconds = 30
        return policy
    }

    private static func runIsolatedWorker(
        client: AgentWorkerProcessClient,
        scope: String,
        model: any AgentModelClient,
        policy: AgentRunPolicy = testPolicy(),
        execute: @escaping AgentRuntime.Executor = { _ in .init("completed") }
    ) async throws -> AgentRunCheckpoint {
        var checkpoint = AgentRunCheckpoint(
            scope: scope,
            messages: [.init(role: .user, content: scope)]
        )
        checkpoint.id = UUID()
        return try await client.executeRun(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            model: model,
            tools: [.init(
                name: "finish",
                description: "完成运行",
                schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
                effect: .terminal
            )],
            execute: execute,
            completionCheck: { nil },
            contextProvider: nil,
            record: { _, _ in }
        )
    }

    private static func testPolicy() -> AgentRunPolicy {
        var policy = AgentRunPolicy()
        policy.maximumModelCalls = 2
        policy.maximumRequestRetries = 0
        policy.maximumNoProgressRounds = 2
        policy.requestTimeoutSeconds = 10
        policy.runTimeoutSeconds = 30
        return policy
    }
}

private actor AgentWorkerRecordedEvents {
    private var values: [(AgentRunCheckpoint, AgentRunEvent)] = []

    func append(checkpoint: AgentRunCheckpoint, event: AgentRunEvent) {
        values.append((checkpoint, event))
    }

    func kinds() -> [String] { values.map { $0.1.kind } }
}

private struct AgentWorkerTestModel: AgentModelClient {
    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        XCTAssertTrue(tools.contains { $0.name == "finish" })
        return .init(role: .assistant, toolCalls: [
            .init(id: "finish-call", name: "finish", arguments: "{}"),
        ])
    }
}

private actor AgentWorkerTestMemory: AgentMemoryServicing {
    private var messages: [AgentMemoryEntry] = []

    func ensureThread() async throws {}

    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        messages.append(contentsOf: entries)
    }

    func compose() async throws -> AgentMemoryContext {
        .init(blocks: [], recentRecords: messages.map {
            .init(id: $0.id, message: $0.message)
        })
    }
}

private actor AgentWorkerProcessGate {
    private var blocked = false
    private var released = false
    private var blockedWaiters: [CheckedContinuation<Void, Never>] = []
    private var releaseWaiters: [CheckedContinuation<Void, Never>] = []

    func block() async {
        blocked = true
        let waiters = blockedWaiters
        blockedWaiters.removeAll()
        waiters.forEach { $0.resume() }
        guard !released else { return }
        await withCheckedContinuation { releaseWaiters.append($0) }
    }

    func waitUntilBlocked() async {
        guard !blocked else { return }
        await withCheckedContinuation { blockedWaiters.append($0) }
    }

    func release() {
        released = true
        let waiters = releaseWaiters
        releaseWaiters.removeAll()
        waiters.forEach { $0.resume() }
    }
}

private actor AgentWorkerProcessIDs {
    private var values: [Int32] = []

    func append(_ value: Int32) { values.append(value) }

    func waitForCount(_ count: Int) async -> [Int32] {
        for _ in 0..<100 {
            if values.count >= count { return Array(values.prefix(count)) }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return values
    }
}

private struct AgentWorkerGatedModel: AgentModelClient {
    let gate: AgentWorkerProcessGate
    let shouldWait: Bool
    let callID: String

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        if shouldWait { await gate.block() }
        return .init(role: .assistant, toolCalls: [
            .init(id: callID, name: "finish", arguments: "{}"),
        ])
    }
}

private actor AgentWorkerCancellationProbe {
    private var started = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func markStarted() {
        started = true
        let current = waiters
        waiters.removeAll()
        current.forEach { $0.resume() }
    }

    func waitUntilStarted() async {
        guard !started else { return }
        await withCheckedContinuation { waiters.append($0) }
    }
}

private actor AgentWorkerAuthorizationGate {
    private var authorizationCount = 0
    private var workerAuthorizationStarted = false
    private var released = false
    private var startWaiters: [CheckedContinuation<Void, Never>] = []
    private var releaseWaiters: [CheckedContinuation<Void, Never>] = []

    func authorize() async {
        authorizationCount += 1
        guard authorizationCount == 2 else { return }
        workerAuthorizationStarted = true
        let waiters = startWaiters
        startWaiters.removeAll()
        waiters.forEach { $0.resume() }
        guard !released else { return }
        await withCheckedContinuation { releaseWaiters.append($0) }
    }

    func waitUntilWorkerAuthorization() async {
        guard !workerAuthorizationStarted else { return }
        await withCheckedContinuation { startWaiters.append($0) }
    }

    func release() {
        released = true
        let waiters = releaseWaiters
        releaseWaiters.removeAll()
        waiters.forEach { $0.resume() }
    }
}

private struct AgentWorkerBlockingAuthorizationModel: AgentModelClient {
    let configuration: AgentWorkerModelTransportConfiguration
    let gate: AgentWorkerAuthorizationGate

    var usesServerSideCompaction: Bool { true }
    var workerTransportConfiguration: AgentWorkerModelTransportConfiguration? { configuration }

    func validateWorkerTransportAuthorization() async throws {
        await gate.authorize()
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        throw AgentRuntimeError.invalidResponse
    }
}

private struct AgentWorkerWriteModel: AgentModelClient {
    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        .init(role: .assistant, toolCalls: [
            .init(id: "write-call", name: "write_state", arguments: "{}"),
        ])
    }
}

private actor AgentWorkerCheckpointProbe {
    private var latest: AgentRunCheckpoint?

    func record(checkpoint: AgentRunCheckpoint, event: AgentRunEvent) {
        latest = checkpoint
    }

    func latestCheckpoint() -> AgentRunCheckpoint? { latest }
}

private actor AgentWorkerExecutionCount {
    private var count = 0
    func increment() { count += 1 }
    func value() -> Int { count }
}

private struct AgentWorkerCancellationModel: AgentModelClient {
    let probe: AgentWorkerCancellationProbe

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        await probe.markStarted()
        try await Task.sleep(for: .seconds(60))
        return .init(role: .assistant)
    }
}

private actor AgentWorkerDirectModelProbe {
    private var authorizations = 0
    private var parentModelCalls = 0

    func authorize() { authorizations += 1 }
    func recordParentModelCall() { parentModelCalls += 1 }
    func snapshot() -> (authorizations: Int, parentModelCalls: Int) {
        (authorizations, parentModelCalls)
    }
}

private struct AgentWorkerDirectModel: AgentModelClient {
    let configuration: AgentWorkerModelTransportConfiguration
    let probe: AgentWorkerDirectModelProbe

    var usesServerSideCompaction: Bool { true }
    var workerTransportConfiguration: AgentWorkerModelTransportConfiguration? { configuration }

    func validateWorkerTransportAuthorization() async throws {
        await probe.authorize()
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        await probe.recordParentModelCall()
        throw AgentRuntimeError.invalidResponse
    }
}

private enum AgentWorkerAuthenticationError: LocalizedError {
    case sessionChanged
    var errorDescription: String? { "authentication session changed" }
}

private actor AgentWorkerAuthenticationSession {
    private var valid = true
    private var validations = 0

    func validate() throws {
        validations += 1
        guard valid else { throw AgentWorkerAuthenticationError.sessionChanged }
    }

    func invalidate() { valid = false }
    func validationCount() -> Int { validations }
}

private struct AgentWorkerSessionBoundModel: AgentModelClient {
    let configuration: AgentWorkerModelTransportConfiguration
    let session: AgentWorkerAuthenticationSession

    var usesServerSideCompaction: Bool { true }
    var workerTransportConfiguration: AgentWorkerModelTransportConfiguration? { configuration }

    func validateWorkerTransportAuthorization() async throws {
        try await session.validate()
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        throw AgentRuntimeError.invalidResponse
    }
}

private actor AgentWorkerHTTPRequestProbe {
    private var values: [String] = []
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func record(_ request: String) {
        values.append(request)
        let current = waiters
        waiters.removeAll()
        current.forEach { $0.resume() }
    }

    func request() -> String? { values.last }

    func waitUntilRequest() async {
        guard values.isEmpty else { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func waitForCount(_ count: Int) async -> [String] {
        for _ in 0..<500 {
            if values.count >= count { return Array(values.prefix(count)) }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return values
    }
}

private actor AgentWorkerHTTPResponseGate {
    private var released = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        guard !released else { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func release() {
        released = true
        let current = waiters
        waiters.removeAll()
        current.forEach { $0.resume() }
    }
}

private final class AgentWorkerHTTPFixture: @unchecked Sendable {
    let baseURL: URL
    private let descriptor: Int32

    init(
        probe: AgentWorkerHTTPRequestProbe,
        responseGate: AgentWorkerHTTPResponseGate? = nil,
        disconnectCountBeforeResponse: Int = 0
    ) throws {
        guard disconnectCountBeforeResponse >= 0 else {
            throw AgentWorkerProcessClientError.launchFailed
        }
        let descriptor = socket(AF_INET, SOCK_STREAM, 0)
        guard descriptor >= 0 else { throw AgentWorkerProcessClientError.launchFailed }
        var reuse: Int32 = 1
        guard setsockopt(
            descriptor,
            SOL_SOCKET,
            SO_REUSEADDR,
            &reuse,
            socklen_t(MemoryLayout<Int32>.size)
        ) == 0 else {
            Darwin.close(descriptor)
            throw AgentWorkerProcessClientError.launchFailed
        }
        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = 0
        address.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))
        let bound = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(descriptor, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(descriptor, 1) == 0 else {
            Darwin.close(descriptor)
            throw AgentWorkerProcessClientError.launchFailed
        }
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let named = withUnsafeMutablePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(descriptor, $0, &length)
            }
        }
        guard named == 0,
              let baseURL = URL(string: "http://127.0.0.1:\(UInt16(bigEndian: address.sin_port))/v1/") else {
            Darwin.close(descriptor)
            throw AgentWorkerProcessClientError.launchFailed
        }
        self.descriptor = descriptor
        self.baseURL = baseURL
        Task.detached(priority: .userInitiated) {
            await Self.serve(
                descriptor: descriptor,
                probe: probe,
                responseGate: responseGate,
                disconnectCountBeforeResponse: disconnectCountBeforeResponse
            )
        }
    }

    func close() {
        _ = shutdown(descriptor, SHUT_RDWR)
        Darwin.close(descriptor)
    }

    private static func serve(
        descriptor: Int32,
        probe: AgentWorkerHTTPRequestProbe,
        responseGate: AgentWorkerHTTPResponseGate?,
        disconnectCountBeforeResponse: Int
    ) async {
        for attempt in 0...disconnectCountBeforeResponse {
            let client = accept(descriptor, nil, nil)
            guard client >= 0 else { return }
            _ = fcntl(client, F_SETNOSIGPIPE, 1)
            guard let request = readRequest(client) else {
                Darwin.close(client)
                return
            }
            await probe.record(String(decoding: request, as: UTF8.self))
            if attempt < disconnectCountBeforeResponse {
                _ = shutdown(client, SHUT_RDWR)
                Darwin.close(client)
                continue
            }
            await responseGate?.wait()
            let body = Data("""
            data: {"type":"response.created","response":{"id":"resp_worker"}}

            data: {"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"finish-call","name":"finish","arguments":""}}

            data: {"type":"response.function_call_arguments.delta","output_index":0,"delta":"{}"}

            data: {"type":"response.completed","response":{"id":"resp_worker","status":"completed","output":[{"type":"function_call","call_id":"finish-call","name":"finish","arguments":"{}"}]}}

            """.utf8)
            let header = Data("""
            HTTP/1.1 200 OK\r
            Content-Type: text/event-stream\r
            Content-Length: \(body.count)\r
            Connection: close\r
            \r

            """.utf8)
            writeAll(header + body, descriptor: client)
            Darwin.close(client)
        }
    }

    private static func readRequest(_ descriptor: Int32) -> Data? {
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4_096)
        while data.count <= 2 * 1_024 * 1_024 {
            let count = Darwin.read(descriptor, &buffer, buffer.count)
            guard count > 0 else { return nil }
            data.append(contentsOf: buffer.prefix(count))
            guard let headerEnd = data.range(of: Data("\r\n\r\n".utf8)) else { continue }
            let header = String(decoding: data[..<headerEnd.lowerBound], as: UTF8.self)
            let contentLength = header.split(separator: "\n").first { line in
                line.lowercased().hasPrefix("content-length:")
            }.flatMap { Int($0.split(separator: ":", maxSplits: 1)[1]
                .trimmingCharacters(in: .whitespacesAndNewlines)) } ?? 0
            if data.count >= headerEnd.upperBound + contentLength { return data }
        }
        return nil
    }

    private static func writeAll(_ data: Data, descriptor: Int32) {
        data.withUnsafeBytes { rawBuffer in
            guard let baseAddress = rawBuffer.baseAddress else { return }
            var offset = 0
            while offset < rawBuffer.count {
                let count = Darwin.write(
                    descriptor,
                    baseAddress.advanced(by: offset),
                    rawBuffer.count - offset
                )
                if count > 0 { offset += count }
                else if count < 0, errno == EINTR { continue }
                else { return }
            }
        }
    }
}
