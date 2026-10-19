import ChatOSCore
import Combine
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class TaskReplyInspectorViewModelTests: XCTestCase {
    func testProcessLoadsOnlyTask() async throws {
        let service = TaskReplyInspectorServiceStub()
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .process),
            service: service
        )

        viewModel.load()
        try await waitUntilLoaded(viewModel)

        let calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 1)
        XCTAssertEqual(calls.fetchRun, 0)
        XCTAssertEqual(viewModel.processTimelineItems.count, 1)
    }

    func testDetailLoadsCurrentTaskBeforeItsLatestRun() async throws {
        let service = TaskReplyInspectorServiceStub()
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )

        viewModel.load()
        try await waitUntilLoaded(viewModel)

        let calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 1)
        XCTAssertEqual(calls.fetchRun, 1)
        XCTAssertEqual(viewModel.task?.lastRun?.reportContent, "模型输出")
    }

    func testDetailIgnoresHistoricalCallbackRunAfterRetry() async throws {
        let service = TaskReplyInspectorServiceStub(latestRunID: "run-2")
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )

        viewModel.load()
        try await waitUntilLoaded(viewModel)

        let requestedRunIDs = await service.requestedRunIDs()
        XCTAssertEqual(requestedRunIDs, ["run-2"])
        XCTAssertEqual(viewModel.task?.lastRun?.id, "run-2")
    }

    func testSwitchingFromProcessToDetailReusesTaskAndLoadsOnlyRun() async throws {
        let service = TaskReplyInspectorServiceStub()
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .process),
            service: service
        )

        viewModel.load()
        try await waitUntilLoaded(viewModel)
        viewModel.selectSection(.detail)
        try await waitUntilLoaded(viewModel)

        var calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 1)
        XCTAssertEqual(calls.fetchRun, 1)

        viewModel.selectSection(.process)
        calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 1)
        XCTAssertEqual(calls.fetchRun, 1)
    }

    func testDetailFallsBackToTaskWhenRunRequestFails() async throws {
        let service = TaskReplyInspectorServiceStub(failRunRequest: true)
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )

        viewModel.load()
        try await waitUntilLoaded(viewModel)

        let calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 1)
        XCTAssertEqual(calls.fetchRun, 1)
        XCTAssertEqual(viewModel.task?.id, "task-1")
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertNotNil(viewModel.modelOutputError)
    }

    func testRealtimeRefreshSupersedingVisibleLoadClearsLoadingState() async throws {
        let service = TaskReplyInspectorServiceStub(fetchTaskDelayMilliseconds: 100)
        let realtimeService = TaskReplyInspectorRealtimeServiceStub()
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service,
            realtimeService: realtimeService
        )

        viewModel.load()
        XCTAssertTrue(viewModel.isLoading)
        try await waitUntilRealtimeSubscribed(realtimeService)

        await realtimeService.yield(ConversationRealtimeSignal(
            eventID: "event-1",
            eventSequence: 1,
            sessionID: "session-1",
            turnID: "turn-1",
            kind: .updated,
            eventName: "task.updated",
            timestamp: "2026-10-19T00:00:00Z"
        ))

        try await waitUntilLoaded(viewModel)
        let calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 2)
        XCTAssertFalse(viewModel.isLoading)
        XCTAssertFalse(viewModel.isLoadingModelOutput)
    }

    func testRetryFailureRemainsVisibleWithLoadedTask() async throws {
        let service = TaskReplyInspectorServiceStub(failRetryRequest: true)
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )

        viewModel.load()
        try await waitUntilLoaded(viewModel)
        viewModel.retryInstruction = "Use the current date"
        viewModel.retry()
        try await waitUntilRetryFinishes(viewModel)

        XCTAssertEqual(viewModel.task?.id, "task-1")
        XCTAssertEqual(viewModel.retryInstruction, "Use the current date")
        XCTAssertEqual(viewModel.retryErrorMessage, "重试被拒绝")
    }

    func testStopRejectsLateRetryResponse() async throws {
        let service = TaskReplyInspectorServiceStub(delayRetryRequest: true)
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )
        viewModel.load()
        try await waitUntilLoaded(viewModel)
        viewModel.retryInstruction = "保留这条重试说明"

        viewModel.retry()
        try await waitUntilPendingRetry(service)
        viewModel.stop()
        await service.resumeRetryRequest()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertFalse(viewModel.isRetrying)
        XCTAssertEqual(viewModel.retryInstruction, "保留这条重试说明")
        XCTAssertEqual(viewModel.task?.id, "task-1")
        XCTAssertNil(viewModel.retryErrorMessage)
        let calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 1)
    }

    func testStopDisablesRefreshUntilReloadStartsNewPresentation() async throws {
        let service = TaskReplyInspectorServiceStub()
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )
        viewModel.load()
        try await waitUntilLoaded(viewModel)
        let oldSession = try XCTUnwrap(viewModel.lifecycleState.activeSession)

        viewModel.stop()
        viewModel.refresh()
        try await Task.sleep(for: .milliseconds(30))

        var calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 1)
        XCTAssertNil(viewModel.lifecycleState.activeSession)

        viewModel.load()
        try await waitUntilFetchTaskCount(2, service: service)
        let currentSession = try XCTUnwrap(viewModel.lifecycleState.activeSession)
        calls = await service.callCounts()

        XCTAssertEqual(calls.fetchTask, 2)
        XCTAssertEqual(currentSession.generation, oldSession.generation + 1)
        XCTAssertFalse(viewModel.lifecycleState.accepts(oldSession))
    }

    func testChangingRetryTargetRejectsLateRetryResponse() async throws {
        let service = TaskReplyInspectorServiceStub(delayRetryRequest: true)
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )
        viewModel.load()
        try await waitUntilLoaded(viewModel)
        viewModel.retryInstruction = "旧任务的说明"

        viewModel.retry()
        try await waitUntilPendingRetry(service)
        viewModel.update(selection: makeSelection(
            section: .detail,
            turnID: "turn-2",
            replyID: "reply-2",
            taskID: "task-2",
            runID: "run-2"
        ))
        try await waitUntilLoaded(viewModel)
        await service.resumeRetryRequest()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertFalse(viewModel.isRetrying)
        XCTAssertEqual(viewModel.retryInstruction, "")
        XCTAssertNil(viewModel.retryErrorMessage)
        let calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 2)
    }

    func testIdenticalPollingResultDoesNotRepublishInspectorState() {
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .process),
            service: TaskReplyInspectorServiceStub()
        )
        let task = MessageTask(
            id: "task-1",
            title: "任务一",
            status: "running",
            processLog: "正在执行"
        )
        viewModel.apply(task)

        var publicationCount = 0
        let cancellable = viewModel.objectWillChange.sink {
            publicationCount += 1
        }
        viewModel.apply(task)

        XCTAssertEqual(publicationCount, 0)
        withExtendedLifetime(cancellable) {}
    }

    private func waitUntilLoaded(
        _ viewModel: TaskReplyInspectorViewModel,
        timeoutIterations: Int = 100
    ) async throws {
        for _ in 0..<timeoutIterations {
            if !viewModel.isLoading, viewModel.task != nil { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("任务详情未在预期时间内完成加载")
    }

    private func waitUntilRetryFinishes(
        _ viewModel: TaskReplyInspectorViewModel,
        timeoutIterations: Int = 100
    ) async throws {
        for _ in 0..<timeoutIterations {
            if !viewModel.isRetrying, viewModel.retryErrorMessage != nil { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("任务重试未在预期时间内结束")
    }

    private func waitUntilRealtimeSubscribed(
        _ service: TaskReplyInspectorRealtimeServiceStub,
        timeoutIterations: Int = 100
    ) async throws {
        for _ in 0..<timeoutIterations {
            if await service.hasSubscriber(sessionID: "session-1") { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("实时任务流未在预期时间内建立")
    }

    private func waitUntilPendingRetry(
        _ service: TaskReplyInspectorServiceStub,
        timeoutIterations: Int = 100
    ) async throws {
        for _ in 0..<timeoutIterations {
            if await service.hasPendingRetryRequest() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("任务重试未进入待完成状态")
    }

    private func waitUntilFetchTaskCount(
        _ expectedCount: Int,
        service: TaskReplyInspectorServiceStub,
        timeoutIterations: Int = 100
    ) async throws {
        for _ in 0..<timeoutIterations {
            let counts = await service.callCounts()
            if counts.fetchTask == expectedCount { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("任务详情未达到预期请求次数")
    }

    private func makeSelection(
        section: TaskReplyInspectorSection,
        sessionID: String = "session-1",
        turnID: String = "turn-1",
        replyID: String = "reply-1",
        taskID: String = "task-1",
        runID: String = "run-1"
    ) -> TaskReplySelection {
        let now = Date(timeIntervalSince1970: 1)
        let userMessage = ChatMessage(
            id: "user-\(turnID)",
            role: .user,
            text: "执行",
            createdAt: now
        )
        let reply = ConversationAssistantReply(
            message: ChatMessage(id: replyID, role: .assistant, text: "完成", createdAt: now),
            taskCallback: TaskExecutionCallbackReference(
                taskID: taskID,
                runID: runID,
                sourceSessionID: sessionID,
                sourceTurnID: turnID,
                sourceUserMessageID: userMessage.id
            )
        )
        let turn = ConversationTurn(
            id: turnID,
            sessionID: sessionID,
            sequence: 1,
            revision: 1,
            userMessage: userMessage,
            assistantReplies: [reply],
            status: .completed,
            startedAt: now,
            completedAt: now
        )
        return TaskReplySelection(turn: turn, reply: reply, initialSection: section)
    }
}

private actor TaskReplyInspectorServiceStub: MessageTaskGraphServicing {
    private let failRunRequest: Bool
    private let failRetryRequest: Bool
    private let latestRunID: String
    private let fetchTaskDelayMilliseconds: Int
    private let delayRetryRequest: Bool
    private var taskCalls = 0
    private var runCalls = 0
    private var runIDs: [String] = []
    private var retryContinuation: CheckedContinuation<Void, Never>?

    init(
        failRunRequest: Bool = false,
        failRetryRequest: Bool = false,
        latestRunID: String = "run-1",
        fetchTaskDelayMilliseconds: Int = 0,
        delayRetryRequest: Bool = false
    ) {
        self.failRunRequest = failRunRequest
        self.failRetryRequest = failRetryRequest
        self.latestRunID = latestRunID
        self.fetchTaskDelayMilliseconds = fetchTaskDelayMilliseconds
        self.delayRetryRequest = delayRetryRequest
    }

    func callCounts() -> (fetchTask: Int, fetchRun: Int) {
        (taskCalls, runCalls)
    }

    func requestedRunIDs() -> [String] {
        runIDs
    }

    func hasPendingRetryRequest() -> Bool {
        retryContinuation != nil
    }

    func resumeRetryRequest() {
        retryContinuation?.resume()
        retryContinuation = nil
    }

    func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: [],
            nodes: [],
            edges: [],
            sourceSessionID: "session-1"
        )
    }

    func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        taskCalls += 1
        if fetchTaskDelayMilliseconds > 0 {
            try await Task.sleep(for: .milliseconds(fetchTaskDelayMilliseconds))
        }
        return task()
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        runCalls += 1
        runIDs.append(runID)
        if failRunRequest {
            throw TaskReplyInspectorStubError.runUnavailable
        }
        return MessageTaskRunDetail(
            task: task(),
            run: MessageTaskRun(
                id: runID,
                taskID: "task-1",
                status: "succeeded",
                resultSummary: "完成",
                reportContent: "模型输出"
            ),
            events: []
        )
    }

    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        if delayRetryRequest {
            await withCheckedContinuation { continuation in
                retryContinuation = continuation
            }
        }
        if failRetryRequest {
            throw TaskReplyInspectorStubError.retryRejected
        }
        return MessageTaskRun(id: runID, taskID: "task-1")
    }

    func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {}

    private func task() -> MessageTask {
        MessageTask(
            id: "task-1",
            title: "任务一",
            status: "succeeded",
            processLog: "[2026-09-08 10:00] 开始\n执行完成",
            lastRunID: latestRunID
        )
    }
}

private actor TaskReplyInspectorRealtimeServiceStub: ConversationRealtimeStreaming {
    private var continuations: [
        String: AsyncThrowingStream<ConversationRealtimeSignal, Error>.Continuation
    ] = [:]

    func events(
        sessionID: String
    ) -> AsyncThrowingStream<ConversationRealtimeSignal, Error> {
        let (stream, continuation) = AsyncThrowingStream.makeStream(
            of: ConversationRealtimeSignal.self,
            throwing: Error.self
        )
        continuations[sessionID] = continuation
        return stream
    }

    func hasSubscriber(sessionID: String) -> Bool {
        continuations[sessionID] != nil
    }

    func yield(_ signal: ConversationRealtimeSignal) {
        continuations[signal.sessionID]?.yield(signal)
    }
}

private enum TaskReplyInspectorStubError: LocalizedError {
    case runUnavailable
    case retryRejected

    var errorDescription: String? {
        switch self {
        case .runUnavailable: "Run 暂时不可用"
        case .retryRejected: "重试被拒绝"
        }
    }
}
