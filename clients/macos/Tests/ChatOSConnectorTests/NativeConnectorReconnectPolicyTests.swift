import Foundation
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

    @Test
    func requestScopedGatewayErrorsDoNotDropTheControlChannel() {
        for code in [
            "plugin_installation_status_rejected",
            "plugin_oauth_status_rejected",
            "invalid_relay_response",
        ] {
            #expect(NativeLocalConnectorService.isRecoverableGatewayProtocolError(code))
        }
        #expect(!NativeLocalConnectorService.isRecoverableGatewayProtocolError(
            "connector_session_lease_lost"
        ))
        #expect(!NativeLocalConnectorService.isRecoverableGatewayProtocolError(nil))
    }

    @Test
    func connectorCredentialRefreshIsRateLimited() {
        let now = Date(timeIntervalSince1970: 10_000)

        #expect(NativeLocalConnectorService.shouldAttemptConnectorCredentialRefresh(
            lastAttempt: nil,
            now: now
        ))
        #expect(!NativeLocalConnectorService.shouldAttemptConnectorCredentialRefresh(
            lastAttempt: now.addingTimeInterval(-59),
            now: now
        ))
        #expect(NativeLocalConnectorService.shouldAttemptConnectorCredentialRefresh(
            lastAttempt: now.addingTimeInterval(-60),
            now: now
        ))
    }
}
