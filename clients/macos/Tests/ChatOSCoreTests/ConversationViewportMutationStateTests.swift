import Testing
@testable import ChatOSCore

@Suite("ConversationViewportMutationState")
struct ConversationViewportMutationStateTests {
    @Test
    func newerAnchorRejectsOlderAnchorCompletion() {
        var state = ConversationViewportMutationState()

        let old = state.begin(.timelineAnchor)
        let current = state.begin(.timelineAnchor)

        #expect(!state.accepts(old))
        #expect(state.accepts(current))
        #expect(current.generation == old.generation + 1)
    }

    @Test
    func anchorAndMarkReadUseIndependentLanes() {
        var state = ConversationViewportMutationState()

        let anchor = state.begin(.timelineAnchor)
        let markRead = state.begin(.markNewerContentRead)
        let anchorFinished = state.finish(anchor)

        #expect(anchorFinished)
        #expect(!state.accepts(anchor))
        #expect(state.accepts(markRead))
    }

    @Test
    func resetRejectsEveryLateViewportMutation() {
        var state = ConversationViewportMutationState()
        let anchor = state.begin(.timelineAnchor)
        let markRead = state.begin(.markNewerContentRead)

        state.reset()

        #expect(!state.accepts(anchor))
        #expect(!state.accepts(markRead))
        #expect(state.activeTokens.isEmpty)
    }
}
