import XCTest
@testable import ChatOSCore

final class ProjectExecutionConfirmationStateTests: XCTestCase {
    func testCompleteReadyGraphCanBeConfirmed() {
        let state = ProjectExecutionConfirmationState(
            context: context(),
            graph: graph(tasks: [
                MessageTask(id: "task-1", title: "分析", status: "ready"),
                MessageTask(id: "task-2", title: "实现", status: "pending"),
            ]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .awaitingConfirmation)
        XCTAssertTrue(state.graphReadyForConfirmation)
        XCTAssertTrue(state.canConfirm)
        XCTAssertEqual(state.identity?.executionGroupID, "group-1")
    }

    func testMissingExecutionGroupNeverEnablesConfirmation() {
        var incomplete = context()
        incomplete.executionGroupID = nil
        let state = ProjectExecutionConfirmationState(
            context: incomplete,
            graph: graph(tasks: [MessageTask(id: "task-1", title: "分析", status: "ready")]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .awaitingConfirmation)
        XCTAssertNil(state.identity)
        XCTAssertFalse(state.canConfirm)
    }

    func testStartedBlockedTaskIsNotTreatedAsAwaitingConfirmation() {
        let state = ProjectExecutionConfirmationState(
            context: context(),
            graph: graph(tasks: [
                MessageTask(
                    id: "task-1",
                    title: "执行",
                    status: "blocked",
                    lastRunID: "run-1",
                    lastRunStatus: "blocked"
                ),
            ]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .blocked)
        XCTAssertTrue(state.hasStartedTasks)
        XCTAssertFalse(state.canConfirm)
    }

    func testCompletedMetadataWithoutAnyGraphIsReportedAsUnavailable() {
        var completed = context()
        completed.overallStatus = "completed"
        completed.confirmationStatus = "completed"
        let state = ProjectExecutionConfirmationState(
            context: completed,
            graph: graph(tasks: []),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .graphUnavailable)
        XCTAssertFalse(state.canConfirm)
    }

    func testConfirmedPlanIsRunningBeforeFirstRunRecordAppears() {
        var confirmed = context()
        confirmed.overallStatus = "processing"
        confirmed.confirmationStatus = "confirmed"
        let state = ProjectExecutionConfirmationState(
            context: confirmed,
            graph: graph(tasks: [MessageTask(id: "task-1", title: "分析", status: "ready")]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .running)
        XCTAssertFalse(state.canConfirm)
    }

    func testPlannerFailureMetadataWinsOverUnstartedReadyTasks() {
        var failed = context()
        failed.overallStatus = "failed"
        failed.confirmationStatus = "failed"
        let state = ProjectExecutionConfirmationState(
            context: failed,
            graph: graph(tasks: [MessageTask(id: "task-1", title: "分析", status: "ready")]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .failed)
        XCTAssertFalse(state.canConfirm)
    }

    func testPartialReadyGraphRemainsPlanningUntilBackendRequestsConfirmation() {
        var planning = context()
        planning.overallStatus = "planning"
        planning.confirmationStatus = "planning"
        let state = ProjectExecutionConfirmationState(
            context: planning,
            graph: graph(tasks: [MessageTask(id: "task-1", title: "分析", status: "ready")]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .planning)
        XCTAssertFalse(state.canConfirm)
    }

    func testBlockedGraphOverridesLaggingRunningExecutionMetadata() {
        var running = context()
        running.overallStatus = "running"
        running.confirmationStatus = "confirmed"
        let state = ProjectExecutionConfirmationState(
            context: running,
            graph: graph(tasks: [
                MessageTask(
                    id: "task-1",
                    title: "真实任务",
                    status: "blocked",
                    lastRunID: "run-1",
                    lastRunStatus: "running"
                ),
            ]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .blocked)
        XCTAssertEqual(state.overallStatus, "blocked")
    }

    func testCancelledTaskStatusOverridesStaleRunningRunStatus() {
        var running = context()
        running.overallStatus = "running"
        running.confirmationStatus = "confirmed"
        let state = ProjectExecutionConfirmationState(
            context: running,
            graph: graph(tasks: [
                MessageTask(
                    id: "task-1",
                    title: "已取消任务",
                    status: "cancelled",
                    lastRunID: "run-1",
                    lastRunStatus: "running"
                ),
            ]),
            conversationID: "conversation-1"
        )

        XCTAssertEqual(state.phase, .failed)
        XCTAssertEqual(state.overallStatus, "cancelled")
    }

    private func context() -> ProjectExecutionContext {
        ProjectExecutionContext(
            projectID: "project-1",
            requirementID: "requirement-1",
            executionGroupID: "group-1",
            contactID: "contact-1",
            mode: "project_requirement_execution",
            confirmationStatus: "awaiting_confirmation"
        )
    }

    private func graph(tasks: [MessageTask]) -> MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: tasks.last.map { [$0.id] } ?? [],
            nodes: tasks.enumerated().map { index, task in
                MessageTaskGraphNode(
                    task: task,
                    depth: index,
                    isRoot: index == tasks.count - 1,
                    isCurrentMessage: true
                )
            },
            edges: []
        )
    }
}
