import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MessageTaskWorkspaceViewModelTests: XCTestCase {
    func testEmptyGraphWithMessageLookupStartsAutomaticRetryState() async {
        let service = MessageTaskGraphServiceStub(graphs: [.empty])
        let viewModel = makeViewModel(service: service)

        await viewModel.refreshWorkspaceState(refreshInspector: false)

        XCTAssertEqual(viewModel.graph?.nodes, [])
        XCTAssertTrue(viewModel.isAwaitingInitialGraph)
        XCTAssertTrue(viewModel.shouldRetryEmptyGraph)
    }

    func testLaterGraphClearsAutomaticRetryState() async {
        let service = MessageTaskGraphServiceStub(graphs: [.empty, .populated])
        let viewModel = makeViewModel(service: service)

        await viewModel.refreshWorkspaceState(refreshInspector: false)
        await viewModel.refreshWorkspaceState(refreshInspector: false)

        XCTAssertEqual(viewModel.graph?.nodes.map(\.id), ["task-1"])
        XCTAssertFalse(viewModel.isAwaitingInitialGraph)
        XCTAssertFalse(viewModel.shouldRetryEmptyGraph)
    }

    func testPollingAutomaticallyRecoversFromInitialEmptyGraph() async throws {
        let service = MessageTaskGraphServiceStub(graphs: [.empty, .populated])
        let viewModel = makeViewModel(service: service)

        await viewModel.refreshWorkspaceState(refreshInspector: false)
        viewModel.startPollingIfNeeded()
        defer { viewModel.stopPolling() }

        for _ in 0..<12 where viewModel.graph?.nodes.isEmpty != false {
            try await Task.sleep(for: .milliseconds(100))
        }

        XCTAssertEqual(viewModel.graph?.nodes.map(\.id), ["task-1"])
        XCTAssertFalse(viewModel.isAwaitingInitialGraph)
    }

    func testTransientEmptyResponseDoesNotEraseLoadedGraph() async {
        let service = MessageTaskGraphServiceStub(graphs: [.populated, .empty])
        let viewModel = makeViewModel(service: service)

        await viewModel.refreshWorkspaceState(refreshInspector: false)
        await viewModel.refreshWorkspaceState(refreshInspector: false)

        XCTAssertEqual(viewModel.graph?.nodes.map(\.id), ["task-1"])
        XCTAssertFalse(viewModel.isAwaitingInitialGraph)
    }

    func testSwitchingTasksCancelsStaleInspectorAndIgnoresItsLateError() async throws {
        let service = SwitchingMessageTaskGraphServiceStub()
        let viewModel = makeViewModel(service: service)
        let first = MessageTask(id: "task-a", title: "任务 A", status: "running")
        let second = MessageTask(id: "task-b", title: "任务 B", status: "completed")

        viewModel.select(first)
        await Task.yield()
        viewModel.select(second)
        try await Task.sleep(for: .milliseconds(150))

        let cancelledFirstRequest = await service.cancelledFirstRequest
        XCTAssertTrue(cancelledFirstRequest)
        XCTAssertEqual(viewModel.selectedTask?.id, second.id)
        XCTAssertEqual(viewModel.taskDetail?.id, second.id)
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertFalse(viewModel.isLoadingInspector)
    }

    private func makeViewModel(
        service: any MessageTaskGraphServicing
    ) -> MessageTaskWorkspaceViewModel {
        let turn = ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "检查任务图",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            messageTaskLookup: MessageTaskLookup(
                sessionID: "session-1",
                turnID: "turn-1",
                sourceUserMessageID: "message-1"
            ),
            status: .completed,
            startedAt: Date(timeIntervalSince1970: 1)
        )
        return MessageTaskWorkspaceViewModel(
            turn: turn,
            graphService: service
        )
    }
}

private actor SwitchingMessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private(set) var cancelledFirstRequest = false

    func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        .empty
    }

    func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        if taskID == "task-a" {
            try? await Task.sleep(for: .milliseconds(75))
            cancelledFirstRequest = Task.isCancelled
            throw SwitchingStubError.staleFailure
        }
        return MessageTask(id: taskID, title: "任务 B", status: "completed")
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        let task = MessageTask(id: "task-b", title: "任务 B", status: "completed")
        return .init(task: task, run: .init(id: runID, taskID: task.id), events: [])
    }

    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        .init(id: runID, taskID: "task-b")
    }

    func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {}
}

private enum SwitchingStubError: Error {
    case staleFailure
}

private actor MessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private var graphs: [MessageTaskGraphSnapshot]

    init(graphs: [MessageTaskGraphSnapshot]) {
        self.graphs = graphs
    }

    func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        guard !graphs.isEmpty else { return .empty }
        return graphs.removeFirst()
    }

    func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        MessageTask(id: taskID, title: "任务一", status: "completed")
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        let task = MessageTask(id: "task-1", title: "任务一", status: "completed")
        return MessageTaskRunDetail(
            task: task,
            run: MessageTaskRun(id: runID, taskID: task.id),
            events: []
        )
    }

    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        MessageTaskRun(id: runID, taskID: "task-1")
    }

    func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {}
}

private extension MessageTaskGraphSnapshot {
    static var empty: MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: [],
            nodes: [],
            edges: [],
            sourceSessionID: "session-1",
            sourceTurnID: "turn-1",
            sourceUserMessageID: "message-1"
        )
    }

    static var populated: MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: ["task-1"],
            nodes: [
                MessageTaskGraphNode(
                    task: MessageTask(id: "task-1", title: "任务一", status: "completed"),
                    depth: 0,
                    isRoot: true,
                    isCurrentMessage: true
                ),
            ],
            edges: [],
            sourceSessionID: "session-1",
            sourceTurnID: "turn-1",
            sourceUserMessageID: "message-1"
        )
    }
}
