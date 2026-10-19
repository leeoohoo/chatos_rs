import Testing
@testable import ChatOSCore

@Suite("ProjectConversationPreparationState")
struct ProjectConversationPreparationStateTests {
    @Test
    func projectsPrepareIndependently() {
        var state = ProjectConversationPreparationState()
        let first = state.begin(projectID: "project-a")
        let second = state.begin(projectID: "project-b")

        #expect(state.accepts(first))
        #expect(state.accepts(second))
        let didFinishFirst = state.finish(first)
        #expect(didFinishFirst)
        #expect(state.accepts(second))
    }

    @Test
    func newerRequestRejectsOlderCompletionForSameProject() {
        var state = ProjectConversationPreparationState()
        let stale = state.begin(projectID: "project-a")
        let current = state.begin(projectID: "project-a")

        #expect(current.generation == stale.generation + 1)
        #expect(!state.accepts(stale))
        #expect(state.accepts(current))
        let didFinishStale = state.finish(stale)
        #expect(!didFinishStale)
        #expect(state.accepts(current))
    }

    @Test
    func projectCancellationDoesNotInvalidateOtherProjects() {
        var state = ProjectConversationPreparationState()
        let cancelled = state.begin(projectID: "project-a")
        let current = state.begin(projectID: "project-b")

        state.cancel(projectID: "project-a")

        #expect(!state.accepts(cancelled))
        #expect(state.accepts(current))
    }

    @Test
    func resetRejectsAllAccountScopedWork() {
        var state = ProjectConversationPreparationState()
        let first = state.begin(projectID: "project-a")
        let second = state.begin(projectID: "project-b")

        state.reset()

        #expect(!state.accepts(first))
        #expect(!state.accepts(second))
        #expect(state.activeTokens.isEmpty)
    }
}
