import Foundation
import Testing
@testable import ChatOSCore

struct PetActivityRecoveryMapperTests {
    @Test
    func restoresPendingPlanConfirmation() throws {
        let now = Date()
        let turn = makeTurn(
            status: .completed,
            startedAt: now.addingTimeInterval(-30),
            context: ProjectExecutionContext(
                projectID: "project-1",
                executionGroupID: "group-1",
                confirmationStatus: "awaiting_confirmation",
                overallStatus: "awaiting_confirmation"
            )
        )

        let activities = PetActivityRecoveryMapper.activities(
            conversationID: "conversation-1",
            projectID: "project-1",
            turns: [turn],
            now: now
        )

        let activity = try #require(activities.first)
        #expect(activity.id == "project-execution:group-1")
        #expect(activity.kind == .waitingForUser)
        #expect(activity.route.turnID == "turn-1")
        #expect(activity.route.runID == "group-1")
    }

    @Test
    func restoresLatestRunningTaskWithRunRoute() throws {
        let now = Date()
        var turn = makeTurn(status: .completed, startedAt: now.addingTimeInterval(-60))
        turn.assistantReplies = [
            ConversationAssistantReply(
                message: ChatMessage(
                    id: "reply-1",
                    role: .assistant,
                    text: "任务开始",
                    createdAt: now.addingTimeInterval(-10)
                ),
                taskCallback: TaskRunnerCallbackReference(
                    taskID: "task-1",
                    runID: "run-1",
                    event: "task.run.started",
                    status: "running",
                    sourceTurnID: "turn-1"
                )
            ),
        ]

        let activities = PetActivityRecoveryMapper.activities(
            conversationID: "conversation-1",
            projectID: nil,
            turns: [turn],
            now: now
        )

        let activity = try #require(activities.first)
        #expect(activity.kind == .working)
        #expect(activity.route.messageID == "reply-1")
        #expect(activity.route.taskID == "task-1")
        #expect(activity.route.runID == "run-1")
        #expect(activity.expiresAt == nil)
    }

    @Test
    func longRunningTaskRemainsVisibleUntilARealTerminalEventArrives() throws {
        let now = Date()
        var turn = makeTurn(status: .completed, startedAt: now.addingTimeInterval(-3_600))
        turn.assistantReplies = [
            ConversationAssistantReply(
                message: ChatMessage(
                    id: "reply-long-running",
                    role: .assistant,
                    text: "任务仍在执行",
                    createdAt: now.addingTimeInterval(-3_500)
                ),
                taskCallback: TaskRunnerCallbackReference(
                    taskID: "task-long-running",
                    runID: "run-long-running",
                    event: "task.run.started",
                    status: "running",
                    sourceTurnID: "turn-1"
                )
            ),
        ]

        let activities = PetActivityRecoveryMapper.activities(
            conversationID: "conversation-1",
            projectID: nil,
            turns: [turn],
            now: now
        )

        let activity = try #require(activities.first)
        #expect(activity.kind == .working)
        #expect(activity.expiresAt == nil)
    }

    @Test
    func authoritativeCancelledTaskRemovesStaleRunningCallback() {
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
    func recentLegacyCompletionBridgesInboxDeliveryWithoutBecomingPermanent() throws {
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
    func oldLegacyCompletionIsNotResurrectedAsUnreadPetWork() {
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

    @Test
    func runningExecutionGroupDoesNotBecomeAFakeTask() {
        let now = Date()
        let turn = makeTurn(
            status: .completed,
            startedAt: now.addingTimeInterval(-60),
            context: ProjectExecutionContext(
                projectID: "project-1",
                executionGroupID: "group-running",
                confirmationStatus: "confirmed",
                overallStatus: "running"
            )
        )

        let activities = PetActivityRecoveryMapper.activities(
            conversationID: "conversation-1",
            projectID: "project-1",
            turns: [turn],
            now: now
        )

        #expect(activities.isEmpty)
    }

    @Test
    func expiredCompletionDoesNotReturnAfterReconnect() {
        let now = Date()
        let turn = makeTurn(
            status: .completed,
            startedAt: now.addingTimeInterval(-120),
            completedAt: now.addingTimeInterval(-90)
        )

        let activities = PetActivityRecoveryMapper.activities(
            conversationID: "conversation-1",
            projectID: nil,
            turns: [turn],
            now: now
        )

        #expect(activities.isEmpty)
    }

    private func makeTurn(
        status: TurnStatus,
        startedAt: Date,
        completedAt: Date? = nil,
        context: ProjectExecutionContext? = nil
    ) -> ConversationTurn {
        ConversationTurn(
            id: "turn-1",
            sessionID: "conversation-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "user-1",
                role: .user,
                text: "执行任务",
                createdAt: startedAt
            ),
            projectExecutionContext: context,
            status: status,
            startedAt: startedAt,
            completedAt: completedAt
        )
    }
}
