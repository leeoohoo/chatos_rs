import Foundation
import Testing
@testable import ChatOSCore

struct PetActivityRecoveryMapperTests {
    @Test
    func authoritativeCancelledTaskRemovesStaleRunningActivity() {
        let now = Date()
        let staleActivity = PetActivity(
            id: "task-runner:task-1",
            source: .taskRunner,
            kind: .working,
            title: "任务正在执行",
            route: PetActivityRoute(messageID: "message-1", taskID: "task-1"),
            updatedAt: now.addingTimeInterval(-3_600)
        )
        let task = MessageTask(
            id: "task-1",
            title: "使用 Safari 搜索并总结今日 AI 新闻",
            status: "cancelled",
            updatedAt: now.addingTimeInterval(-3_000)
        )

        let reconciled = PetActivityRecoveryMapper.applyingAuthoritativeTask(
            task,
            to: staleActivity,
            now: now
        )

        #expect(reconciled == nil)
    }

    @Test
    func authoritativeTaskStatusAndTitleOverrideRunLogStatus() throws {
        let now = Date()
        let staleActivity = PetActivity(
            id: "task-runner:task-1",
            source: .taskRunner,
            kind: .cancelled,
            title: "旧状态",
            route: PetActivityRoute(messageID: "message-1", taskID: "task-1"),
            updatedAt: now.addingTimeInterval(-60)
        )
        let task = MessageTask(
            id: "task-1",
            title: "真实任务名称",
            status: "running",
            lastRunID: "run-2",
            updatedAt: now
        )

        let reconciled = try #require(PetActivityRecoveryMapper.applyingAuthoritativeTask(
            task,
            to: staleActivity,
            now: now
        ))

        #expect(reconciled.kind == .working)
        #expect(reconciled.title == "任务「真实任务名称」正在执行")
        #expect(reconciled.route.runID == "run-2")
        #expect(reconciled.expiresAt == nil)
    }

    @Test
    func recentCompletionBridgesInboxDeliveryWithoutBecomingPermanent() throws {
        let now = Date()
        let runningActivity = PetActivity(
            id: "task-runner:task-1",
            source: .taskRunner,
            kind: .working,
            title: "任务正在执行",
            route: PetActivityRoute(messageID: "message-1", taskID: "task-1"),
            updatedAt: now.addingTimeInterval(-60)
        )
        let task = MessageTask(
            id: "task-1",
            title: "整理调研结论",
            status: "completed",
            resultSummary: "已经整理完成",
            updatedAt: now
        )

        let completed = try #require(PetActivityRecoveryMapper.applyingAuthoritativeTask(
            task,
            to: runningActivity,
            now: now.addingTimeInterval(60)
        ))

        #expect(completed.kind == .succeeded)
        #expect(completed.detail == "已经整理完成")
        #expect(completed.expiresAt != nil)
    }

    @Test
    func oldCompletionIsNotResurrectedAsUnreadPetWork() {
        let now = Date()
        let runningActivity = PetActivity(
            id: "task-runner:task-old",
            source: .taskRunner,
            kind: .working,
            title: "旧任务",
            route: PetActivityRoute(messageID: "message-old", taskID: "task-old"),
            updatedAt: now.addingTimeInterval(-86_400)
        )
        let task = MessageTask(
            id: "task-old",
            title: "旧任务",
            status: "completed",
            updatedAt: now.addingTimeInterval(-86_400)
        )

        let recovered = PetActivityRecoveryMapper.applyingAuthoritativeTask(
            task,
            to: runningActivity,
            now: now
        )

        #expect(recovered == nil)
    }

}
