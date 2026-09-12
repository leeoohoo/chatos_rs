// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation
import Testing
@testable import ChatOSCore

@Suite("Conversation history Local Agent reducer")
struct ConversationHistoryStoreLocalAgentTests {
    @Test("reconstructs a Main Chat turn and reduces typed events idempotently")
    func reducesMainChatEvents() async throws {
        let store = ConversationHistoryStore()
        let binding = mainChatBinding()

        try await store.applyLocalAgentUIEvent(
            uiEvent(1, .runSnapshot(run(status: .modelRunning))),
            mainChatBinding: binding
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(2, .modelStream(stream(.content, "Hel"))),
            mainChatBinding: binding
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(3, .modelStream(stream(.content, "lo"))),
            mainChatBinding: binding
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(3, .modelStream(stream(.content, "lo"))),
            mainChatBinding: binding
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(4, .modelStream(stream(.reasoning, "private reasoning"))),
            mainChatBinding: binding
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(5, .toolSnapshot(LocalAgentToolSnapshot(
                invocationID: "invocation-1",
                runID: "run-1",
                batchID: "batch-1",
                toolCallID: "call-1",
                toolName: "inspect_design",
                effect: .read,
                argumentsDigest: "sha256:args",
                status: .succeeded,
                boundedResult: .object(["ok": .bool(true)])
            ))),
            mainChatBinding: binding
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(6, .memorySync(LocalAgentMemorySyncStatus(
                runID: "run-1",
                pendingCount: 0,
                failedCount: 1,
                lastErrorCode: "memory_sync_failed"
            ))),
            mainChatBinding: binding
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(7, .runSnapshot(run(
                status: .succeeded,
                terminalOutcome: .object(["text": .string("Hello")])
            ))),
            mainChatBinding: binding
        )

        let snapshot = await store.snapshot(sessionID: "thread-1")
        let turn = try #require(snapshot.turns.first)
        #expect(snapshot.turns.count == 1)
        #expect(turn.id == "turn-1")
        #expect(turn.userMessage.id == "message-1")
        #expect(turn.userMessage.text == "Design it")
        #expect(turn.finalAssistantMessage?.text == "Hello")
        #expect(turn.finalAssistantMessage?.text.contains("private reasoning") == false)
        #expect(turn.processEvents.contains(where: {
            $0.id == "local-agent-reasoning-run-1-1"
                && $0.detail == "private reasoning"
        }))
        #expect(turn.processEvents.contains(where: {
            $0.id == "local-agent-tool-invocation-1" && $0.status == .completed
        }))
        let memoryEvent = turn.processEvents.first(where: {
            $0.id == "local-agent-memory-run-1"
        })
        #expect(memoryEvent?.status == .failed)
        #expect(memoryEvent?.detail?.contains("失败 1 条") == true)
        #expect(turn.status == .completed)
        #expect(turn.revision == 7)
        #expect(turn.completedAt != nil)
    }

    @Test("rejects a successful Run without a final text atomically")
    func rejectsEmptySuccess() async throws {
        let store = ConversationHistoryStore()
        let binding = mainChatBinding()

        await #expect(throws: LocalAgentConversationHistoryError.missingSuccessfulOutcome) {
            try await store.applyLocalAgentUIEvent(
                uiEvent(1, .runSnapshot(run(
                    status: .succeeded,
                    terminalOutcome: .object(["reason": .string("bad-shape")])
                ))),
                mainChatBinding: binding
            )
        }

        #expect(await store.snapshot(sessionID: "thread-1").turns.isEmpty)
    }

    @Test("tracks only actionable Run controls and tool approvals")
    func tracksRunControlsAndApprovals() async throws {
        let store = ConversationHistoryStore()
        let binding = mainChatBinding()
        try await store.applyLocalAgentUIEvent(
            uiEvent(1, .runSnapshot(run(
                status: .paused,
                pendingInteraction: .object(["type": .string("ask_user")])
            ))),
            mainChatBinding: binding
        )
        var control = try #require(await store.localAgentRunControls(
            sessionID: "thread-1"
        ).first)
        #expect(control.requiresUserAnswer)
        #expect(!control.canResume)
        #expect(control.canCancel)

        let pendingTool = LocalAgentToolSnapshot(
            invocationID: "invocation-write-1",
            runID: "run-1",
            batchID: "batch-1",
            toolCallID: "call-1",
            toolName: "save_design",
            effect: .write,
            argumentsDigest: "sha256:write-args",
            status: .awaitingApproval
        )
        try await store.applyLocalAgentUIEvent(
            uiEvent(2, .toolSnapshot(pendingTool)),
            mainChatBinding: binding
        )
        #expect(await store.localAgentPendingToolApprovals(
            sessionID: "thread-1"
        ).map(\.invocationID) == ["invocation-write-1"])

        var approvedTool = pendingTool
        approvedTool.status = .approved
        try await store.applyLocalAgentUIEvent(
            uiEvent(3, .toolSnapshot(approvedTool)),
            mainChatBinding: binding
        )
        #expect(await store.localAgentPendingToolApprovals(
            sessionID: "thread-1"
        ).isEmpty)

        try await store.applyLocalAgentUIEvent(
            uiEvent(4, .runSnapshot(run(
                status: .needsReview,
                pendingInteraction: .object([
                    "type": .string("review_unknown_tool_outcome"),
                    "batch_id": .string("batch-1"),
                ])
            ))),
            mainChatBinding: binding
        )
        control = try #require(await store.localAgentRunControls(
            sessionID: "thread-1"
        ).first)
        #expect(control.canResume)
        #expect(!control.canPause)
        #expect(control.reviewReason?.contains("batch-1") == true)
    }
}

