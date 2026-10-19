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

    func testReloadingSameTaskRejectsOlderInspectorResult() async throws {
        let service = SameTaskReloadMessageTaskGraphServiceStub()
        let viewModel = makeViewModel(service: service)
        let task = MessageTask(id: "task-a", title: "任务 A", status: "completed")

        viewModel.select(task)
        try await waitUntil { await service.hasDelayedFirstRequest() }
        viewModel.select(task)
        try await waitUntil { viewModel.taskDetail?.title == "最新详情" }
        await service.resumeFirstRequest()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertEqual(viewModel.selectedTask?.id, task.id)
        XCTAssertEqual(viewModel.taskDetail?.title, "最新详情")
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertFalse(viewModel.isLoadingInspector)
    }

    func testSelectingTaskWithRunLoadsDetailBeforeModelOutput() async throws {
        let service = SequencedInspectorMessageTaskGraphServiceStub()
        let viewModel = makeViewModel(service: service)
        let task = MessageTask(
            id: "task-a",
            title: "任务列表摘要",
            status: "completed",
            lastRunID: "run-a"
        )

        viewModel.select(task)
        try await waitUntil {
            viewModel.taskDetail?.title == "服务端任务详情"
                && viewModel.taskDetail?.lastRun?.id == "run-a"
        }

        let requestCounts = await service.requestCounts()
        XCTAssertEqual(requestCounts.task, 1)
        XCTAssertEqual(requestCounts.modelOutput, 1)
        XCTAssertEqual(viewModel.selectedTask?.id, task.id)
        XCTAssertFalse(viewModel.isLoadingInspector)
        XCTAssertFalse(viewModel.isLoadingModelOutput)
        XCTAssertNil(viewModel.errorMessage)
    }

    func testSwitchingTaskRejectsLateRetryCompletionForOldTarget() async throws {
        let service = DelayedRetryMessageTaskGraphServiceStub()
        let viewModel = makeViewModel(service: service)
        _ = viewModel.workspaceLifecycleState.start()
        let first = MessageTask(
            id: "task-a",
            title: "任务 A",
            status: "failed",
            lastRunID: "run-a"
        )
        let second = MessageTask(
            id: "task-b",
            title: "任务 B",
            status: "completed",
            lastRunID: "run-b"
        )
        viewModel.select(first)
        try await waitUntil { viewModel.taskDetail?.id == first.id }
        viewModel.retryInstruction = "旧任务的重试说明"
        viewModel.retrySelectedRun()
        try await waitUntil { await service.hasDelayedRetry() }

        viewModel.select(second)
        try await waitUntil { viewModel.taskDetail?.id == second.id }

        XCTAssertFalse(viewModel.isRetrying)
        XCTAssertEqual(viewModel.retryInstruction, "")
        XCTAssertNil(viewModel.retryState.activeToken)

        await service.resumeRetry()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertEqual(viewModel.selectedTask?.id, second.id)
        XCTAssertEqual(viewModel.taskDetail?.id, second.id)
        XCTAssertFalse(viewModel.isRetrying)
        XCTAssertNil(viewModel.retryState.activeToken)
    }

    func testStoppedPollingSessionRejectsLateGraphResult() async throws {
        let service = DelayedPollingMessageTaskGraphServiceStub()
        let viewModel = makeViewModel(service: service)
        let session = viewModel.graphPollingState.startSession()

        let refresh = Task {
            await viewModel.refreshWorkspaceState(
                refreshInspector: false,
                pollingSession: session
            )
        }
        try await waitUntil { await service.hasDelayedGraphRequest() }
        viewModel.stopPolling()
        await service.resumeGraphRequest()
        await refresh.value

        XCTAssertNil(viewModel.graph)
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertFalse(viewModel.graphPollingState.accepts(session))
    }

    func testStoppingWorkspaceRejectsLateInitialLoadAndBackgroundRestart() async throws {
        let service = DelayedPollingMessageTaskGraphServiceStub()
        let viewModel = makeViewModel(service: service)

        viewModel.load()
        try await waitUntil { await service.hasDelayedGraphRequest() }
        let session = try XCTUnwrap(viewModel.workspaceLifecycleState.activeSession)
        viewModel.stop()
        await service.resumeGraphRequest()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertNil(viewModel.graph)
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertFalse(viewModel.isLoading)
        XCTAssertFalse(viewModel.workspaceLifecycleState.accepts(session))
        XCTAssertNil(viewModel.initialLoadTask)
        XCTAssertNil(viewModel.pollingTask)
        XCTAssertNil(viewModel.realtimeTask)
    }

    func testDelayedInspectorRequestDoesNotRetainWorkspaceViewModel() async throws {
        let service = NonCancellableInspectorMessageTaskGraphServiceStub()
        var viewModel: MessageTaskWorkspaceViewModel? = makeViewModel(service: service)
        weak let weakViewModel = viewModel
        let task = MessageTask(id: "task-a", title: "任务 A", status: "running")

        viewModel?.select(task)
        try await waitUntil { await service.hasDelayedTaskRequest() }
        viewModel = nil

        XCTAssertNil(weakViewModel)

        await service.resumeTaskRequest()
        try await Task.sleep(for: .milliseconds(30))
        XCTAssertNil(weakViewModel)
    }

    private func waitUntil(
        _ condition: @escaping @MainActor () async -> Bool
    ) async throws {
        for _ in 0..<100 {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for task workspace state")
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

private actor NonCancellableInspectorMessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private var taskContinuation: CheckedContinuation<Void, Never>?

    func hasDelayedTaskRequest() -> Bool { taskContinuation != nil }

    func resumeTaskRequest() {
        taskContinuation?.resume()
        taskContinuation = nil
    }

    func fetchGraph(
        messageID _: String,
        lookup _: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        .empty
    }

    func fetchTask(
        messageID _: String,
        taskID: String,
        lookup _: MessageTaskLookup?
    ) async throws -> MessageTask {
        await withCheckedContinuation { continuation in
            taskContinuation = continuation
        }
        return MessageTask(id: taskID, title: "迟到详情", status: "completed")
    }

    func fetchRun(
        messageID _: String,
        runID: String,
        lookup _: MessageTaskLookup?,
        includeEvents _: Bool,
        eventLimit _: Int,
        eventOffset _: Int
    ) async throws -> MessageTaskRunDetail {
        let task = MessageTask(id: "task-a", title: "任务 A", status: "completed")
        return .init(task: task, run: .init(id: runID, taskID: task.id), events: [])
    }

    func retryRun(
        messageID _: String,
        runID: String,
        lookup _: MessageTaskLookup?,
        instruction _: String?
    ) async throws -> MessageTaskRun {
        .init(id: runID, taskID: "task-a")
    }

    func cancelTask(
        messageID _: String,
        taskID _: String,
        lookup _: MessageTaskLookup?,
        reason _: String?
    ) async throws {}
}

private actor SequencedInspectorMessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private var taskRequestCount = 0
    private var modelOutputRequestCount = 0

    func requestCounts() -> (task: Int, modelOutput: Int) {
        (taskRequestCount, modelOutputRequestCount)
    }

    func fetchGraph(
        messageID _: String,
        lookup _: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        .empty
    }

    func fetchTask(
        messageID _: String,
        taskID: String,
        lookup _: MessageTaskLookup?
    ) async throws -> MessageTask {
        taskRequestCount += 1
        return MessageTask(
            id: taskID,
            title: "服务端任务详情",
            status: "completed",
            lastRunID: "run-a"
        )
    }

    func fetchRun(
        messageID _: String,
        runID: String,
        lookup _: MessageTaskLookup?,
        includeEvents _: Bool,
        eventLimit _: Int,
        eventOffset _: Int
    ) async throws -> MessageTaskRunDetail {
        modelOutputRequestCount += 1
        let task = MessageTask(
            id: "task-a",
            title: "服务端任务详情",
            status: "completed",
            lastRunID: runID
        )
        return .init(
            task: task,
            run: .init(id: runID, taskID: task.id, reportContent: "完成"),
            events: []
        )
    }

    func retryRun(
        messageID _: String,
        runID: String,
        lookup _: MessageTaskLookup?,
        instruction _: String?
    ) async throws -> MessageTaskRun {
        .init(id: runID, taskID: "task-a")
    }

    func cancelTask(
        messageID _: String,
        taskID _: String,
        lookup _: MessageTaskLookup?,
        reason _: String?
    ) async throws {}
}

