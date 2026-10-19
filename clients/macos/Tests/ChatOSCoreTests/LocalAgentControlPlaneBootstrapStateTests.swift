import Testing
@testable import ChatOSCore

@Suite("Local Agent control-plane bootstrap")
struct LocalAgentControlPlaneBootstrapStateTests {
    @Test
    func requestAccountAndLifecycleIdentityMustAllMatch() {
        var state = LocalAgentControlPlaneBootstrapState()
        let old = state.begin(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        )
        let current = state.begin(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        )

        #expect(!state.accepts(
            old,
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        ))
        #expect(state.accepts(
            current,
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        ))
        #expect(!state.accepts(
            current,
            ownerUserID: "owner-2",
            accountGeneration: 2,
            lifecycleGeneration: 11
        ))
    }

    @Test
    func finishAndResetRejectActiveRequest() {
        var state = LocalAgentControlPlaneBootstrapState()
        let finished = state.begin(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        )

        let didFinish = state.finish(finished)
        #expect(didFinish)
        #expect(!state.accepts(
            finished,
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        ))

        let reset = state.begin(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        )
        state.reset()

        #expect(!state.accepts(
            reset,
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        ))
    }
}
