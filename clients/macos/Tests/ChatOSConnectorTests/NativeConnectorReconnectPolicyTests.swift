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

    @Test
    func connectorCredentialRenewsAtHalfOfItsIssuedLifetime() throws {
        let issuedAt = 10_000.0
        let expiresAt = issuedAt + 12 * 60 * 60
        let payload = try JSONSerialization.data(withJSONObject: [
            "iat": issuedAt,
            "exp": expiresAt,
        ])
        let encoded = payload.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
        let token = "header.\(encoded).signature"

        #expect(NativeLocalConnectorService.connectorCredentialRenewalDelay(
            token: token,
            now: Date(timeIntervalSince1970: issuedAt)
        ) == 6 * 60 * 60)
        #expect(NativeLocalConnectorService.connectorCredentialRenewalDelay(
            token: token,
            now: Date(timeIntervalSince1970: expiresAt)
        ) == 0)
    }

    @Test
    func malformedConnectorCredentialUsesBoundedRenewalFallback() {
        #expect(NativeLocalConnectorService.connectorCredentialRenewalDelay(
            token: "not-a-jwt",
            now: Date(timeIntervalSince1970: 10_000)
        ) == 60 * 60)
    }

    @Test
    func socketFailureHandlingIsSingleFlightAndRejectsStaleSockets() {
        #expect(NativeLocalConnectorService.shouldStartGatewayConnectionFailureHandling(
            socketMatches: true,
            hasActiveHandler: false,
            cleanupInProgress: false
        ))
        #expect(!NativeLocalConnectorService.shouldStartGatewayConnectionFailureHandling(
            socketMatches: false,
            hasActiveHandler: false,
            cleanupInProgress: false
        ))
        #expect(!NativeLocalConnectorService.shouldStartGatewayConnectionFailureHandling(
            socketMatches: true,
            hasActiveHandler: true,
            cleanupInProgress: false
        ))
        #expect(!NativeLocalConnectorService.shouldStartGatewayConnectionFailureHandling(
            socketMatches: true,
            hasActiveHandler: false,
            cleanupInProgress: true
        ))
    }

    @Test
    func foregroundStatusPollingCannotReconnectDuringCredentialRecovery() {
        #expect(NativeLocalConnectorService.shouldOpenGatewayConnection(
            hasSocket: false,
            hasReconnectTask: false,
            hasFailureHandler: false,
            cleanupInProgress: false
        ))
        #expect(!NativeLocalConnectorService.shouldOpenGatewayConnection(
            hasSocket: false,
            hasReconnectTask: false,
            hasFailureHandler: true,
            cleanupInProgress: false
        ))
        #expect(!NativeLocalConnectorService.shouldOpenGatewayConnection(
            hasSocket: true,
            hasReconnectTask: false,
            hasFailureHandler: false,
            cleanupInProgress: false
        ))
        #expect(!NativeLocalConnectorService.shouldOpenGatewayConnection(
            hasSocket: false,
            hasReconnectTask: true,
            hasFailureHandler: false,
            cleanupInProgress: false
        ))
        #expect(!NativeLocalConnectorService.shouldOpenGatewayConnection(
            hasSocket: false,
            hasReconnectTask: false,
            hasFailureHandler: false,
            cleanupInProgress: true
        ))
    }

    @Test
    func reconnectLoopCannotRaceCredentialRecoveryOrSleep() {
        #expect(NativeLocalConnectorService.shouldScheduleGatewayReconnect(
            shouldMaintain: true,
            isSystemSleeping: false,
            hasDevice: true,
            hasSocket: false,
            cleanupInProgress: false,
            hasFailureHandler: false,
            hasReconnectTask: false
        ))
        #expect(!NativeLocalConnectorService.shouldScheduleGatewayReconnect(
            shouldMaintain: true,
            isSystemSleeping: false,
            hasDevice: true,
            hasSocket: false,
            cleanupInProgress: false,
            hasFailureHandler: true,
            hasReconnectTask: false
        ))
        #expect(!NativeLocalConnectorService.shouldScheduleGatewayReconnect(
            shouldMaintain: true,
            isSystemSleeping: true,
            hasDevice: true,
            hasSocket: false,
            cleanupInProgress: false,
            hasFailureHandler: false,
            hasReconnectTask: false
        ))
    }

    @Test
    func stoppedOrReplacedFailureRecoveryCannotPublishRefreshedCredentials() {
        #expect(NativeLocalConnectorService.gatewayConnectionFailureIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: false,
            socketMatches: true
        ))
        #expect(!NativeLocalConnectorService.gatewayConnectionFailureIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 5,
            isCancelled: false,
            socketMatches: true
        ))
        #expect(!NativeLocalConnectorService.gatewayConnectionFailureIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: true,
            socketMatches: true
        ))
        #expect(!NativeLocalConnectorService.gatewayConnectionFailureIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: false,
            socketMatches: false
        ))
    }

    @Test
    func gatewayBootstrapRejectsCancelledReplacedAndDisconnectedWork() {
        #expect(NativeLocalConnectorService.gatewayConnectedBootstrapIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: false,
            socketMatches: true,
            gatewayConnected: true
        ))
        #expect(!NativeLocalConnectorService.gatewayConnectedBootstrapIsCurrent(
            expectedGeneration: 3,
            currentGeneration: 4,
            isCancelled: false,
            socketMatches: true,
            gatewayConnected: true
        ))
        #expect(!NativeLocalConnectorService.gatewayConnectedBootstrapIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: true,
            socketMatches: true,
            gatewayConnected: true
        ))
        #expect(!NativeLocalConnectorService.gatewayConnectedBootstrapIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: false,
            socketMatches: false,
            gatewayConnected: true
        ))
        #expect(!NativeLocalConnectorService.gatewayConnectedBootstrapIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: false,
            socketMatches: true,
            gatewayConnected: false
        ))
    }

    @Test
    func managedRuntimeRefreshRejectsCancellationAndInvalidatedGeneration() {
        #expect(NativeLocalConnectorService.managedRuntimeConfigRefreshIsCurrent(
            expectedGeneration: 7,
            currentGeneration: 7,
            isCancelled: false
        ))
        #expect(!NativeLocalConnectorService.managedRuntimeConfigRefreshIsCurrent(
            expectedGeneration: 6,
            currentGeneration: 7,
            isCancelled: false
        ))
        #expect(!NativeLocalConnectorService.managedRuntimeConfigRefreshIsCurrent(
            expectedGeneration: 7,
            currentGeneration: 7,
            isCancelled: true
        ))
    }

    @Test
    func pluginSourceCacheRequiresMatchingGenerationAndUnexpiredSnapshot() {
        let now = Date(timeIntervalSince1970: 20_000)
        #expect(NativeLocalConnectorService.pluginSourceCacheIsUsable(
            cacheGeneration: 9,
            currentGeneration: 9,
            expiresAt: now.addingTimeInterval(1),
            now: now
        ))
        #expect(!NativeLocalConnectorService.pluginSourceCacheIsUsable(
            cacheGeneration: 8,
            currentGeneration: 9,
            expiresAt: now.addingTimeInterval(1),
            now: now
        ))
        #expect(!NativeLocalConnectorService.pluginSourceCacheIsUsable(
            cacheGeneration: 9,
            currentGeneration: 9,
            expiresAt: now,
            now: now
        ))
    }

    @Test
    func pluginSourceRefreshRejectsCancellationAndInvalidatedGeneration() {
        #expect(NativeLocalConnectorService.pluginSourceRefreshIsCurrent(
            expectedGeneration: 5,
            currentGeneration: 5,
            isCancelled: false
        ))
        #expect(!NativeLocalConnectorService.pluginSourceRefreshIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 5,
            isCancelled: false
        ))
        #expect(!NativeLocalConnectorService.pluginSourceRefreshIsCurrent(
            expectedGeneration: 5,
            currentGeneration: 5,
            isCancelled: true
        ))
    }

    @Test
    func modelCatalogCacheRequiresMatchingGenerationAndUnexpiredSnapshot() {
        let now = Date(timeIntervalSince1970: 30_000)
        #expect(NativeLocalConnectorService.modelCatalogCacheIsUsable(
            cacheGeneration: 12,
            currentGeneration: 12,
            expiresAt: now.addingTimeInterval(1),
            now: now
        ))
        #expect(!NativeLocalConnectorService.modelCatalogCacheIsUsable(
            cacheGeneration: 11,
            currentGeneration: 12,
            expiresAt: now.addingTimeInterval(1),
            now: now
        ))
        #expect(!NativeLocalConnectorService.modelCatalogCacheIsUsable(
            cacheGeneration: 12,
            currentGeneration: 12,
            expiresAt: now,
            now: now
        ))
    }

    @Test
    func modelCatalogRefreshRejectsCancellationAndInvalidatedGeneration() {
        #expect(NativeLocalConnectorService.modelCatalogRefreshIsCurrent(
            expectedGeneration: 8,
            currentGeneration: 8,
            isCancelled: false
        ))
        #expect(!NativeLocalConnectorService.modelCatalogRefreshIsCurrent(
            expectedGeneration: 7,
            currentGeneration: 8,
            isCancelled: false
        ))
        #expect(!NativeLocalConnectorService.modelCatalogRefreshIsCurrent(
            expectedGeneration: 8,
            currentGeneration: 8,
            isCancelled: true
        ))
    }

    @Test
    func pluginPermissionSnapshotRequiresMatchingGenerationAndUnexpiredValue() {
        let now = Date(timeIntervalSince1970: 40_000)
        #expect(NativePluginPermissionSnapshotCache.snapshotIsUsable(
            snapshotGeneration: 3,
            currentGeneration: 3,
            expiresAt: now.addingTimeInterval(1),
            now: now
        ))
        #expect(!NativePluginPermissionSnapshotCache.snapshotIsUsable(
            snapshotGeneration: 2,
            currentGeneration: 3,
            expiresAt: now.addingTimeInterval(1),
            now: now
        ))
        #expect(!NativePluginPermissionSnapshotCache.snapshotIsUsable(
            snapshotGeneration: 3,
            currentGeneration: 3,
            expiresAt: now,
            now: now
        ))
    }

    @Test
    func pluginPermissionRefreshRejectsCancellationAndInvalidatedGeneration() {
        #expect(NativePluginPermissionSnapshotCache.refreshIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: false
        ))
        #expect(!NativePluginPermissionSnapshotCache.refreshIsCurrent(
            expectedGeneration: 3,
            currentGeneration: 4,
            isCancelled: false
        ))
        #expect(!NativePluginPermissionSnapshotCache.refreshIsCurrent(
            expectedGeneration: 4,
            currentGeneration: 4,
            isCancelled: true
        ))
    }
}
