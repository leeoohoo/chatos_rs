import Testing
@testable import ChatOSCore

@Suite("ConversationRuntimeSettingsMutationState")
struct ConversationRuntimeSettingsMutationStateTests {
    @Test
    func newerMutationRejectsOlderResponseAndFinish() {
        var state = ConversationRuntimeSettingsMutationState()

        let old = state.begin()
        let current = state.begin()
        let oldAccepted = state.accepts(old)
        let currentAccepted = state.accepts(current)
        let oldFinished = state.finish(old)
        let currentFinished = state.finish(current)

        #expect(current.generation == old.generation + 1)
        #expect(!oldAccepted)
        #expect(currentAccepted)
        #expect(!oldFinished)
        #expect(currentFinished)
        #expect(state.activeToken == nil)
    }

    @Test
    func resetRejectsLateMutation() {
        var state = ConversationRuntimeSettingsMutationState()
        let token = state.begin()

        state.reset()
        let finished = state.finish(token)

        #expect(!state.accepts(token))
        #expect(!finished)
    }
}
