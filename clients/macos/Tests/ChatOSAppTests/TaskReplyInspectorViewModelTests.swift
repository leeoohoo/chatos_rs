import ChatOSCore
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

    func testDetailWithCallbackRunLoadsOnlyRun() async throws {
        let service = TaskReplyInspectorServiceStub()
        let viewModel = TaskReplyInspectorViewModel(
            selection: makeSelection(section: .detail),
            service: service
        )

        viewModel.load()
        try await waitUntilLoaded(viewModel)

        let calls = await service.callCounts()
        XCTAssertEqual(calls.fetchTask, 0)
        XCTAssertEqual(calls.fetchRun, 1)
        XCTAssertEqual(viewModel.task?.lastRun?.reportContent, "模型输出")
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

    private func makeSelection(section: TaskReplyInspectorSection) -> TaskReplySelection {
        let now = Date(timeIntervalSince1970: 1)
        let userMessage = ChatMessage(id: "user-1", role: .user, text: "执行", createdAt: now)
        let reply = ConversationAssistantReply(
            message: ChatMessage(id: "reply-1", role: .assistant, text: "完成", createdAt: now),
            taskCallback: TaskRunnerCallbackReference(
                taskID: "task-1",
                runID: "run-1",
                sourceSessionID: "session-1",
                sourceTurnID: "turn-1",
                sourceUserMessageID: "user-1"
            )
        )
        let turn = ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
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
    private var taskCalls = 0
    private var runCalls = 0

    init(failRunRequest: Bool = false) {
        self.failRunRequest = failRunRequest
    }

    func callCounts() -> (fetchTask: Int, fetchRun: Int) {
        (taskCalls, runCalls)
    }

    func fetchGraph(
        sourceThreadID: String,
        sourceTurnID: String
    ) async throws -> MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: [],
            nodes: [],
            edges: [],
            sourceSessionID: "session-1"
        )
    }

    func fetchTask(taskID: String) async throws -> MessageTask {
        taskCalls += 1
        return task()
    }

    func fetchRun(
        taskID: String,
        runID: String,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        runCalls += 1
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

    func retryTask(
        taskID: String,
        expectedRunID: String,
        instruction: String?
    ) async throws -> MessageTaskRun {
        MessageTaskRun(id: expectedRunID, taskID: taskID)
    }

    func cancelTask(taskID: String) async throws {}

    private func task() -> MessageTask {
        MessageTask(
            id: "task-1",
            title: "任务一",
            status: "succeeded",
            processLog: "[2026-09-08 10:00] 开始\n执行完成",
            lastRunID: "run-1"
        )
    }
}

private enum TaskReplyInspectorStubError: LocalizedError {
    case runUnavailable

    var errorDescription: String? { "Run 暂时不可用" }
}
