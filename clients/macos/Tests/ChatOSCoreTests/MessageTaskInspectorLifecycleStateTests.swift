import Testing
@testable import ChatOSCore

@Suite("MessageTaskInspectorLifecycleState")
struct MessageTaskInspectorLifecycleStateTests {
    @Test
    func repeatedStartKeepsCurrentPresentationSession() {
        var state = MessageTaskInspectorLifecycleState()

        let first = state.start()
        let second = state.start()

        #expect(second == first)
        #expect(state.accepts(first))
    }

    @Test
    func stopRejectsLateWorkAndRestartAdvancesGeneration() {
        var state = MessageTaskInspectorLifecycleState()
        let old = state.start()

        state.stop()
        let current = state.start()

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }
}
