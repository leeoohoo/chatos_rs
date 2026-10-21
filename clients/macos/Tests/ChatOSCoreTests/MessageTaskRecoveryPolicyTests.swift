import ChatOSCore
import Foundation
import XCTest

final class MessageTaskRecoveryPolicyTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 10_000)

    func testNormalRunningTaskIsNotForceRestartable() {
        let task = runningTask(activity: MessageTaskRunActivity(
            kind: .runningModel,
            startedAt: now.addingTimeInterval(-600),
            lastProgressAt: now.addingTimeInterval(-600)
        ))

        XCTAssertFalse(MessageTaskRecoveryPolicy.canForceRestart(task, now: now))
    }

    func testWaitingToolBecomesDiagnosableButNotForceRestartable() {
        XCTAssertFalse(MessageTaskRecoveryPolicy.shouldOfferDiagnosis(
            runningTask(activity: MessageTaskRunActivity(
                kind: .waitingTool,
                startedAt: now.addingTimeInterval(-59),
                lastProgressAt: now.addingTimeInterval(-59)
            )),
            now: now
        ))
        let stalled = runningTask(activity: MessageTaskRunActivity(
                kind: .waitingTool,
                startedAt: now.addingTimeInterval(-60),
                lastProgressAt: now.addingTimeInterval(-60)
            ))
        XCTAssertTrue(MessageTaskRecoveryPolicy.shouldOfferDiagnosis(stalled, now: now))
        XCTAssertFalse(MessageTaskRecoveryPolicy.canForceRestart(stalled, now: now))
    }

    func testExplicitRuntimeInterruptionIsForceRestartable() {
        var task = runningTask(activity: MessageTaskRunActivity(
            kind: .waitingTool,
            startedAt: now.addingTimeInterval(-600),
            lastProgressAt: now.addingTimeInterval(-600)
        ))
        task.status = "blocked"
        task.lastRun?.errorMessage = "任务执行意外中断，插件进程已退出。"

        XCTAssertTrue(MessageTaskRecoveryPolicy.canForceRestart(task, now: now))
    }

    func testBusinessFailureDoesNotOfferForceRestart() {
        var task = runningTask(activity: MessageTaskRunActivity(
            kind: .waitingTool,
            startedAt: now.addingTimeInterval(-600)
        ))
        task.status = "failed"
        task.lastRun?.errorMessage = "验收未通过：页面内容不符合要求"

        XCTAssertFalse(MessageTaskRecoveryPolicy.canForceRestart(task, now: now))
    }

    func testOrdinaryPluginFailureDoesNotMasqueradeAsProcessInterruption() {
        var task = runningTask(activity: MessageTaskRunActivity(
            kind: .waitingTool,
            startedAt: now.addingTimeInterval(-600)
        ))
        task.status = "failed"
        task.lastRun?.errorMessage = "Plugin process rejected invalid tool arguments"

        XCTAssertFalse(MessageTaskRecoveryPolicy.canForceRestart(task, now: now))
    }

    private func runningTask(activity: MessageTaskRunActivity) -> MessageTask {
        MessageTask(
            id: "task-1",
            title: "Acceptance",
            status: "running",
            lastRunID: "run-1",
            lastRun: MessageTaskLastRunSummary(
                id: "run-1",
                status: "waiting_tool_result",
                activity: activity
            )
        )
    }
}
