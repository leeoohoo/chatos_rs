import ChatOSCore
import Testing

struct LocalAgentRuntimeStateTests {
    @Test
    func runtimeIsReadyOnlyAfterTheSameOwnerCompletesSynchronization() {
        var state = LocalAgentRuntimeState()

        state.beginLaunch(ownerUserID: "owner-a", generation: 1)
        #expect(state.phase == .launching)
        #expect(!state.isReady(for: "owner-a"))

        state.beginHandshake(ownerUserID: "owner-a", generation: 1)
        #expect(state.phase == .handshaking)

        state.beginSynchronization(ownerUserID: "owner-a", generation: 1)
        #expect(state.phase == .synchronizing)

        state.becomeReady(ownerUserID: "owner-a", generation: 1)
        #expect(state.phase == .ready)
        #expect(state.isReady(for: "owner-a"))
        #expect(!state.isReady(for: "owner-b"))
        #expect(!state.isReady(for: nil))
    }

    @Test
    func staleGenerationAndWrongOwnerCannotOverwriteCurrentAccount() {
        var state = LocalAgentRuntimeState()
        state.beginLaunch(ownerUserID: "owner-a", generation: 4)
        state.beginLaunch(ownerUserID: "owner-b", generation: 5)

        state.becomeReady(ownerUserID: "owner-a", generation: 4)
        state.becomeReady(ownerUserID: "owner-a", generation: 5)

        #expect(state.phase == .launching)
        #expect(state.ownerUserID == "owner-b")
        #expect(state.generation == 5)
    }

    @Test
    func recoveryAndStopPreserveGenerationOrdering() {
        var state = LocalAgentRuntimeState()
        state.beginLaunch(ownerUserID: "owner-a", generation: 7)
        state.scheduleRestart(
            ownerUserID: "owner-a",
            generation: 7,
            attempt: 2,
            errorMessage: "transport closed"
        )

        #expect(state.phase == .restarting)
        #expect(state.recoveryAttempt == 2)
        #expect(state.errorMessage == "transport closed")

        state.stop(generation: 6)
        #expect(state.phase == .restarting)

        state.stop(generation: 8)
        #expect(state == LocalAgentRuntimeState(generation: 8))
    }

    @Test
    func degradedStateRequiresTheActiveGenerationAndOwner() {
        var state = LocalAgentRuntimeState()
        state.beginLaunch(ownerUserID: "owner-a", generation: 3)
        state.becomeDegraded(
            ownerUserID: "owner-b",
            generation: 3,
            errorMessage: "wrong account"
        )
        state.becomeDegraded(
            ownerUserID: "owner-a",
            generation: 2,
            errorMessage: "stale"
        )

        #expect(state.phase == .launching)

        state.becomeDegraded(
            ownerUserID: "owner-a",
            generation: 3,
            errorMessage: "host failed"
        )
        #expect(state.phase == .degraded)
        #expect(state.errorMessage == "host failed")
        #expect(!state.isReady(for: "owner-a"))
    }
}
