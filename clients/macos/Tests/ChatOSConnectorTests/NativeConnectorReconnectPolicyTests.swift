import Testing
@testable import ChatOSConnector

struct NativeConnectorReconnectPolicyTests {
    @Test
    func reconnectBackoffStartsImmediatelyAndCapsAtThirtySeconds() {
        let delays = (0...8).map {
            NativeLocalConnectorService.gatewayReconnectDelaySeconds(afterFailedAttempts: $0)
        }

        #expect(delays == [0, 1, 2, 4, 8, 16, 30, 30, 30])
    }

    @Test
    func transientGatewayFailurePreservesPreparedPluginSessions() {
        #expect(!NativeLocalConnectorService.transientGatewayFailureTerminatesPluginSessions)
    }
}
