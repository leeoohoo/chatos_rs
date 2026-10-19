import Testing
@testable import ChatOSCore

@Suite("Workspace load")
struct WorkspaceLoadStateTests {
    @Test
    func newerRequestAndAccountChangeRejectOlderToken() {
        var state = WorkspaceLoadState()
        let old = state.begin(ownerUserID: "owner-1", accountGeneration: 1)
        let current = state.begin(ownerUserID: "owner-1", accountGeneration: 1)

        #expect(!state.accepts(
            old,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
        #expect(state.accepts(
            current,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
        #expect(!state.accepts(
            current,
            ownerUserID: "owner-2",
            accountGeneration: 2
        ))
    }

    @Test
    func finishAndResetRejectActiveRequest() {
        var state = WorkspaceLoadState()
        let finished = state.begin(ownerUserID: "owner-1", accountGeneration: 1)

        let didFinish = state.finish(finished)
        #expect(didFinish)
        #expect(!state.accepts(
            finished,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))

        let reset = state.begin(ownerUserID: "owner-1", accountGeneration: 1)
        state.reset()

        #expect(!state.accepts(
            reset,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
    }
}
