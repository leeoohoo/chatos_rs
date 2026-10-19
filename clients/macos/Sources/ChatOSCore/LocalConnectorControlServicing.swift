import Foundation

public protocol LocalConnectorPairingTicketProviding: Sendable {
    func issueLocalConnectorPairingTicket() async throws -> String
}

public protocol LocalConnectorApprovalServicing: Sendable {
    func fetchApprovalSettings() async throws -> LocalConnectorApprovalSettings
    func updateDefaultApprovalMode(
        _ mode: LocalConnectorApprovalMode,
        riskAcknowledged: Bool
    ) async throws -> LocalConnectorApprovalSettings
    func fetchPendingApprovals() async throws -> [LocalConnectorPendingApproval]
    func resolveApproval(id: String, decision: String) async throws
}

public protocol LocalConnectorControlServicing: LocalConnectorApprovalServicing {
    func fetchStatus() async throws -> LocalConnectorStatus
    func pairWithCurrentChatOSSession(deviceName: String?) async throws -> LocalConnectorStatus
    /// Stops account-scoped runtime activity without deleting persisted local state.
    /// Authentication loss and app replacement must not be treated as an explicit unpair.
    func suspendForSignedOut() async
    func resumeServerAccess() async throws -> LocalConnectorStatus
    func disconnect() async throws -> LocalConnectorStatus
    func fetchRuntimeSettings() async throws -> LocalConnectorRuntimeSettings
    func updateDeveloperMode(_ enabled: Bool) async throws -> LocalConnectorRuntimeSettings
    func fetchSystemPermissions() async throws -> LocalConnectorSystemPermissions
    func requestSystemPermission(id: String) async throws -> LocalConnectorSystemPermissions
    func executeTerminal(workspaceID: String, commandLine: String, cwd: String?) async throws -> LocalConnectorTerminalResult
    func fetchCommandHistory(limit: Int) async throws -> [LocalConnectorCommandHistoryEntry]
    func clearCommandHistory() async throws
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
    func fetchPlugins(refresh: Bool) async throws -> [LocalConnectorPlugin]
    func fetchPluginApplications() async throws -> [LocalConnectorPluginApplication]
    func launchPluginApplication(
        pluginID: String,
        componentKey: String,
        context: LocalConnectorPluginApplicationContext?,
        expectedOwnerUserID: String
    ) async throws -> LocalConnectorPluginApplicationLaunch
    func installPlugin(id: String) async throws
    func inspectCustomMCP(_ draft: LocalConnectorCustomMCPDraft) async throws -> LocalConnectorPluginInspection
    func installCustomMCP(
        _ draft: LocalConnectorCustomMCPDraft,
        expectedFingerprint: String
    ) async throws
    func inspectLocalPlugin(atPath path: String) async throws -> LocalConnectorPluginInspection
    func installLocalPlugin(atPath path: String, expectedFingerprint: String) async throws
    func inspectPluginSource(
        _ source: LocalConnectorPluginSourceDraft
    ) async throws -> LocalConnectorPluginInspection
    func installPluginSource(
        _ source: LocalConnectorPluginSourceDraft,
        expectedFingerprint: String
    ) async throws
    func uninstallPlugin(id: String) async throws
    func updatePluginEnabled(id: String, enabled: Bool) async throws
    func requestPluginPermission(pluginID: String, permissionID: String) async throws
    func isBrowserExtensionPaired(pluginID: String) async throws -> Bool
    func startBrowserExtensionPairing(pluginID: String) async throws
}

public extension LocalConnectorControlServicing {
    func suspendForSignedOut() async {}
    func resumeServerAccess() async throws -> LocalConnectorStatus { try await fetchStatus() }

    func fetchPluginApplications() async throws -> [LocalConnectorPluginApplication] { [] }
    func fetchPlugins(refresh: Bool) async throws -> [LocalConnectorPlugin] {
        try await fetchPlugins()
    }

    func inspectCustomMCP(_ draft: LocalConnectorCustomMCPDraft) async throws -> LocalConnectorPluginInspection {
        throw URLError(.unsupportedURL)
    }

    func installCustomMCP(
        _ draft: LocalConnectorCustomMCPDraft,
        expectedFingerprint: String
    ) async throws {
        throw URLError(.unsupportedURL)
    }

    func inspectLocalPlugin(atPath path: String) async throws -> LocalConnectorPluginInspection {
        throw URLError(.unsupportedURL)
    }

    func installLocalPlugin(atPath path: String, expectedFingerprint: String) async throws {
        throw URLError(.unsupportedURL)
    }

    func inspectPluginSource(
        _ source: LocalConnectorPluginSourceDraft
    ) async throws -> LocalConnectorPluginInspection {
        throw URLError(.unsupportedURL)
    }

    func installPluginSource(
        _ source: LocalConnectorPluginSourceDraft,
        expectedFingerprint: String
    ) async throws {
        throw URLError(.unsupportedURL)
    }

    func launchPluginApplication(
        pluginID: String,
        componentKey: String,
        context: LocalConnectorPluginApplicationContext?,
        expectedOwnerUserID: String
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        throw URLError(.unsupportedURL)
    }

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

    func startBrowserExtensionPairing(pluginID: String) async throws {
        throw URLError(.unsupportedURL)
    }

    func isBrowserExtensionPaired(pluginID: String) async throws -> Bool {
        false
    }
}
