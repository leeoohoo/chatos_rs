import Testing
@testable import ChatOSCore

@Suite("ConversationSendLifecycleState")
struct ConversationSendLifecycleStateTests {
    @Test
    func newerSendRejectsOlderCompletion() {
        var state = ConversationSendLifecycleState()

        let old = state.begin()
        let current = state.begin()

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }

    @Test
    func staleSendCannotFinishCurrentLoadingOwner() {
        var state = ConversationSendLifecycleState()
        let old = state.begin()
        let current = state.begin()

        let oldFinished = state.finish(old)
        let currentFinished = state.finish(current)

        #expect(!oldFinished)
        #expect(currentFinished)
        #expect(state.activeToken == nil)
    }

    @Test
    func resetRejectsLateHostResponse() {
        var state = ConversationSendLifecycleState()
        let token = state.begin()

        state.reset()
        let finished = state.finish(token)

        #expect(!state.accepts(token))
        #expect(!finished)
    }
}
