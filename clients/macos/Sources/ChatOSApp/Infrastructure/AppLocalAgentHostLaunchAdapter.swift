import ChatOSAPI
import ChatOSConnector
import ChatOSCore

/// Composition adapter for the Local Agent launch use case.
///
/// It is intentionally owned by ChatOSApp because the launch path joins the API access-token
/// source with native Connector actors. The Application use case sees only its typed port.
@MainActor
struct AppLocalAgentHostLaunchAdapter: LocalAgentHostLaunchPort {
    let host: any LocalAgentHostLifecycleServicing
    let eventHub: NativeLocalAgentEventHub?
    let apiClient: ChatOSAPIClient
    let localConnectorService: NativeLocalConnectorService
    let serviceConfigurationFactory: NativeLocalAgentServiceConfigurationFactory
    let workspaceService: NativeLocalAgentWorkspaceService?
    let projectConversationService: NativeLocalAgentProjectConversationService?
    let notepadService: NativeLocalAgentNotepadService
    let remoteConnectionMetadataService: NativeLocalAgentRemoteConnectionMetadataService
    let isCurrent: () -> Bool

    func resetEventHub() async {
        await eventHub?.reset()
    }

    func startHost(ownerUserID: String) async throws {
        if let nativeHost = host as? NativeLocalAgentHostLifecycle {
            try await nativeHost.startForBootstrap(ownerUserID: ownerUserID)
        } else {
            try await host.start(ownerUserID: ownerUserID)
        }
    }

    func restoreLocalServices(ownerUserID: String) async throws -> Bool {
        guard let nativeHost = host as? NativeLocalAgentHostLifecycle else { return false }
        let memoryAccessToken = await apiClient.currentAccessToken()
        guard let restored = try await localConnectorService.restoreLocalAgentHost(
            nativeHost,
            ownerUserID: ownerUserID,
            memoryAccessToken: memoryAccessToken
        ) else { return false }
        try await LocalAgentServiceConfigurationUseCase().execute(
            port: serviceConfigurationFactory.makePort(
                ownerUserID: ownerUserID,
                bootstrap: restored
            ),
            isCurrent: isCurrent
        )
        return true
    }

    func configureWorkspace(ownerUserID: String) async {
        await workspaceService?.configure(ownerUserID: ownerUserID)
    }

    func configureProjectConversation(ownerUserID: String) async {
        await projectConversationService?.configure(ownerUserID: ownerUserID)
    }

    func configureNotepad(ownerUserID: String) async {
        await notepadService.configure(ownerUserID: ownerUserID)
    }

    func configureRemoteConnectionMetadata(ownerUserID: String) async {
        await remoteConnectionMetadataService.configure(ownerUserID: ownerUserID)
    }
}
