// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Testing

@Suite("Local Agent Task state store")
struct LocalAgentTaskStateStoreTests {
    @Test("restores Task and Run identity then reduces durable UI events idempotently")
    func restoresAndReducesEvents() async throws {
        let store = LocalAgentTaskStateStore()
        let task = taskSnapshot()
        var run = runSnapshot()
        try await store.restoreLocalAgentTasks([task], runs: [run])

        let modelEvent = LocalAgentUIEvent(
            eventSeq: 11,
            emittedAt: "2026-09-12T03:01:00Z",
            event: .modelStream(LocalAgentModelStreamEvent(
                runID: run.runID,
                stepSeq: 2,
                deltaKind: .reasoning,
                delta: "Inspect visual hierarchy"
            ))
        )
        try await store.applyLocalAgentTaskEvent(modelEvent)
        try await store.applyLocalAgentTaskEvent(modelEvent)

        let tool = LocalAgentToolSnapshot(
            invocationID: "invocation-1",
            runID: run.runID,
            batchID: "batch-1",
            toolCallID: "call-1",
            toolName: "render_page",
            effect: .read,
            argumentsDigest: "sha256:" + String(repeating: "a", count: 64),
            status: .started
        )
        try await store.applyLocalAgentTaskEvent(LocalAgentUIEvent(
            eventSeq: 12,
            emittedAt: "2026-09-12T03:02:00Z",
            event: .toolSnapshot(tool)
        ))
        run.status = .succeeded
        run.stepSeq = 3
        run.terminalOutcome = .object([
            "text": .string("Visual design completed"),
            "evidence": .array([.string("render.png")]),
        ])
        try await store.applyLocalAgentTaskEvent(LocalAgentUIEvent(
            eventSeq: 13,
            emittedAt: "2026-09-12T03:03:00Z",
            event: .runSnapshot(run)
        ))

        let state = try #require(await store.localAgentTask(taskID: task.taskID))
        #expect(state.run.status == .succeeded)
        #expect(state.run.projectID == "project-1")
        #expect(state.modelSteps.count == 1)
        #expect(state.modelSteps[0].reasoning == "Inspect visual hierarchy")
        #expect(state.tools == [tool])
        #expect(state.lastAppliedEventSequence == 13)
        #expect(await store.localAgentTasks(sessionID: "thread-1").map(\.id) == ["task-1"])
    }

    @Test("rejects a Task whose frozen project identity differs from its Run")
    func rejectsProjectDrift() async {
        let store = LocalAgentTaskStateStore()
        var task = taskSnapshot()
        task.projectID = "project-2"

        await #expect(throws: LocalAgentTaskStateError.self) {
            try await store.restoreLocalAgentTasks([task], runs: [runSnapshot()])
        }
    }
}

private func taskSnapshot() -> LocalAgentTaskSnapshot {
    LocalAgentTaskSnapshot(
        taskID: "task-1",
        revision: 1,
        sourceThreadID: "thread-1",
        sourceTurnID: "turn-1",
        projectID: "project-1",
        runID: "task-run-1",
        objective: "Implement the approved visual design",
        acceptanceCriteria: ["Match the approved reference", "Pass visual QA"],
        status: "running",
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        createdAt: "2026-09-12T03:00:00Z",
        updatedAt: "2026-09-12T03:00:00Z"
    )
}

private func runSnapshot() -> LocalAgentRunSnapshot {
    LocalAgentRunSnapshot(
        runID: "task-run-1",
        profileKey: "task_runner",
        ownerUserID: "user-1",
        ownerEntityType: "task",
        ownerEntityID: "task-1",
        projectID: "project-1",
        status: .modelRunning,
        version: 2,
        stepSeq: 2,
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
