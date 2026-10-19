import Testing
@testable import ChatOSCore

@Suite("LocalConnectorApprovalMonitoringState")
struct LocalConnectorApprovalMonitoringStateTests {
    @Test
    func streamingSessionUsesSlowConsistencyChecks() {
        var state = LocalConnectorApprovalMonitoringState()

        let session = state.start(hasStreamingService: true)

        #expect(session.generation == 1)
        #expect(session.shouldRefreshImmediately == false)
        #expect(session.consistencyCheckInterval == .seconds(60))
        #expect(state.accepts(session))
    }

    @Test
    func pollingSessionRefreshesImmediately() {
        var state = LocalConnectorApprovalMonitoringState()

        let session = state.start(hasStreamingService: false)

        #expect(session.shouldRefreshImmediately)
        #expect(session.consistencyCheckInterval == .seconds(2))
    }

    @Test
    func repeatedStartKeepsCurrentSession() {
        var state = LocalConnectorApprovalMonitoringState()

        let first = state.start(hasStreamingService: true)
        let second = state.start(hasStreamingService: true)

        #expect(second == first)
        #expect(state.accepts(first))
    }

    @Test
    func stopRejectsOldSessionAndRestartAdvancesGeneration() {
        var state = LocalConnectorApprovalMonitoringState()
        let oldSession = state.start(hasStreamingService: true)

        state.stop()

        #expect(!state.accepts(oldSession))
        let nextSession = state.start(hasStreamingService: true)
        #expect(nextSession.generation == 2)
        #expect(state.accepts(nextSession))
        #expect(!state.accepts(oldSession))
    }
}
