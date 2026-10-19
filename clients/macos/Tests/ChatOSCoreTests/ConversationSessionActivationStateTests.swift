import Testing
@testable import ChatOSCore

@Suite("ConversationSessionActivationState")
struct ConversationSessionActivationStateTests {
    @Test
    func repeatedActivationKeepsCurrentSession() {
        var state = ConversationSessionActivationState()

        let first = state.activate()
        let second = state.activate()

        #expect(second == first)
        #expect(state.accepts(first))
    }

    @Test
    func reactivationRejectsCallbacksFromPreviousPresentation() {
        var state = ConversationSessionActivationState()
        let old = state.activate()

        state.deactivate()
        let current = state.activate()

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }
}
