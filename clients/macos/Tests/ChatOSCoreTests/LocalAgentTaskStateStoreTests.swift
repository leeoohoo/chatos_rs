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

    @Test("restores a retried Task from its current Run and ignores historical Run events")
    func restoresRetriedTask() async throws {
        let store = LocalAgentTaskStateStore()
        var task = taskSnapshot()
        task.revision = 2
        task.currentRunID = "task-run-2"
        task.runIDs = ["task-run-1", "task-run-2"]
        var historicalRun = runSnapshot()
        historicalRun.status = .failed
        var currentRun = runSnapshot()
        currentRun.runID = "task-run-2"
        currentRun.status = .modelRunning

        try await store.restoreLocalAgentTasks([task], runs: [historicalRun, currentRun])
        try await store.applyLocalAgentTaskEvent(LocalAgentUIEvent(
            eventSeq: 20,
            emittedAt: "2026-09-12T03:04:00Z",
            event: .runSnapshot(historicalRun)
        ))

        let restored = try #require(await store.localAgentTask(taskID: task.taskID))
        #expect(restored.run.runID == "task-run-2")
        #expect(restored.run.status == .modelRunning)
        #expect(restored.lastAppliedEventSequence == 0)
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

    @Test("restores a pending Task Runner question from its authoritative Run snapshot")
    func restoresPendingQuestion() async throws {
        let store = LocalAgentTaskStateStore()
        let task = taskSnapshot()
        var run = runSnapshot()
        run.status = .paused
        run.pendingInteraction = .object([
            "type": .string("ask_user"),
            "interaction_id": .string("restored-interaction"),
            "question": .object([
                "prompt": .string("Choose a restored visual direction"),
                "options": .array([
                    .object([
                        "option_id": .string("editorial"),
                        "label": .string("Editorial"),
                        "description": .string("Typography-led"),
                    ]),
                ]),
                "image_references": .array([.string("reference://restored-preview")]),
                "details": .object(["title": .string("Visual direction")]),
            ]),
        ])

        try await store.restoreLocalAgentTasks([task], runs: [run])

        let prompt = try #require(await store.localAgentPrompts(
            sessionID: task.sourceThreadID,
            limit: 100
        ).first)
        #expect(prompt.id == "restored-interaction")
        #expect(prompt.message == "Choose a restored visual direction")
        #expect(prompt.choice?.options.first?.description == "Typography-led")
        #expect(try await store.localAgentPromptRoute(
            promptID: prompt.id,
            sessionID: task.sourceThreadID
        ) == LocalAgentAskUserRoute(
            runID: run.runID,
            interactionID: "restored-interaction"
        ))
        #expect(await store.localAgentRunControls(
            sessionID: task.sourceThreadID
        ).first?.requiresUserAnswer == true)
    }

    @Test("binds Task prompts controls and approvals to the exact Run and conversation")
    func bindsNativeInteractions() async throws {
        let store = LocalAgentTaskStateStore()
        let task = taskSnapshot()
        var run = runSnapshot()
        try await store.restoreLocalAgentTasks([task], runs: [run])

        let interaction = LocalAgentUserInteractionEvent(
            interactionID: "task-interaction-1",
            runID: run.runID,
            prompt: "Which visual direction should the Task continue?",
            options: [
                .init(optionID: "editorial", label: "Editorial"),
                .init(optionID: "spatial", label: "Spatial"),
            ],
            imageReferences: ["reference://task-visual"],
            details: .object([
                "title": .string("Choose the visual direction"),
                "allows_multiple": .bool(false),
            ])
        )
        try await store.applyLocalAgentTaskEvent(LocalAgentUIEvent(
            eventSeq: 1,
            emittedAt: "2026-09-12T03:01:00Z",
            event: .userInteraction(interaction)
        ))
        let tool = LocalAgentToolSnapshot(
            invocationID: "task-invocation-1",
            runID: run.runID,
            batchID: "task-batch-1",
            toolCallID: "task-call-1",
            toolName: "save_design",
            effect: .write,
            argumentsDigest: "sha256:" + String(repeating: "b", count: 64),
            status: .awaitingApproval
        )
        try await store.applyLocalAgentTaskEvent(LocalAgentUIEvent(
            eventSeq: 2,
            emittedAt: "2026-09-12T03:02:00Z",
            event: .toolSnapshot(tool)
        ))

        let prompt = try #require(await store.localAgentPrompts(
            sessionID: "thread-1",
            limit: 100
        ).first)
        #expect(prompt.id == interaction.interactionID)
        #expect(prompt.turnID == task.sourceTurnID)
        #expect(prompt.choice?.options.map(\.value) == ["editorial", "spatial"])
        #expect(try await store.localAgentPromptRoute(
            promptID: prompt.id,
            sessionID: "thread-1"
        ) == LocalAgentAskUserRoute(
            runID: run.runID,
            interactionID: interaction.interactionID
        ))
        #expect(await store.localAgentRunControls(sessionID: "thread-1").map(\.runID) == [
            run.runID,
        ])
        #expect(await store.localAgentPendingToolApprovals(
            sessionID: "thread-1"
        ).map(\.invocationID) == [tool.invocationID])

        await #expect(throws: LocalAgentConversationHistoryError.promptUnavailable) {
            _ = try await store.localAgentPromptRoute(
                promptID: prompt.id,
                sessionID: "thread-other"
            )
        }
        await #expect(throws: LocalAgentConversationHistoryError.runUnavailable) {
            _ = try await store.requireLocalAgentRunControl(
                runID: run.runID,
                sessionID: "thread-other"
            )
        }
        await #expect(throws: LocalAgentConversationHistoryError.toolApprovalUnavailable) {
            _ = try await store.requireLocalAgentToolApproval(
                invocationID: tool.invocationID,
                sessionID: "thread-other"
            )
        }

        let answered = try await store.updateLocalAgentPromptStatus(
            promptID: prompt.id,
            sessionID: "thread-1",
            status: .ok
        )
        #expect(answered.status == .ok)
        await #expect(throws: LocalAgentConversationHistoryError.promptUnavailable) {
            _ = try await store.localAgentPromptRoute(
                promptID: prompt.id,
                sessionID: "thread-1"
            )
        }

        run.status = .succeeded
        run.updatedAt = "2026-09-12T03:03:00Z"
        try await store.applyLocalAgentTaskEvent(LocalAgentUIEvent(
            eventSeq: 3,
            emittedAt: run.updatedAt,
            event: .runSnapshot(run)
        ))
        #expect(await store.localAgentRunControls(sessionID: "thread-1").isEmpty)
        #expect(await store.localAgentPendingToolApprovals(sessionID: "thread-1").isEmpty)
        await #expect(throws: LocalAgentConversationHistoryError.toolApprovalUnavailable) {
            _ = try await store.requireLocalAgentToolApproval(
                invocationID: tool.invocationID,
                sessionID: "thread-1"
            )
        }
    }

    @Test("publishes one global update for native Pet projection")
    func publishesGlobalUpdates() async throws {
        let store = LocalAgentTaskStateStore()
        let stream = await store.localAgentTaskUpdates()
        var iterator = stream.makeAsyncIterator()

        try await store.registerLocalAgentTask(taskSnapshot(), run: runSnapshot())

        #expect(await iterator.next() != nil)
    }
}

private func taskSnapshot() -> LocalAgentTaskSnapshot {
    LocalAgentTaskSnapshot(
        taskID: "task-1",
        revision: 1,
        sourceThreadID: "thread-1",
        sourceTurnID: "turn-1",
        projectID: "project-1",
        initialRunID: "task-run-1",
        currentRunID: "task-run-1",
        runIDs: ["task-run-1"],
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
