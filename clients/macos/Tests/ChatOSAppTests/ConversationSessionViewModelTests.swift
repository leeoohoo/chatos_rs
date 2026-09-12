import ChatOSCore
import Combine
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class ConversationSessionViewModelTests: XCTestCase {
    func testUnchangedSnapshotDoesNotPublishViewUpdates() async throws {
        let turn = ConversationRemoteServiceStub.turn(revision: 1)
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [turn],
            historyStore: ConversationHistoryStore()
        )

        try await waitUntil { viewModel.turns == [turn] }
        try await Task.sleep(for: .milliseconds(50))
        var updateCount = 0
        let cancellable = viewModel.objectWillChange.sink {
            updateCount += 1
        }

        await viewModel.refreshSnapshot()

        XCTAssertEqual(updateCount, 0)
        withExtendedLifetime(cancellable) {}
    }

    func testSendingWhileAnotherTurnStreamsCreatesANewLocalRun() async throws {
        let streaming = ConversationRemoteServiceStub.turn(revision: 1)
        let commands = ConversationCommandRecorder()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [streaming],
            historyStore: ConversationHistoryStore(),
            commandService: commands
        )
        try await waitUntil { viewModel.turns == [streaming] }

        viewModel.draft = "Refine the header typography"
        viewModel.sendDraft()

        try await waitUntil {
            await commands.sentCommands().count == 1 && !viewModel.isSending
        }
        let sentCommands = await commands.sentCommands()
        let command = try XCTUnwrap(sentCommands.first)
        XCTAssertEqual(command.sessionID, "session-1")
        XCTAssertNotEqual(command.turnID, streaming.id)
        XCTAssertTrue(command.messageID.hasPrefix("optimistic_"))
        XCTAssertEqual(command.content, "Refine the header typography")
        XCTAssertEqual(viewModel.turns.count, 2)
    }

    func testTaskGraphAvailabilityComesFromStableLocalTaskBinding() async throws {
        let taskStateStore = LocalAgentTaskStateStore()
        try await taskStateStore.registerLocalAgentTask(
            Self.taskSnapshot(),
            run: Self.taskRunSnapshot()
        )
        let turn = ConversationRemoteServiceStub.turn(revision: 1)
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [turn],
            historyStore: ConversationHistoryStore(),
            localAgentTaskStateStore: taskStateStore
        )

        viewModel.activate()
        try await waitUntil { viewModel.tasks(for: turn.id).count == 1 }

        XCTAssertTrue(viewModel.hasTaskGraph(for: turn))
        XCTAssertEqual(viewModel.tasks(for: turn.id).first?.task.taskID, "task-1")
        XCTAssertEqual(viewModel.tasks(for: turn.id).first?.run.runID, "task-run-1")
    }

    private static func taskSnapshot() -> LocalAgentTaskSnapshot {
        LocalAgentTaskSnapshot(
            taskID: "task-1",
            revision: 1,
            sourceThreadID: "session-1",
            sourceTurnID: "turn-1",
            projectID: "project-1",
            currentRunID: "task-run-1",
            runIDs: ["task-run-1"],
            objective: "Refine the visual design",
            acceptanceCriteria: ["Match the approved reference"],
            status: "running",
            modelConfigID: "model-1",
            modelConfigRevision: 1,
            createdAt: "2026-09-12T03:00:00Z",
            updatedAt: "2026-09-12T03:00:00Z"
        )
    }

    private static func taskRunSnapshot() -> LocalAgentRunSnapshot {
        LocalAgentRunSnapshot(
            runID: "task-run-1",
            profileKey: "task_runner",
            ownerUserID: "user-1",
            ownerEntityType: "task",
            ownerEntityID: "task-1",
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
            capabilitySnapshotRef: "capability-1",
            createdAt: "2026-09-12T03:00:00Z",
            updatedAt: "2026-09-12T03:00:00Z"
        )
    }

    private func waitUntil(
        timeoutIterations: Int = 100,
        condition: @escaping @MainActor () async -> Bool
    ) async throws {
        for _ in 0..<timeoutIterations {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTFail("Timed out waiting for asynchronous conversation state")
    }
}

private actor ConversationCommandRecorder: ConversationCommandServicing {
    private var commands: [ConversationSendCommand] = []

    func sendNewTurn(_ command: ConversationSendCommand) async throws
        -> ConversationCommandAck
    {
        commands.append(command)
        return ConversationCommandAck(
            operationID: "operation-1",
            runID: "run-1",
            turnID: command.turnID,
            userMessageID: command.messageID
        )
    }

    func cancelRun(runID: String) async throws {}

    func sentCommands() -> [ConversationSendCommand] { commands }
}

private actor ConversationRemoteServiceStub: ConversationRemoteServicing {
    private var queries: [ConversationHistoryQuery] = []
    private var fetchDelayMilliseconds = 0

    func fetchHistory(_ query: ConversationHistoryQuery) async throws -> HistoryPage {
        queries.append(query)
        let revision = Int64(queries.count)
        let delay = fetchDelayMilliseconds
        if delay > 0 {
            try await Task.sleep(for: .milliseconds(delay))
        }
        return HistoryPage(
            turns: [Self.turn(revision: revision)],
            olderCursor: nil,
            hasOlder: false,
            snapshotRevision: revision,
            requestGeneration: query.requestGeneration
        )
    }

    func requestedSessionIDs() -> [String] {
        queries.map(\.sessionID)
    }

    func requestCount() -> Int {
        queries.count
    }

    func setFetchDelay(milliseconds: Int) {
        fetchDelayMilliseconds = milliseconds
    }

    static func turn(revision: Int64) -> ConversationTurn {
        ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
            sequence: 1,
            revision: revision,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行任务",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            finalAssistantMessage: revision > 1
                ? ChatMessage(
                    id: "assistant-1",
                    role: .assistant,
                    text: "任务已完成",
                    createdAt: Date(timeIntervalSince1970: 2)
                )
                : nil,
            status: revision > 1 ? .completed : .streaming,
            startedAt: Date(timeIntervalSince1970: 1),
            completedAt: revision > 1 ? Date(timeIntervalSince1970: 2) : nil
        )
    }
}
