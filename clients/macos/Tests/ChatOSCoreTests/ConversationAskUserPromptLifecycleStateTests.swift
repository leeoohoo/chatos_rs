import Testing
@testable import ChatOSCore

@Suite("ConversationAskUserPromptLifecycleState")
struct ConversationAskUserPromptLifecycleStateTests {
    @Test
    func newerRefreshRejectsOlderSnapshot() {
        var state = ConversationAskUserPromptLifecycleState()

        let old = state.beginRefresh()
        let current = state.beginRefresh()

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }

    @Test
    func actionInvalidatesOlderRefreshAndIsExclusivePerPrompt() throws {
        var state = ConversationAskUserPromptLifecycleState()
        let refresh = state.beginRefresh()
        let started = state.beginAction(promptID: "prompt-1")
        let action = try #require(started)
        let duplicate = state.beginAction(promptID: "prompt-1")

        #expect(!state.accepts(refresh))
        #expect(duplicate == nil)
        #expect(state.accepts(action))
        #expect(state.activePromptIDs == ["prompt-1"])
    }

    @Test
    func unrelatedPromptActionsRemainIndependent() throws {
        var state = ConversationAskUserPromptLifecycleState()
        let firstStarted = state.beginAction(promptID: "prompt-1")
        let secondStarted = state.beginAction(promptID: "prompt-2")
        let first = try #require(firstStarted)
        let second = try #require(secondStarted)

        let didFinishFirst = state.finishAction(first)

        #expect(didFinishFirst)
        #expect(!state.accepts(first))
        #expect(state.accepts(second))
        #expect(state.activePromptIDs == ["prompt-2"])
    }

    @Test
    func actionCompletionRejectsSnapshotStartedDuringMutation() throws {
        var state = ConversationAskUserPromptLifecycleState()
        let started = state.beginAction(promptID: "prompt-1")
        let action = try #require(started)
        let refresh = state.beginRefresh()

        let didFinish = state.finishAction(action)

        #expect(didFinish)
        #expect(!state.accepts(refresh))
    }

    @Test
    func resetRejectsAllLateResults() throws {
        var state = ConversationAskUserPromptLifecycleState()
        let refresh = state.beginRefresh()
        let started = state.beginAction(promptID: "prompt-1")
        let action = try #require(started)

        state.reset()

        #expect(!state.accepts(refresh))
        #expect(!state.accepts(action))
        #expect(state.activePromptIDs.isEmpty)
    }
}
