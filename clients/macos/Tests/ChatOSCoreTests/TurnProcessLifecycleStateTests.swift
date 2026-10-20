import Testing
@testable import ChatOSCore

@Suite("TurnProcessLifecycleState")
struct TurnProcessLifecycleStateTests {
    @Test
    func replacementRejectsLateResult() {
        var state = TurnProcessLifecycleState()
        let old = state.begin(sessionID: "session-1", turnID: "turn-1")
        let current = state.begin(sessionID: "session-1", turnID: "turn-1")

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }

    @Test
    func resetRejectsVisibleRequest() {
        var state = TurnProcessLifecycleState()
        let token = state.begin(sessionID: "session-1", turnID: "turn-1")

        state.reset()

        #expect(!state.accepts(token))
        #expect(state.activeToken == nil)
    }

    @Test
    func staleRequestCannotFinishReplacement() {
        var state = TurnProcessLifecycleState()
        let old = state.begin(sessionID: "session-1", turnID: "turn-1")
        let current = state.begin(sessionID: "session-1", turnID: "turn-1")

        let didFinishOld = state.finish(old)
        let didFinishCurrent = state.finish(current)

        #expect(!didFinishOld)
        #expect(didFinishCurrent)
        #expect(state.activeToken == nil)
    }
}