private actor DelayedRetryMessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private var retryContinuation: CheckedContinuation<Void, Never>?

    func hasDelayedRetry() -> Bool { retryContinuation != nil }

    func resumeRetry() {
        retryContinuation?.resume()
        retryContinuation = nil
    }

    func fetchGraph(
        messageID _: String,
        lookup _: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        .empty
    }

    func fetchTask(
        messageID _: String,
        taskID: String,
        lookup _: MessageTaskLookup?
    ) async throws -> MessageTask {
        MessageTask(
            id: taskID,
            title: taskID == "task-a" ? "任务 A" : "任务 B",
            status: taskID == "task-a" ? "failed" : "completed",
            lastRunID: taskID == "task-a" ? "run-a" : "run-b"
        )
    }

    func fetchRun(
        messageID _: String,
        runID: String,
        lookup _: MessageTaskLookup?,
        includeEvents _: Bool,
        eventLimit _: Int,
        eventOffset _: Int
    ) async throws -> MessageTaskRunDetail {
        let taskID = runID == "run-a" ? "task-a" : "task-b"
        let task = try await fetchTask(messageID: "", taskID: taskID, lookup: nil)
        return .init(
            task: task,
            run: .init(id: runID, taskID: taskID),
            events: []
        )
    }

    func retryRun(
        messageID _: String,
        runID: String,
        lookup _: MessageTaskLookup?,
        instruction _: String?
    ) async throws -> MessageTaskRun {
        await withCheckedContinuation { continuation in
            retryContinuation = continuation
        }
        return .init(id: runID, taskID: "task-a")
    }

    func cancelTask(
        messageID _: String,
        taskID _: String,
        lookup _: MessageTaskLookup?,
        reason _: String?
    ) async throws {}
}

