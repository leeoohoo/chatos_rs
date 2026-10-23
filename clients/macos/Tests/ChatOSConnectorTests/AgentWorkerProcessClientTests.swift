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

    private static func runIsolatedWorker(
        client: AgentWorkerProcessClient,
        scope: String,
        model: any AgentModelClient
    ) async throws -> AgentRunCheckpoint {
        var checkpoint = AgentRunCheckpoint(
            scope: scope,
            messages: [.init(role: .user, content: scope)]
        )
        checkpoint.id = UUID()
        var policy = AgentRunPolicy()
        policy.maximumModelCalls = 2
        policy.maximumRequestRetries = 0
        policy.maximumNoProgressRounds = 2
        policy.requestTimeoutSeconds = 10
        policy.runTimeoutSeconds = 30
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
            execute: { _ in .init("completed:\(scope)") },
            completionCheck: { nil },
            contextProvider: nil,
            record: { _, _ in }
        )
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
