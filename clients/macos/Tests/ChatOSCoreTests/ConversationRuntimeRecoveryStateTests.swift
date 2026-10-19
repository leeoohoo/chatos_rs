import Testing
@testable import ChatOSCore

@Suite("ConversationRuntimeRecoveryState")
struct ConversationRuntimeRecoveryStateTests {
    @Test
    func newerReadySignalRejectsOlderRecovery() {
        var state = ConversationRuntimeRecoveryState()

        let old = state.begin()
        let current = state.begin()

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }

    @Test
    func staleRecoveryCannotFinishCurrentRecovery() {
        var state = ConversationRuntimeRecoveryState()
        let old = state.begin()
        let current = state.begin()

        let oldFinished = state.finish(old)
        let currentFinished = state.finish(current)

        #expect(!oldFinished)
        #expect(currentFinished)
        #expect(state.activeToken == nil)
    }

    @Test
    func resetRejectsLateRecovery() {
        var state = ConversationRuntimeRecoveryState()
        let token = state.begin()

        state.reset()
        let finished = state.finish(token)

        #expect(!state.accepts(token))
        #expect(!finished)
    }
}
