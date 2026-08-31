import Foundation
import Testing
@testable import ChatOSCore

struct PetStateReducerTests {
    @Test
    func approvalOutranksWorkAndPersists() {
        let now = Date()
        var reducer = PetStateReducer()
        reducer.apply(.upsert(.init(
            id: "work-1",
            source: .chat,
            kind: .working,
            title: "执行中",
            updatedAt: now
        )))
        reducer.apply(.upsert(.init(
            id: "approval-1",
            source: .localApproval,
            kind: .waitingForApproval,
            title: "等待审批",
            updatedAt: now.addingTimeInterval(1)
        )))

        let presentation = reducer.presentation(at: now.addingTimeInterval(2))
        #expect(presentation.animationState == .waiting)
        #expect(presentation.primaryActivity?.id == "approval-1")
        #expect(presentation.activeWorkCount == 1)
        #expect(presentation.attentionCount == 1)
    }

    @Test
    func transientSuccessExpiresBackToWork() {
        let now = Date()
        var reducer = PetStateReducer()
        reducer.apply(.upsert(.init(
            id: "work-1",
            source: .taskRunner,
            kind: .working,
            title: "任务执行中",
            updatedAt: now
        )))
        reducer.apply(.upsert(.init(
            id: "success-1",
            source: .taskRunner,
            kind: .succeeded,
            title: "任务完成",
            updatedAt: now.addingTimeInterval(1),
            expiresAt: now.addingTimeInterval(5)
        )))

        #expect(reducer.presentation(at: now.addingTimeInterval(2)).animationState == .succeeded)
        reducer.removeExpired(at: now.addingTimeInterval(6))
        #expect(reducer.presentation(at: now.addingTimeInterval(6)).animationState == .running)
    }

    @Test
    func transientApprovalResultBrieflySurfacesAbovePendingApproval() {
        let now = Date()
        var reducer = PetStateReducer()
        reducer.apply(.upsert(.init(
            id: "approval-1",
            source: .localApproval,
            kind: .waitingForApproval,
            title: "另一个操作等待审批",
            updatedAt: now
        )))
        reducer.apply(.upsert(.init(
            id: "approval-result-1",
            source: .localApproval,
            kind: .succeeded,
            title: "AI 已批准本机操作",
            updatedAt: now.addingTimeInterval(1),
            expiresAt: now.addingTimeInterval(5)
        )))

        #expect(reducer.presentation(at: now.addingTimeInterval(2)).primaryActivity?.id == "approval-result-1")
        reducer.removeExpired(at: now.addingTimeInterval(6))
        #expect(reducer.presentation(at: now.addingTimeInterval(6)).primaryActivity?.id == "approval-1")
    }

    @Test
    func duplicateEventIDDoesNotOverwriteNewerActivity() {
        let now = Date()
        var reducer = PetStateReducer()
        reducer.apply(.upsert(.init(
            id: "task-1",
            source: .taskRunner,
            kind: .working,
            title: "第一次",
            eventID: "event-1",
            eventSequence: 1,
            updatedAt: now
        )))
        reducer.apply(.upsert(.init(
            id: "task-1",
            source: .taskRunner,
            kind: .failed,
            title: "重复事件",
            eventID: "event-1",
            eventSequence: 2,
            updatedAt: now.addingTimeInterval(1)
        )))

        #expect(reducer.presentation(at: now.addingTimeInterval(2)).primaryActivity?.title == "第一次")
    }

    @Test
    func replacingApprovalSnapshotRemovesResolvedApprovals() {
        var reducer = PetStateReducer()
        reducer.replace(source: .localApproval, with: [
            .init(
                id: "approval-1",
                source: .localApproval,
                kind: .waitingForApproval,
                title: "等待审批"
            ),
        ])
        reducer.replace(source: .localApproval, with: [])

        #expect(reducer.presentation().animationState == .idle)
    }

    @Test
    func removingProcessAndCompletionKindsPreservesAttentionItems() {
        var reducer = PetStateReducer()
        reducer.apply(.upsert(.init(
            id: "work-1",
            source: .chat,
            kind: .working,
            title: "执行中"
        )))
        reducer.apply(.upsert(.init(
            id: "success-1",
            source: .taskRunner,
            kind: .succeeded,
            title: "已完成"
        )))
        reducer.apply(.upsert(.init(
            id: "approval-1",
            source: .localApproval,
            kind: .waitingForApproval,
            title: "等待审批"
        )))

        reducer.remove(kinds: [.working, .reviewing, .succeeded])

        let presentation = reducer.presentation()
        #expect(presentation.primaryActivity?.id == "approval-1")
        #expect(presentation.activeWorkCount == 0)
        #expect(presentation.attentionCount == 1)
    }

    @Test
    func failureDoesNotIncreasePendingActionBadge() {
        var reducer = PetStateReducer()
        reducer.apply(.upsert(.init(
            id: "failure-1",
            source: .taskRunner,
            kind: .failed,
            title: "任务失败"
        )))
        reducer.apply(.upsert(.init(
            id: "blocked-1",
            source: .taskBoard,
            kind: .blocked,
            title: "任务阻塞"
        )))

        let presentation = reducer.presentation()
        #expect(presentation.attentionCount == 0)
        #expect(presentation.primaryActivity?.kind == .blocked)
    }

    @Test
    func visibleActivitiesKeepRunningWorkAlongsideApproval() {
        var reducer = PetStateReducer()
        reducer.apply(.upsert(.init(
            id: "work-1",
            source: .taskRunner,
            kind: .working,
            title: "正在执行"
        )))
        reducer.apply(.upsert(.init(
            id: "approval-1",
            source: .localApproval,
            kind: .waitingForApproval,
            title: "等待审批"
        )))

        let activities = reducer.visibleActivities()
        #expect(activities.map(\.id) == ["approval-1", "work-1"])
    }

    @Test
    func specificTaskHidesDuplicateConversationLevelProgress() {
        var reducer = PetStateReducer()
        let route = PetActivityRoute(conversationID: "conversation-1")
        reducer.apply(.upsert(.init(
            id: "chat-progress",
            source: .chat,
            kind: .working,
            title: "AI 正在处理任务",
            route: route
        )))
        reducer.apply(.upsert(.init(
            id: "execution-progress",
            source: .projectExecution,
            kind: .working,
            title: "执行计划正在运行",
            route: route
        )))
        reducer.apply(.upsert(.init(
            id: "task-progress",
            source: .taskRunner,
            kind: .working,
            title: "任务正在执行",
            route: route
        )))

        let activities = reducer.visibleActivities()
        #expect(activities.map(\.id) == ["task-progress"])
        #expect(reducer.presentation().activeWorkCount == 1)
    }
}
