import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MessageTaskWorkspaceViewModelTests: XCTestCase {
    func testEmptyGraphWithStableTaskIdentityStartsAutomaticRetryState() async {
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

    private func makeViewModel(
        service: MessageTaskGraphServiceStub
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
            status: .completed,
            startedAt: Date(timeIntervalSince1970: 1)
        )
        return MessageTaskWorkspaceViewModel(
            turn: turn,
            graphService: service,
            initialTaskID: "task-1"
        )
    }
}

private actor MessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private var graphs: [MessageTaskGraphSnapshot]

    init(graphs: [MessageTaskGraphSnapshot]) {
        self.graphs = graphs
    }

    func fetchGraph(
        sourceThreadID: String,
        sourceTurnID: String
    ) async throws -> MessageTaskGraphSnapshot {
        guard !graphs.isEmpty else { return .empty }
        return graphs.removeFirst()
    }

    func fetchTask(taskID: String) async throws -> MessageTask {
        MessageTask(id: taskID, title: "任务一", status: "completed")
    }

    func fetchRun(
        taskID: String,
        runID: String,
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

    func retryTask(
        taskID: String,
        expectedRunID: String,
        instruction: String?
    ) async throws -> MessageTaskRun {
        MessageTaskRun(id: expectedRunID, taskID: taskID)
    }

    func cancelTask(taskID: String) async throws {}
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
