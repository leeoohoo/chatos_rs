import Testing
@testable import ChatOSCore

@Suite("MessageTaskWorkspaceLifecycleState")
struct MessageTaskWorkspaceLifecycleStateTests {
    @Test
    func repeatedStartKeepsCurrentPresentationSession() {
        var state = MessageTaskWorkspaceLifecycleState()

        let first = state.start()
        let second = state.start()

        #expect(second == first)
        #expect(state.accepts(first))
    }

    @Test
    func stopRejectsLateWorkAndRestartAdvancesGeneration() {
        var state = MessageTaskWorkspaceLifecycleState()
        let old = state.start()

        state.stop()
        let current = state.start()

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }
}
