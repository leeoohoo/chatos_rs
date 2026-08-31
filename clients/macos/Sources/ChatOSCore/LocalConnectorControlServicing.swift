import Foundation

public protocol LocalConnectorPairingTicketProviding: Sendable {
    func issueLocalConnectorPairingTicket() async throws -> String
}

public protocol LocalConnectorControlServicing: Sendable {
    func fetchStatus() async throws -> LocalConnectorStatus
    func pairWithCurrentChatOSSession(deviceName: String?) async throws -> LocalConnectorStatus
    func disconnect() async throws -> LocalConnectorStatus
    func fetchRuntimeSettings() async throws -> LocalConnectorRuntimeSettings
    func updateDeveloperMode(_ enabled: Bool) async throws -> LocalConnectorRuntimeSettings
    func fetchSystemPermissions() async throws -> LocalConnectorSystemPermissions
    func requestSystemPermission(id: String) async throws -> LocalConnectorSystemPermissions
    func executeTerminal(workspaceID: String, commandLine: String, cwd: String?) async throws -> LocalConnectorTerminalResult
    func fetchCommandHistory(limit: Int) async throws -> [LocalConnectorCommandHistoryEntry]
    func clearCommandHistory() async throws
    func fetchApprovalSettings() async throws -> LocalConnectorApprovalSettings
    func updateDefaultApprovalMode(
        _ mode: LocalConnectorApprovalMode,
        riskAcknowledged: Bool
    ) async throws -> LocalConnectorApprovalSettings
    func fetchPendingApprovals() async throws -> [LocalConnectorPendingApproval]
    func resolveApproval(id: String, decision: String) async throws
    func fetchModelCatalog(refresh: Bool) async throws -> LocalConnectorModelCatalog
    func fetchModelProviders() async throws -> [LocalConnectorModelProvider]
    func createModelProvider(_ draft: LocalConnectorModelProviderDraft) async throws
    func updateModelProvider(id: String, draft: LocalConnectorModelProviderDraft) async throws
    func refreshModelProvider(id: String) async throws
    func deleteModelProvider(id: String) async throws
    func updateModelConfig(id: String, update: LocalConnectorModelConfigUpdate) async throws
    func updateModelSettings(_ settings: LocalConnectorModelSettings) async throws
    func fetchSandboxBackends() async throws -> [LocalConnectorSandboxBackend]
    func fetchSandboxSettings() async throws -> LocalConnectorSandboxSettings
    func updateSandboxSettings(
        enabled: Bool?,
        permissionProfileID: String?,
        approvalPolicy: String?,
        approvalReviewer: String?,
        networkAccess: String?
    ) async throws -> LocalConnectorSandboxSettings
    func fetchPlugins() async throws -> [LocalConnectorPlugin]
    func installPlugin(id: String) async throws
    func uninstallPlugin(id: String) async throws
    func updatePluginEnabled(id: String, enabled: Bool) async throws
    func requestPluginPermission(pluginID: String, permissionID: String) async throws
}

public extension LocalConnectorControlServicing {
    func fetchModelProviders() async throws -> [LocalConnectorModelProvider] { [] }
    func createModelProvider(_ draft: LocalConnectorModelProviderDraft) async throws {}
    func updateModelProvider(id: String, draft: LocalConnectorModelProviderDraft) async throws {}
    func refreshModelProvider(id: String) async throws {}
    func deleteModelProvider(id: String) async throws {}
    func updateModelConfig(id: String, update: LocalConnectorModelConfigUpdate) async throws {}
    func updateModelSettings(_ settings: LocalConnectorModelSettings) async throws {}
    func requestPluginPermission(pluginID: String, permissionID: String) async throws {
        _ = try await requestSystemPermission(id: permissionID)
    }
}
