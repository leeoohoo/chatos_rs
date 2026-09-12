import Foundation
import Testing
@testable import ChatOSCore

struct PetActivityRecoveryMapperTests {
    @Test
    func runningTaskUsesAuthoritativeLocalIdentity() throws {
        let state = makeState(status: .modelRunning)

        let activity = try #require(PetActivityRecoveryMapper.activity(from: state))

        #expect(activity.id == "task-runner:task-1")
        #expect(activity.source == .taskRunner)
        #expect(activity.kind == .working)
        #expect(activity.route.projectID == "project-1")
        #expect(activity.route.conversationID == "session-1")
        #expect(activity.route.turnID == "turn-1")
        #expect(activity.route.taskID == "task-1")
        #expect(activity.route.runID == "run-1")
        #expect(activity.activityVersion == "run-1:3")
    }

    @Test
    func pendingLocalInteractionBecomesAskUserActivity() throws {
        var state = makeState(status: .paused)
        state.userPrompt = AskUserPrompt(
            id: "prompt-1",
            sessionID: "session-1",
            turnID: "turn-1",
            kind: "choice",
            status: .pending,
            title: "选择方案",
            message: "请选择视觉方向",
            allowsCancel: true
        )

        let activity = try #require(PetActivityRecoveryMapper.activity(from: state))

        #expect(activity.id == "ask-user:prompt-1")
        #expect(activity.source == .askUserPrompt)
        #expect(activity.kind == .waitingForUser)
        #expect(activity.route.promptID == "prompt-1")
        #expect(activity.route.taskID == "task-1")
    }

    @Test
    func completedTaskUsesLocalTerminalOutcomeAndRetention() throws {
        let now = Date(timeIntervalSince1970: 2_000)
        var state = makeState(status: .succeeded, updatedAt: "1970-01-01T00:33:00Z")
        state.run.terminalOutcome = .object(["text": .string("设计稿已完成")])

        let activity = try #require(PetActivityRecoveryMapper.activity(from: state, now: now))

        #expect(activity.kind == .succeeded)
        #expect(activity.detail == "设计稿已完成")
        #expect(activity.expiresAt == Date(timeIntervalSince1970: 2_580))
    }

    @Test
    func expiredTerminalTaskIsNotRestored() {
        let state = makeState(status: .failed, updatedAt: "1970-01-01T00:10:00Z")
        let activity = PetActivityRecoveryMapper.activity(
            from: state,
            now: Date(timeIntervalSince1970: 2_000)
        )
        #expect(activity == nil)
    }

    private func makeState(
        status: LocalAgentRunStatus,
        updatedAt: String = "2026-09-13T01:00:00Z"
    ) -> LocalAgentTaskState {
        let task = LocalAgentTaskSnapshot(
            taskID: "task-1",
            revision: 2,
            sourceThreadID: "session-1",
            sourceTurnID: "turn-1",
            projectID: "project-1",
            currentRunID: "run-1",
            runIDs: ["run-1"],
            objective: "完成网站视觉设计",
            acceptanceCriteria: ["通过视觉审查"],
            status: status.rawValue,
            modelConfigID: "model-1",
            modelConfigRevision: 1,
            createdAt: "2026-09-13T00:00:00Z",
            updatedAt: updatedAt
        )
        let run = LocalAgentRunSnapshot(
            runID: "run-1",
            profileKey: "task_runner",
            ownerUserID: "user-1",
            ownerEntityType: "task",
            ownerEntityID: "task-1",
            projectID: "project-1",
            status: status,
            version: 3,
            stepSeq: 2,
            iteration: 2,
            retryCount: 0,
            modelConfigID: "model-1",
            modelConfigRevision: 1,
            modelRuntimeSnapshot: .object([:]),
            contextStrategy: "memory_engine",
            promptRevision: "prompt-v1",
            capabilitySnapshotRef: "capability-1",
            createdAt: "2026-09-13T00:00:00Z",
            updatedAt: updatedAt
        )
        return LocalAgentTaskState(task: task, run: run)
    }
}
