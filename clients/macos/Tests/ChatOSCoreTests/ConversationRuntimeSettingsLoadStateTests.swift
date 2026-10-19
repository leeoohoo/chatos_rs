import Testing
@testable import ChatOSCore

@Suite("ConversationRuntimeSettingsLoadState")
struct ConversationRuntimeSettingsLoadStateTests {
    @Test
    func newerLoadRejectsOlderSnapshotAndFinish() {
        var state = ConversationRuntimeSettingsLoadState()
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
    }

    @Test
    func resetRejectsSnapshotThatPredatesMutationOrDisconnect() {
        var state = ConversationRuntimeSettingsLoadState()
        let stale = state.begin()

        state.reset()

        #expect(!state.accepts(stale))
        let staleFinished = state.finish(stale)
        #expect(!staleFinished)
    }
}
