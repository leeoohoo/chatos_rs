import Testing
@testable import ChatOSCore

@Suite("ConversationBootstrapState")
struct ConversationBootstrapStateTests {
    @Test
    func resetRejectsBootstrapFinalizationAfterRuntimeTransition() {
        var state = ConversationBootstrapState()
        let token = state.begin()

        state.reset()
        let finished = state.finish(token)

        #expect(!state.accepts(token))
        #expect(!finished)
    }

    @Test
    func currentBootstrapCanFinishOnce() {
        var state = ConversationBootstrapState()
        let token = state.begin()

        let firstFinish = state.finish(token)
        let secondFinish = state.finish(token)

        #expect(firstFinish)
        #expect(!secondFinish)
        #expect(state.activeToken == nil)
    }
}
