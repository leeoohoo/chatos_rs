// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Testing

@Suite("Unified Local Agent interaction state")
struct LocalAgentUnifiedInteractionStateTests {
    @Test("merges Main Chat and Task Runner interactions without losing exact routes")
    func mergesBothProfiles() async throws {
        let mainChat = ConversationHistoryStore()
        let taskRunner = LocalAgentTaskStateStore()
        let state = LocalAgentUnifiedInteractionState(
            mainChat: mainChat,
            taskRunner: taskRunner
        )
        let binding = unifiedMainChatBinding()
        let mainRun = unifiedRunSnapshot(
            runID: binding.runID,
            profileKey: "main_chat",
            ownerEntityType: "turn",
            ownerEntityID: binding.turnID,
            projectID: "project-1"
        )
        try await mainChat.applyLocalAgentUIEvent(
            .init(eventSeq: 1, emittedAt: unifiedTimestamp, event: .runSnapshot(mainRun)),
            mainChatBinding: binding
        )
        try await mainChat.applyLocalAgentUIEvent(
            .init(
                eventSeq: 2,
                emittedAt: unifiedTimestamp,
                event: .userInteraction(unifiedInteraction(
                    id: "main-interaction",
                    runID: mainRun.runID,
                    prompt: "Choose the Main Chat direction"
                ))
            ),
            mainChatBinding: binding
        )
        try await mainChat.applyLocalAgentUIEvent(
            .init(
                eventSeq: 3,
                emittedAt: unifiedTimestamp,
                event: .toolSnapshot(unifiedTool(
                    id: "main-invocation",
                    runID: mainRun.runID
                ))
            ),
            mainChatBinding: binding
        )

        let task = unifiedTaskSnapshot()
        let taskRun = unifiedRunSnapshot(
            runID: task.currentRunID,
            profileKey: "task_runner",
            ownerEntityType: "task",
            ownerEntityID: task.taskID,
            projectID: task.projectID
        )
        try await taskRunner.restoreLocalAgentTasks([task], runs: [taskRun])
        try await taskRunner.applyLocalAgentTaskEvent(.init(
            eventSeq: 4,
            emittedAt: unifiedTimestamp,
            event: .userInteraction(unifiedInteraction(
                id: "task-interaction",
                runID: taskRun.runID,
                prompt: "Choose the Task direction"
            ))
        ))
        try await taskRunner.applyLocalAgentTaskEvent(.init(
            eventSeq: 5,
            emittedAt: unifiedTimestamp,
            event: .toolSnapshot(unifiedTool(
                id: "task-invocation",
                runID: taskRun.runID
            ))
        ))

        let prompts = try await state.localAgentPrompts(sessionID: "thread-1", limit: 100)
        #expect(Set(prompts.map(\.id)) == ["main-interaction", "task-interaction"])
        #expect(try await state.localAgentPromptRoute(
            promptID: "main-interaction",
            sessionID: "thread-1"
        ) == LocalAgentAskUserRoute(
            runID: mainRun.runID,
            interactionID: "main-interaction"
        ))
        #expect(try await state.localAgentPromptRoute(
            promptID: "task-interaction",
            sessionID: "thread-1"
        ) == LocalAgentAskUserRoute(
            runID: taskRun.runID,
            interactionID: "task-interaction"
        ))
        #expect(Set(await state.localAgentRunControls(
            sessionID: "thread-1"
        ).map(\.runID)) == [mainRun.runID, taskRun.runID])
        #expect(Set(await state.localAgentPendingToolApprovals(
            sessionID: "thread-1"
        ).map(\.invocationID)) == ["main-invocation", "task-invocation"])

        let updated = try await state.updateLocalAgentPromptStatus(
            promptID: "task-interaction",
            sessionID: "thread-1",
            status: .ok
        )
        #expect(updated.status == .ok)
        #expect(try await mainChat.localAgentPrompts(
            sessionID: "thread-1",
            limit: 100
        ).first?.status == .pending)
        #expect(await taskRunner.localAgentPrompts(
            sessionID: "thread-1",
            limit: 100
        ).first?.status == .ok)

        #expect(try await state.localAgentPrompts(
            sessionID: "thread-other",
            limit: 100
        ).isEmpty)
        #expect(await state.localAgentRunControls(sessionID: "thread-other").isEmpty)
        #expect(await state.localAgentPendingToolApprovals(
            sessionID: "thread-other"
        ).isEmpty)
    }

    @Test("deduplicates an interaction projected by both profile stores")
    func deduplicatesProjection() async throws {
        let first = UnifiedInteractionStub(promptID: "shared", runID: "shared-run")
        let second = UnifiedInteractionStub(promptID: "shared", runID: "shared-run")
        let state = LocalAgentUnifiedInteractionState(mainChat: first, taskRunner: second)

        #expect(try await state.localAgentPrompts(
            sessionID: "thread-1",
            limit: 100
        ).map(\.id) == ["shared"])
        #expect(await state.localAgentRunControls(
            sessionID: "thread-1"
        ).map(\.runID) == ["shared-run"])
        #expect(await state.localAgentPendingToolApprovals(
            sessionID: "thread-1"
        ).map(\.invocationID) == ["shared-invocation"])
    }
}

private let unifiedTimestamp = "2026-09-12T08:00:00Z"

