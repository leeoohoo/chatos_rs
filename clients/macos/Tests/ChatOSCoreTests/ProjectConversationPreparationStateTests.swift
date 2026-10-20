import Testing
@testable import ChatOSCore

@Suite("ProjectConversationPreparationState")
struct ProjectConversationPreparationStateTests {
    @Test
    func projectsPrepareIndependently() {
        var state = ProjectConversationPreparationState()
        let first = state.begin(projectID: "project-a", ownerUserID: "owner", accountGeneration: 1)
        let second = state.begin(projectID: "project-b", ownerUserID: "owner", accountGeneration: 1)

        #expect(state.phase(for: "project-a") == .preparing)
        #expect(state.phase(for: "project-b") == .preparing)
        #expect(state.accepts(first, ownerUserID: "owner", accountGeneration: 1))
        #expect(state.accepts(second, ownerUserID: "owner", accountGeneration: 1))
        let didFinishFirst = state.finish(first)
        #expect(didFinishFirst)
        #expect(state.phase(for: "project-a") == .idle)
        #expect(state.accepts(second, ownerUserID: "owner", accountGeneration: 1))
    }

    @Test
    func newerRequestRejectsOlderCompletionForSameProject() {
        var state = ProjectConversationPreparationState()
        let stale = state.begin(projectID: "project-a", ownerUserID: "owner", accountGeneration: 1)
        let current = state.begin(projectID: "project-a", ownerUserID: "owner", accountGeneration: 1)

        #expect(current.generation == stale.generation + 1)
        #expect(!state.accepts(stale, ownerUserID: "owner", accountGeneration: 1))
        #expect(state.accepts(current, ownerUserID: "owner", accountGeneration: 1))
        let didFinishStale = state.finish(stale)
        #expect(!didFinishStale)
        #expect(state.accepts(current, ownerUserID: "owner", accountGeneration: 1))
    }

    @Test
    func projectCancellationDoesNotInvalidateOtherProjects() {
        var state = ProjectConversationPreparationState()
        let cancelled = state.begin(projectID: "project-a", ownerUserID: "owner", accountGeneration: 1)
        let current = state.begin(projectID: "project-b", ownerUserID: "owner", accountGeneration: 1)

        state.cancel(projectID: "project-a")

        #expect(!state.accepts(cancelled, ownerUserID: "owner", accountGeneration: 1))
        #expect(state.accepts(current, ownerUserID: "owner", accountGeneration: 1))
    }

    @Test
    func resetRejectsAllAccountScopedWork() {
        var state = ProjectConversationPreparationState()
        let first = state.begin(projectID: "project-a", ownerUserID: "owner", accountGeneration: 1)
        let second = state.begin(projectID: "project-b", ownerUserID: "owner", accountGeneration: 1)

        state.reset()

        #expect(!state.accepts(first, ownerUserID: "owner", accountGeneration: 1))
        #expect(!state.accepts(second, ownerUserID: "owner", accountGeneration: 1))
        #expect(state.activeTokens.isEmpty)
        #expect(state.phases.isEmpty)
    }

    @Test
    func failureRemainsUntilExplicitRetryOrCancellation() {
        var state = ProjectConversationPreparationState()
        let failed = state.begin(projectID: "project-a", ownerUserID: "owner", accountGeneration: 1)

        let didFinish = state.finish(failed, errorMessage: "Host unavailable")
        #expect(didFinish)
        #expect(state.phase(for: "project-a") == .failed(message: "Host unavailable"))

        state.clearFailure(projectID: "project-a")
        #expect(state.phase(for: "project-a") == .idle)

        _ = state.begin(projectID: "project-a", ownerUserID: "owner", accountGeneration: 1)
        state.cancel(projectID: "project-a")
        #expect(state.phase(for: "project-a") == .idle)
    }

    @Test
    func accountIdentityRejectsARequestFromAnOldSession() {
        var state = ProjectConversationPreparationState()
        let token = state.begin(
            projectID: "project-a",
            ownerUserID: "owner-a",
            accountGeneration: 7
        )

        #expect(!state.accepts(token, ownerUserID: "owner-b", accountGeneration: 7))
        #expect(!state.accepts(token, ownerUserID: "owner-a", accountGeneration: 8))
        #expect(state.accepts(token, ownerUserID: "owner-a", accountGeneration: 7))
    }
}