private actor DelayedPollingMessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private var graphContinuation: CheckedContinuation<Void, Never>?

    func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        await withCheckedContinuation { continuation in
            graphContinuation = continuation
        }
        return .populated
    }

    func hasDelayedGraphRequest() -> Bool { graphContinuation != nil }

    func resumeGraphRequest() {
        graphContinuation?.resume()
        graphContinuation = nil
    }

    func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        MessageTask(id: taskID, title: "Task", status: "completed")
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        let task = MessageTask(id: "task-1", title: "Task", status: "completed")
        return .init(task: task, run: .init(id: runID, taskID: task.id), events: [])
    }

    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        .init(id: runID, taskID: "task-1")
    }

    func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {}
}

private actor SameTaskReloadMessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private var taskRequestCount = 0
    private var delayedFirstRequestContinuation: CheckedContinuation<Void, Never>?

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
        taskRequestCount += 1
        let requestNumber = taskRequestCount
        if requestNumber == 1 {
            await withCheckedContinuation { continuation in
                delayedFirstRequestContinuation = continuation
            }
        }
        return MessageTask(
            id: taskID,
            title: requestNumber == 1 ? "过期详情" : "最新详情",
            status: "completed"
        )
    }

    func hasDelayedFirstRequest() -> Bool {
        delayedFirstRequestContinuation != nil
    }

    func resumeFirstRequest() {
        delayedFirstRequestContinuation?.resume()
        delayedFirstRequestContinuation = nil
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        let task = MessageTask(id: "task-a", title: "最新详情", status: "completed")
        return .init(task: task, run: .init(id: runID, taskID: task.id), events: [])
    }

    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        .init(id: runID, taskID: "task-a")
    }

    func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {}
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
