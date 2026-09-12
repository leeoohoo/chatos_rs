import ChatOSCore
import Combine
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class ConversationSessionViewModelTests: XCTestCase {
    func testUnchangedSnapshotDoesNotPublishViewUpdates() async throws {
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            historyStore: ConversationHistoryStore()
        )

        try await waitUntil { viewModel.turns.isEmpty }
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
        let streaming = Self.optimisticTurn()
        let store = ConversationHistoryStore()
        try await store.upsertOptimisticTurn(streaming, sessionID: "session-1")
        let commands = ConversationCommandRecorder()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            historyStore: store,
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

    func testFailedSendRemovesOptimisticTurnAndRestoresDraftAndAttachments() async throws {
        let store = ConversationHistoryStore()
        let commands = ConversationCommandRecorder(error: ConversationCommandTestError.rejected)
        let attachment = ConversationAttachmentDraft(
            id: "attachment-1",
            name: "reference.txt",
            mimeType: "text/plain",
            kind: .file,
            origin: .pastedText,
            data: Data("visual reference".utf8)
        )
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            historyStore: store,
            commandService: commands
        )
        viewModel.draft = "Refine the visual hierarchy"
        viewModel.attachments = [attachment]

        viewModel.sendDraft()

        try await waitUntil { !viewModel.isSending && viewModel.sendError != nil }
        XCTAssertEqual(viewModel.draft, "Refine the visual hierarchy")
        XCTAssertEqual(viewModel.attachments, [attachment])
        XCTAssertTrue(viewModel.turns.isEmpty)
        let snapshot = await store.snapshot(sessionID: "session-1")
        XCTAssertEqual(snapshot.turns, [])
    }

    func testTaskGraphAvailabilityComesFromStableLocalTaskBinding() async throws {
        let taskStateStore = LocalAgentTaskStateStore()
        try await taskStateStore.registerLocalAgentTask(
            Self.taskSnapshot(),
            run: Self.taskRunSnapshot()
        )
        let turn = Self.optimisticTurn()
        let store = ConversationHistoryStore()
        try await store.upsertOptimisticTurn(turn, sessionID: "session-1")
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            historyStore: store,
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
            initialRunID: "task-run-1",
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

    private static func optimisticTurn() -> ConversationTurn {
        ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
            sequence: 1,
            revision: 0,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行任务",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            status: .streaming,
            startedAt: Date(timeIntervalSince1970: 1)
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

private enum ConversationCommandTestError: Error {
    case rejected
}

private actor ConversationCommandRecorder: ConversationCommandServicing {
    private var commands: [ConversationSendCommand] = []
    private let error: Error?

    init(error: Error? = nil) {
        self.error = error
    }

    func sendNewTurn(_ command: ConversationSendCommand) async throws
        -> ConversationCommandAck
    {
        commands.append(command)
        if let error { throw error }
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
