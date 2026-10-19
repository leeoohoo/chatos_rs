import Testing
@testable import ChatOSCore

@Suite("LocalConnectorApprovalLoadState")
struct LocalConnectorApprovalLoadStateTests {
    @Test
    func newerRequestRejectsOlderResult() {
        var state = LocalConnectorApprovalLoadState()

        let old = state.begin()
        let current = state.begin()

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }

    @Test
    func cancelRejectsLateResult() {
        var state = LocalConnectorApprovalLoadState()
        let token = state.begin()

        state.cancel()

        #expect(!state.accepts(token))
        #expect(state.activeToken == nil)
    }

    @Test
    func onlyCurrentRequestCanFinish() {
        var state = LocalConnectorApprovalLoadState()
        let old = state.begin()
        let current = state.begin()

        let didFinishOld = state.finish(old)
        #expect(!didFinishOld)
        #expect(state.accepts(current))
        let didFinishCurrent = state.finish(current)
        #expect(didFinishCurrent)
        #expect(state.activeToken == nil)
    }
}
