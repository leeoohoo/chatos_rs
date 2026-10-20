import ChatOSAPI
import ChatOSConnector
import ChatOSCore

/// Composition adapter joining authenticated API credentials with the native Host bootstrap.
@MainActor
struct AppLocalAgentControlPlaneBootstrapAdapter: LocalAgentControlPlaneBootstrapPort {
    let host: NativeLocalAgentHostLifecycle
    let apiClient: ChatOSAPIClient
    let localConnectorService: NativeLocalConnectorService
    let serviceConfigurationFactory: NativeLocalAgentServiceConfigurationFactory
    let isCurrent: () -> Bool
    let shouldStopInvalidatedHost: () -> Bool

    func bootstrap(ownerUserID: String) async throws {
        try validateCurrent()
        let memoryAccessToken = await apiClient.currentAccessToken()
        try validateCurrent()
        let bootstrap = try await localConnectorService.bootstrapLocalAgentHost(
            host,
            ownerUserID: ownerUserID,
            memoryAccessToken: memoryAccessToken
        )
        guard !Task.isCancelled, isCurrent() else {
            if shouldStopInvalidatedHost() {
                await host.stop()
            }
            throw CancellationError()
        }
        try await LocalAgentServiceConfigurationUseCase().execute(
            port: serviceConfigurationFactory.makePort(
                ownerUserID: ownerUserID,
                bootstrap: bootstrap
            ),
            isCurrent: isCurrent
        )
    }

    private func validateCurrent() throws {
        guard !Task.isCancelled, isCurrent() else { throw CancellationError() }
    }
}
