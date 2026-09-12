// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Testing

@Suite("Native Local Agent complete restart consistency")
struct NativeLocalAgentRestartConsistencyTests {
    @Test("restores source Turn, Task, complete Run lineage and final results from one Host")
    func restoresCompleteLineage() async throws {
        let client = RestartConsistencyClient()
        let conversationStore = ConversationHistoryStore()
        let taskStore = LocalAgentTaskStateStore()

        async let restoredConversation: Void = NativeLocalAgentMainChatRestorer(
            clientProvider: { client },
            store: conversationStore
        ).restore()
        async let restoredTasks: Void = NativeLocalAgentTaskEventSink(
            clientProvider: { client },
            store: taskStore
        ).restore()
        _ = try await (restoredConversation, restoredTasks)

        let conversation = await conversationStore.snapshot(sessionID: "thread-1")
        let sourceTurn = try #require(conversation.turns.first)
        #expect(conversation.turns.count == 1)
        #expect(sourceTurn.id == "turn-1")
        #expect(sourceTurn.finalAssistantMessage?.text == "Approved visual direction")
        #expect(sourceTurn.status == .completed)

        let restoredTask = try #require(await taskStore.localAgentTask(taskID: "task-1"))
        #expect(restoredTask.task.sourceThreadID == sourceTurn.sessionID)
        #expect(restoredTask.task.sourceTurnID == sourceTurn.id)
        #expect(restoredTask.task.initialRunID == "task-run-1")
        #expect(restoredTask.task.currentRunID == "task-run-2")
        #expect(restoredTask.task.runIDs == ["task-run-1", "task-run-2"])
        #expect(restoredTask.run.runID == "task-run-2")
        #expect(restoredTask.run.status == .succeeded)
        #expect(restoredTask.run.terminalOutcome == .object([
            "text": .string("Final production-ready design"),
        ]))

        #expect(await client.runListRequestCount() == 2)
        #expect(await client.runDetailRequests() == ["main-run-1"])
    }
}

private actor RestartConsistencyClient:
    NativeLocalAgentMainChatRestoreClient,
    NativeLocalAgentTaskStateClient
{
    private let mainRun: LocalAgentRunSnapshot
    private let historicalTaskRun: LocalAgentRunSnapshot
    private let currentTaskRun: LocalAgentRunSnapshot
    private var runListRequests = 0
    private var detailRequests: [String] = []

    init() {
        mainRun = Self.run(
            id: "main-run-1",
            profile: "main_chat",
            ownerType: "conversation",
            ownerID: "thread-1",
            status: .succeeded,
            outcome: "Approved visual direction"
        )
        historicalTaskRun = Self.run(
            id: "task-run-1",
            profile: "task_runner",
            ownerType: "task",
            ownerID: "task-1",
            status: .failed,
            outcome: "First attempt rejected"
        )
        currentTaskRun = Self.run(
            id: "task-run-2",
            profile: "task_runner",
            ownerType: "task",
            ownerID: "task-1",
            status: .succeeded,
            outcome: "Final production-ready design"
        )
    }

    func runs(cursor: String?, limit: UInt32) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    ) {
        runListRequests += 1
        return ([mainRun, historicalTaskRun, currentTaskRun], nil)
    }

    func tasks(cursor: String?, limit: UInt32) async throws -> (
        tasks: [LocalAgentTaskSnapshot], nextCursor: String?
    ) {
        ([taskSnapshot()], nil)
    }

    func run(id: String) async throws -> LocalAgentRunSnapshot {
        return try #require(allRunsByID[id])
    }

    func task(id: String) async throws -> LocalAgentTaskSnapshot {
        let task = taskSnapshot()
        return try #require(id == task.taskID ? task : nil)
    }

    func runDetail(
        id: String,
        eventLimit: UInt32,
        eventOffset: UInt32
    ) async throws -> LocalAgentRunDetail {
        detailRequests.append(id)
        let run = try #require(allRunsByID[id])
        return LocalAgentRunDetail(
            run: run,
            events: [],
            tools: [],
            eventsTotal: 0,
            eventsHasMore: false,
            snapshotEventSequence: 42
        )
    }

    func mainChatRunBinding(runID: String) async throws -> LocalAgentMainChatRunBinding {
        #expect(runID == mainRun.runID)
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
                content: "Design the product page",
                messageMode: .semantic,
                messageSource: "main_chat",
                memorySyncStatus: .synced,
                createdAt: "2026-09-13T01:00:00Z"
            )
        )
    }

    func runListRequestCount() -> Int { runListRequests }
    func runDetailRequests() -> [String] { detailRequests }

    private var allRunsByID: [String: LocalAgentRunSnapshot] {
        Dictionary(uniqueKeysWithValues: [mainRun, historicalTaskRun, currentTaskRun].map {
            ($0.runID, $0)
        })
    }

    private func taskSnapshot() -> LocalAgentTaskSnapshot {
        LocalAgentTaskSnapshot(
            taskID: "task-1",
            revision: 2,
            sourceThreadID: "thread-1",
            sourceTurnID: "turn-1",
            projectID: "project-1",
            initialRunID: "task-run-1",
            currentRunID: "task-run-2",
            runIDs: ["task-run-1", "task-run-2"],
            objective: "Produce the approved visual design",
            acceptanceCriteria: ["Preserve the approved visual hierarchy"],
            status: "succeeded",
            modelConfigID: "model-1",
            modelConfigRevision: 1,
            createdAt: "2026-09-13T01:01:00Z",
            updatedAt: "2026-09-13T01:04:00Z"
        )
    }

    private static func run(
        id: String,
        profile: String,
        ownerType: String,
        ownerID: String,
        status: LocalAgentRunStatus,
        outcome: String
    ) -> LocalAgentRunSnapshot {
        LocalAgentRunSnapshot(
            runID: id,
            profileKey: profile,
            ownerUserID: "user-1",
            ownerEntityType: ownerType,
            ownerEntityID: ownerID,
            projectID: "project-1",
            status: status,
            version: 2,
            stepSeq: 2,
            iteration: 2,
            retryCount: id == "task-run-2" ? 1 : 0,
            modelConfigID: "model-1",
            modelConfigRevision: 1,
            modelRuntimeSnapshot: .object([:]),
            contextStrategy: "provider_native",
            promptRevision: "prompt-1",
            capabilitySnapshotRef: "capabilities-1",
            terminalOutcome: .object(["text": .string(outcome)]),
            createdAt: "2026-09-13T01:00:00Z",
            updatedAt: "2026-09-13T01:05:00Z"
        )
    }
}
