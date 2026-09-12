// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Testing

@Suite("Native Local Agent Task event sink")
struct NativeLocalAgentTaskEventSinkTests {
    @Test("restores all Task Runner state before consuming the persisted UI cursor")
    func restoresAuthoritativeState() async throws {
        let task = taskSnapshot(id: "task-1", runID: "run-1")
        let run = runSnapshot(id: "run-1", taskID: "task-1")
        let client = TaskStateClient(tasks: [task], runs: [run])
        let store = LocalAgentTaskStateStore()
        let sink = NativeLocalAgentTaskEventSink(clientProvider: { client }, store: store)

        try await sink.restore()

        let restored = try #require(await store.localAgentTask(taskID: "task-1"))
        #expect(restored.task.objective == "Design task-1")
        #expect(restored.run.runID == "run-1")
        #expect(restored.run.projectID == "project-1")

        try await sink.applyLocalAgentUIEvent(
            LocalAgentUIEvent(
                eventSeq: 9,
                emittedAt: "2026-09-12T03:00:01Z",
                event: .memorySync(LocalAgentMemorySyncStatus(
                    runID: run.runID,
                    pendingCount: 2,
                    failedCount: 1,
                    lastErrorCode: "memory_sync_failed"
                ))
            ),
            mainChatBinding: nil
        )
        let updated = try #require(await store.localAgentTask(taskID: "task-1"))
        #expect(updated.memorySync?.runID == run.runID)
        #expect(updated.memorySync?.pendingCount == 2)
        #expect(updated.memorySync?.failedCount == 1)
    }

    @Test("discovers a newly created Task before reducing its first event")
    func discoversNewTask() async throws {
        let task = taskSnapshot(id: "task-2", runID: "run-2")
        let run = runSnapshot(id: "run-2", taskID: "task-2")
        let initialClient = TaskStateClient(tasks: [], runs: [])
        let replacementClient = TaskStateClient(tasks: [task], runs: [run])
        let provider = TaskStateClientProvider(client: initialClient)
        let store = LocalAgentTaskStateStore()
        let sink = NativeLocalAgentTaskEventSink(
            clientProvider: { await provider.client() },
            store: store
        )

        try await sink.restore()
        await provider.install(replacementClient)
        try await sink.applyLocalAgentUIEvent(
            LocalAgentUIEvent(
                eventSeq: 7,
                emittedAt: "2026-09-12T03:00:00Z",
                event: .runSnapshot(run)
            ),
            mainChatBinding: nil
        )

        let restored = try #require(await store.localAgentTask(taskID: "task-2"))
        #expect(restored.lastAppliedEventSequence == 7)
        #expect(restored.run.runID == "run-2")
        #expect(await provider.requestCount() == 2)
    }

    @Test("switches an existing Task to the new current Run after retry")
    func discoversRetryRun() async throws {
        var task = taskSnapshot(id: "task-1", runID: "run-1")
        var oldRun = runSnapshot(id: "run-1", taskID: "task-1")
        oldRun.status = .failed
        let client = TaskStateClient(tasks: [task], runs: [oldRun])
        let store = LocalAgentTaskStateStore()
        let sink = NativeLocalAgentTaskEventSink(clientProvider: { client }, store: store)
        try await sink.restore()

        let retryRun = runSnapshot(id: "run-2", taskID: "task-1")
        task.revision = 2
        task.currentRunID = retryRun.runID
        task.runIDs = [oldRun.runID, retryRun.runID]
        await client.install(task: task, run: retryRun)

        try await sink.applyLocalAgentUIEvent(
            LocalAgentUIEvent(
                eventSeq: 8,
                emittedAt: "2026-09-12T03:02:00Z",
                event: .runSnapshot(retryRun)
            ),
            mainChatBinding: nil
        )

        let restored = try #require(await store.localAgentTask(taskID: task.taskID))
        #expect(restored.task.initialRunID == "run-1")
        #expect(restored.task.runIDs == ["run-1", "run-2"])
        #expect(restored.run.runID == "run-2")
        #expect(restored.lastAppliedEventSequence == 8)
    }
}

private actor TaskStateClientProvider {
    private var current: TaskStateClient
    private var requests = 0

    init(client: TaskStateClient) {
        current = client
    }

    func client() -> any NativeLocalAgentTaskStateClient {
        requests += 1
        return current
    }

    func install(_ client: TaskStateClient) {
        current = client
    }

    func requestCount() -> Int { requests }
}

private actor TaskStateClient: NativeLocalAgentTaskStateClient {
    private var tasksByID: [String: LocalAgentTaskSnapshot]
    private var runsByID: [String: LocalAgentRunSnapshot]

    init(tasks: [LocalAgentTaskSnapshot], runs: [LocalAgentRunSnapshot]) {
        tasksByID = Dictionary(uniqueKeysWithValues: tasks.map { ($0.taskID, $0) })
        runsByID = Dictionary(uniqueKeysWithValues: runs.map { ($0.runID, $0) })
    }

    func install(task: LocalAgentTaskSnapshot, run: LocalAgentRunSnapshot) {
        tasksByID[task.taskID] = task
        runsByID[run.runID] = run
    }

    func run(id: String) async throws -> LocalAgentRunSnapshot {
        try #require(runsByID[id])
    }

    func task(id: String) async throws -> LocalAgentTaskSnapshot {
        try #require(tasksByID[id])
    }

    func runs(cursor: String?, limit: UInt32) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    ) {
        (runsByID.values.sorted { $0.runID < $1.runID }, nil)
    }

    func tasks(cursor: String?, limit: UInt32) async throws -> (
        tasks: [LocalAgentTaskSnapshot], nextCursor: String?
    ) {
        (tasksByID.values.sorted { $0.taskID < $1.taskID }, nil)
    }
}

private func taskSnapshot(id: String, runID: String) -> LocalAgentTaskSnapshot {
    LocalAgentTaskSnapshot(
        taskID: id,
        revision: 1,
        sourceThreadID: "thread-1",
        sourceTurnID: "turn-1",
        projectID: "project-1",
        initialRunID: runID,
        currentRunID: runID,
        runIDs: [runID],
        objective: "Design \(id)",
        acceptanceCriteria: ["Match the approved visual"],
        status: "running",
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        createdAt: "2026-09-12T03:00:00Z",
        updatedAt: "2026-09-12T03:00:00Z"
    )
}

private func runSnapshot(id: String, taskID: String) -> LocalAgentRunSnapshot {
    LocalAgentRunSnapshot(
        runID: id,
        profileKey: "task_runner",
        ownerUserID: "user-1",
        ownerEntityType: "task",
        ownerEntityID: taskID,
        projectID: "project-1",
        status: .modelRunning,
        version: 1,
        stepSeq: 1,
        iteration: 1,
        retryCount: 0,
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        modelRuntimeSnapshot: .object([:]),
        contextStrategy: "provider_native",
        promptRevision: "prompt-1",
        capabilitySnapshotRef: "capabilities-1",
        createdAt: "2026-09-12T03:00:00Z",
        updatedAt: "2026-09-12T03:00:00Z"
    )
}
