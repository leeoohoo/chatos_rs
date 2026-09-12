// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native Local Agent Main Chat restart recovery")
struct NativeLocalAgentMainChatRestorerTests {
    @Test("rebuilds the exact Turn, final result and process without replaying the UI cursor")
    func restoresAuthoritativeTurn() async throws {
        let run = mainChatRecoveryRun()
        let client = MainChatRecoveryClient(run: run)
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns([
            ConversationTurn(
                id: "turn-1",
                sessionID: "thread-1",
                sequence: 1,
                revision: 99,
                userMessage: ChatMessage(
                    id: "message-1",
                    role: .user,
                    text: "stale remote text",
                    createdAt: .distantPast
                ),
                finalAssistantMessage: ChatMessage(
                    id: "remote-assistant",
                    role: .assistant,
                    text: "stale remote result",
                    createdAt: .distantPast
                ),
                status: .completed,
                startedAt: .distantPast
            ),
        ], sessionID: "thread-1")
        let restorer = NativeLocalAgentMainChatRestorer(client: client, store: store)

        try await restorer.restore()
        try await restorer.restore()
        try await store.applyLocalAgentUIEvent(
            LocalAgentUIEvent(
                eventSeq: 20,
                emittedAt: "2026-09-13T01:00:02Z",
                event: .modelStream(LocalAgentModelStreamEvent(
                    runID: "run-main-1",
                    stepSeq: 1,
                    deltaKind: .content,
                    delta: "duplicate snapshot text"
                ))
            ),
            mainChatBinding: try await client.mainChatRunBinding(runID: "run-main-1")
        )
        await store.mergePage(
            HistoryPage(
                turns: [ConversationTurn(
                    id: "turn-1",
                    sessionID: "thread-1",
                    sequence: 1,
                    revision: 1_000,
                    userMessage: ChatMessage(
                        id: "message-1",
                        role: .user,
                        text: "remote overwrite",
                        createdAt: .distantPast
                    ),
                    status: .failed,
                    startedAt: .distantPast
                )],
                olderCursor: nil,
                hasOlder: false,
                snapshotRevision: 1_000,
                requestGeneration: 1
            ),
            sessionID: "thread-1"
        )

        let snapshot = await store.snapshot(sessionID: "thread-1")
        let turn = try #require(snapshot.turns.first)
        #expect(snapshot.turns.count == 1)
        #expect(turn.id == "turn-1")
        #expect(turn.userMessage.id == "message-1")
        #expect(turn.userMessage.text == "Design the launch page")
        #expect(turn.finalAssistantMessage?.id == "local-agent-assistant-run-main-1")
        #expect(turn.finalAssistantMessage?.text == "Final visual specification")
        #expect(turn.revision == 7)
        #expect(turn.status == .completed)
        #expect(turn.processEvents.contains(where: {
            $0.id == "local-agent-recovered-message:assistant-1:reasoning"
                && $0.detail == "Refining the visual hierarchy"
        }))
        #expect(turn.processEvents.contains(where: {
            $0.id == "local-agent-tool-tool-invocation-1" && $0.status == .completed
        }))
        #expect(await client.bindingRequests() == 3)
        #expect(await client.detailOffsets() == [0, 1, 0, 1])
    }
}

