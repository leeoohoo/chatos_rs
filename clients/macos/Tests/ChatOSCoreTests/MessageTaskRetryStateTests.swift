import Testing
@testable import ChatOSCore

@Suite("MessageTaskRetryState")
struct MessageTaskRetryStateTests {
    @Test
    func newerRetryRejectsOlderResponse() {
        var state = MessageTaskRetryState()

        let old = state.begin(contextID: "task-1:run-1")
        let current = state.begin(contextID: "task-2:run-2")

        #expect(!state.accepts(old))
        #expect(state.accepts(current))
        #expect(current.generation == old.generation + 1)
    }

    @Test
    func staleRetryCannotFinishCurrentLoadingOwner() {
        var state = MessageTaskRetryState()
        let old = state.begin(contextID: "task-1:run-1")
        let current = state.begin(contextID: "task-1:run-2")

        let oldFinished = state.finish(old)
        let currentFinished = state.finish(current)

        #expect(!oldFinished)
        #expect(currentFinished)
        #expect(state.activeToken == nil)
    }

    @Test
    func resetRejectsLateRetry() {
        var state = MessageTaskRetryState()
        let token = state.begin(contextID: "task-1:run-1")

        state.reset()
        let finished = state.finish(token)

        #expect(!state.accepts(token))
        #expect(!finished)
    }
}