private let localTimestamp = "2026-09-12T05:00:00Z"

private func mainChatBinding() -> LocalAgentMainChatRunBinding {
    LocalAgentMainChatRunBinding(
        runID: "run-1",
        threadID: "thread-1",
        turnID: "turn-1",
        messageID: "message-1",
        userMessage: LocalAgentStoredMessage(
            recordID: "message-1",
            runID: "run-1",
            threadID: "thread-1",
            turnID: "turn-1",
            sequence: 1,
            role: .user,
            content: "Design it",
            messageMode: .semantic,
            messageSource: "main_chat",
            memorySyncStatus: .pending,
            createdAt: localTimestamp
        )
    )
}

private func uiEvent(
    _ sequence: UInt64,
    _ payload: LocalAgentUIEventPayload
) -> LocalAgentUIEvent {
    LocalAgentUIEvent(eventSeq: sequence, emittedAt: localTimestamp, event: payload)
}

private func stream(
    _ kind: LocalAgentModelStreamDeltaKind,
    _ delta: String
) -> LocalAgentModelStreamEvent {
    LocalAgentModelStreamEvent(
        runID: "run-1",
        stepSeq: 1,
        deltaKind: kind,
        delta: delta
    )
}

private func run(
    status: LocalAgentRunStatus,
    pendingInteraction: LocalAgentJSONValue? = nil,
    terminalOutcome: LocalAgentJSONValue? = nil
) -> LocalAgentRunSnapshot {
    LocalAgentRunSnapshot(
        runID: "run-1",
        profileKey: "main_chat",
        ownerUserID: "user-1",
        ownerEntityType: "conversation",
        ownerEntityID: "thread-1",
        projectID: "project-1",
        status: status,
        version: status == .succeeded ? 4 : 2,
        stepSeq: 1,
        iteration: 0,
        retryCount: 0,
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        modelRuntimeSnapshot: .object([:]),
        contextStrategy: "provider_native",
        promptRevision: "prompt-1",
        capabilitySnapshotRef: "capabilities-1",
        pendingInteraction: pendingInteraction,
        terminalOutcome: terminalOutcome,
        createdAt: localTimestamp,
        updatedAt: localTimestamp
    )
}