private actor MainChatRecoveryClient: NativeLocalAgentMainChatRestoreClient {
    private let run: LocalAgentRunSnapshot
    private var requestedBindings = 0
    private var requestedOffsets: [UInt32] = []

    init(run: LocalAgentRunSnapshot) {
        self.run = run
    }

    func runs(cursor: String?, limit: UInt32) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    ) {
        if cursor == nil {
            return ([run, taskRunnerRecoveryRun()], "second-page")
        }
        return ([], nil)
    }

    func runDetail(
        id: String,
        eventLimit: UInt32,
        eventOffset: UInt32
    ) async throws -> LocalAgentRunDetail {
        requestedOffsets.append(eventOffset)
        var snapshot = run
        if eventOffset > 0 {
            snapshot.version = 7
            snapshot.status = .succeeded
            snapshot.terminalOutcome = .object(["text": .string("Final visual specification")])
            snapshot.updatedAt = "2026-09-13T01:00:05Z"
        } else {
            snapshot.version = 6
            snapshot.status = .modelRunning
            snapshot.terminalOutcome = nil
            snapshot.updatedAt = "2026-09-13T01:00:02Z"
        }
        let allEvents = [
            LocalAgentRunTimelineEvent(
                eventID: "message:assistant-1:content",
                eventType: "message_assistant_content",
                message: "partial result",
                createdAt: "2026-09-13T01:00:01Z"
            ),
            LocalAgentRunTimelineEvent(
                eventID: "message:assistant-1:reasoning",
                eventType: "message_assistant_reasoning",
                message: "Refining the visual hierarchy",
                createdAt: "2026-09-13T01:00:02Z"
            ),
        ]
        let page = eventOffset == 0 ? [allEvents[0]] : [allEvents[1]]
        return LocalAgentRunDetail(
            run: snapshot,
            events: page,
            tools: [recoveryTool()],
            eventsTotal: 2,
            eventsHasMore: eventOffset == 0,
            snapshotEventSequence: eventOffset == 0 ? 10 : 20
        )
    }

    func mainChatRunBinding(runID: String) async throws -> LocalAgentMainChatRunBinding {
        requestedBindings += 1
        return LocalAgentMainChatRunBinding(
            runID: runID,
            threadID: "thread-1",
            turnID: "turn-1",
            messageID: "message-1",
            userMessage: LocalAgentStoredMessage(
                recordID: "message-1",
                runID: runID,
                threadID: "thread-1",
                turnID: "turn-1",
                sequence: 1,
                role: .user,
                content: "Design the launch page",
                messageMode: .semantic,
                messageSource: "main_chat",
                memorySyncStatus: .synced,
                createdAt: "2026-09-13T01:00:00Z"
            )
        )
    }

    func bindingRequests() -> Int { requestedBindings }
    func detailOffsets() -> [UInt32] { requestedOffsets }
}

private func mainChatRecoveryRun() -> LocalAgentRunSnapshot {
    LocalAgentRunSnapshot(
        runID: "run-main-1",
        profileKey: "main_chat",
        ownerUserID: "user-1",
        ownerEntityType: "conversation",
        ownerEntityID: "thread-1",
        projectID: "project-1",
        status: .succeeded,
        version: 7,
        stepSeq: 3,
        iteration: 2,
        retryCount: 0,
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        modelRuntimeSnapshot: .object([:]),
        contextStrategy: "provider_native",
        promptRevision: "prompt-1",
        capabilitySnapshotRef: "capability-1",
        terminalOutcome: .object(["text": .string("Final visual specification")]),
        createdAt: "2026-09-13T01:00:00Z",
        updatedAt: "2026-09-13T01:00:05Z"
    )
}

private func taskRunnerRecoveryRun() -> LocalAgentRunSnapshot {
    var run = mainChatRecoveryRun()
    run.runID = "task-run-2"
    run.profileKey = "task_runner"
    run.ownerEntityType = "task"
    run.ownerEntityID = "task-1"
    return run
}

private func recoveryTool() -> LocalAgentToolSnapshot {
    LocalAgentToolSnapshot(
        invocationID: "tool-invocation-1",
        runID: "run-main-1",
        batchID: "batch-1",
        toolCallID: "call-1",
        toolName: "inspect_design",
        effect: .read,
        argumentsDigest: "sha256:args",
        status: .succeeded,
        boundedResult: .object(["ok": .bool(true)]),
        startedAt: "2026-09-13T01:00:03Z",
        completedAt: "2026-09-13T01:00:04Z"
    )
}