private func unifiedMainChatBinding() -> LocalAgentMainChatRunBinding {
    LocalAgentMainChatRunBinding(
        runID: "main-run",
        threadID: "thread-1",
        turnID: "main-turn",
        messageID: "main-message",
        userMessage: LocalAgentStoredMessage(
            recordID: "main-message",
            runID: "main-run",
            threadID: "thread-1",
            turnID: "main-turn",
            sequence: 1,
            role: .user,
            content: "Design the page",
            messageMode: .semantic,
            messageSource: "main_chat",
            memorySyncStatus: .pending,
            createdAt: unifiedTimestamp
        )
    )
}

private func unifiedTaskSnapshot() -> LocalAgentTaskSnapshot {
    LocalAgentTaskSnapshot(
        taskID: "task-1",
        revision: 1,
        sourceThreadID: "thread-1",
        sourceTurnID: "task-turn",
        projectID: "project-1",
        initialRunID: "task-run",
        currentRunID: "task-run",
        runIDs: ["task-run"],
        objective: "Complete the approved visual direction",
        acceptanceCriteria: ["Pass visual review"],
        status: "running",
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        createdAt: unifiedTimestamp,
        updatedAt: unifiedTimestamp
    )
}

private func unifiedRunSnapshot(
    runID: String,
    profileKey: String,
    ownerEntityType: String,
    ownerEntityID: String,
    projectID: String
) -> LocalAgentRunSnapshot {
    LocalAgentRunSnapshot(
        runID: runID,
        profileKey: profileKey,
        ownerUserID: "user-1",
        ownerEntityType: ownerEntityType,
        ownerEntityID: ownerEntityID,
        projectID: projectID,
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
        createdAt: unifiedTimestamp,
        updatedAt: unifiedTimestamp
    )
}

private func unifiedInteraction(
    id: String,
    runID: String,
    prompt: String
) -> LocalAgentUserInteractionEvent {
    LocalAgentUserInteractionEvent(
        interactionID: id,
        runID: runID,
        prompt: prompt,
        options: [.init(optionID: "continue", label: "Continue")],
        imageReferences: ["reference://visual"],
        details: .object(["title": .string("Visual decision")])
    )
}

private func unifiedTool(id: String, runID: String) -> LocalAgentToolSnapshot {
    LocalAgentToolSnapshot(
        invocationID: id,
        runID: runID,
        batchID: "batch-\(id)",
        toolCallID: "call-\(id)",
        toolName: "save_design",
        effect: .write,
        argumentsDigest: "sha256:" + String(repeating: "c", count: 64),
        status: .awaitingApproval
    )
}

private actor UnifiedInteractionStub:
    LocalAgentAskUserStateStoring,
    LocalAgentRunControlStateStoring
{
    let prompt: AskUserPrompt
    let control: LocalAgentRunControlState
    let approval: LocalAgentToolApprovalRequest

    init(promptID: String, runID: String) {
        prompt = AskUserPrompt(
            id: promptID,
            sessionID: "thread-1",
            turnID: "turn-1",
            kind: "local_agent",
            status: .pending,
            title: "Question",
            message: "Continue?",
            allowsCancel: true
        )
        control = LocalAgentRunControlState(
            runID: runID,
            sessionID: "thread-1",
            turnID: "turn-1",
            status: .modelRunning,
            iteration: 1,
            retryCount: 0
        )
        approval = LocalAgentToolApprovalRequest(
            invocationID: "shared-invocation",
            runID: runID,
            sessionID: "thread-1",
            turnID: "turn-1",
            toolName: "save_design",
            effect: .write,
            argumentsDigest: "sha256:shared"
        )
    }

    func localAgentPrompts(sessionID: String, limit: Int) -> [AskUserPrompt] {
        sessionID == prompt.sessionID && limit > 0 ? [prompt] : []
    }

    func localAgentPromptRoute(
        promptID: String,
        sessionID: String
    ) throws -> LocalAgentAskUserRoute {
        guard promptID == prompt.id, sessionID == prompt.sessionID else {
            throw LocalAgentConversationHistoryError.promptUnavailable
        }
        return LocalAgentAskUserRoute(runID: control.runID, interactionID: prompt.id)
    }

    func updateLocalAgentPromptStatus(
        promptID: String,
        sessionID: String,
        status: AskUserPromptStatus
    ) throws -> AskUserPrompt {
        guard promptID == prompt.id, sessionID == prompt.sessionID else {
            throw LocalAgentConversationHistoryError.promptUnavailable
        }
        var result = prompt
        result.status = status
        return result
    }

    func localAgentRunControls(sessionID: String) -> [LocalAgentRunControlState] {
        sessionID == control.sessionID ? [control] : []
    }

    func localAgentPendingToolApprovals(
        sessionID: String
    ) -> [LocalAgentToolApprovalRequest] {
        sessionID == approval.sessionID ? [approval] : []
    }

    func requireLocalAgentRunControl(
        runID: String,
        sessionID: String
    ) throws -> LocalAgentRunControlState {
        guard runID == control.runID, sessionID == control.sessionID else {
            throw LocalAgentConversationHistoryError.runUnavailable
        }
        return control
    }

    func requireLocalAgentToolApproval(
        invocationID: String,
        sessionID: String
    ) throws -> LocalAgentToolApprovalRequest {
        guard invocationID == approval.invocationID, sessionID == approval.sessionID else {
            throw LocalAgentConversationHistoryError.toolApprovalUnavailable
        }
        return approval
    }
}
