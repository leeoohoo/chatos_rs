import ChatOSCore
import Foundation

public actor ChatOSLocalConnectorControlService: LocalConnectorControlServicing {
    private let chatOSClient: ChatOSAPIClient
    private let localClient: LocalConnectorAPIClient
    private let connectorCloudBaseURL: URL
    private let encoder = JSONEncoder()

    public init(
        chatOSClient: ChatOSAPIClient,
        localBaseURL: URL,
        localDesktopToken: String,
        connectorCloudBaseURL: URL,
        transport: any HTTPTransport = URLSessionHTTPTransport()
    ) {
        self.chatOSClient = chatOSClient
        self.localClient = LocalConnectorAPIClient(
            baseURL: localBaseURL,
            desktopToken: localDesktopToken,
            transport: transport
        )
        self.connectorCloudBaseURL = connectorCloudBaseURL
    }

    public func fetchStatus() async throws -> LocalConnectorStatus {
        let response: ConnectorStatusDTO = try await localClient.request("/api/local/status")
        return response.domainModel
    }

    public func pairWithCurrentChatOSSession(deviceName: String?) async throws -> LocalConnectorStatus {
        let ticket: DesktopTicketDTO = try await chatOSClient.request(
            "/auth/local-connector-ticket",
            method: "POST"
        )
        let body = try encoder.encode(
            DesktopTicketAuthDTO(
                cloudBaseURL: connectorCloudBaseURL.absoluteString,
                ticket: ticket.ticket,
                deviceName: deviceName
            )
        )
        let response: ConnectorStatusDTO = try await localClient.request(
            "/api/local/auth/desktop-ticket",
            method: "POST",
            body: body
        )
        return response.domainModel
    }

    public func disconnect() async throws -> LocalConnectorStatus {
        let response: ConnectorStatusDTO = try await localClient.request(
            "/api/local/auth/logout",
            method: "POST"
        )
        return response.domainModel
    }

    public func fetchRuntimeSettings() async throws -> LocalConnectorRuntimeSettings {
        let response: LocalRuntimeSettingsDTO = try await localClient.request("/api/local/runtime-settings")
        return response.domainModel
    }

    public func updateDeveloperMode(_ enabled: Bool) async throws -> LocalConnectorRuntimeSettings {
        let body = try encoder.encode(UpdateRuntimeSettingsDTO(developerMode: enabled))
        let response: LocalRuntimeSettingsDTO = try await localClient.request(
            "/api/local/runtime-settings",
            method: "POST",
            body: body
        )
        return response.domainModel
    }

    public func fetchSystemPermissions() async throws -> LocalConnectorSystemPermissions {
        let response: SystemPermissionsDTO = try await localClient.request("/api/local/system-permissions")
        return response.domainModel
    }

    public func requestSystemPermission(id: String) async throws -> LocalConnectorSystemPermissions {
        let encodedID = id.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? id
        let response: SystemPermissionsDTO = try await localClient.request(
            "/api/local/system-permissions/\(encodedID)/request",
            method: "POST"
        )
        return response.domainModel
    }

    public func executeTerminal(
        workspaceID: String,
        commandLine: String,
        cwd: String?
    ) async throws -> LocalConnectorTerminalResult {
        let body = try encoder.encode(
            TerminalRequestDTO(
                workspaceID: workspaceID,
                command: "/bin/zsh",
                args: ["-lc", commandLine],
                cwd: cwd,
                timeoutMilliseconds: 120_000
            )
        )
        let response: TerminalResultDTO = try await localClient.request(
            "/api/local/terminal/exec",
            method: "POST",
            body: body
        )
        return response.domainModel
    }

    public func fetchCommandHistory(limit: Int) async throws -> [LocalConnectorCommandHistoryEntry] {
        let response: CommandHistoryDTO = try await localClient.request(
            "/api/local/commands?limit=\(max(1, min(limit, 200)))"
        )
        return response.entries.map(\.domainModel)
    }

    public func clearCommandHistory() async throws {
        try await localClient.requestVoid("/api/local/commands", method: "DELETE")
    }

    public func fetchApprovalSettings() async throws -> LocalConnectorApprovalSettings {
        let response: ApprovalSettingsDTO = try await localClient.request("/api/local/approval/settings")
        return response.domainModel
    }

    public func updateDefaultApprovalMode(
        _ mode: LocalConnectorApprovalMode,
        riskAcknowledged: Bool
    ) async throws -> LocalConnectorApprovalSettings {
        let body = try encoder.encode(
            UpdateApprovalSettingsDTO(
                defaultMode: mode.rawValue,
                riskAcknowledged: riskAcknowledged
            )
        )
        let response: ApprovalSettingsDTO = try await localClient.request(
            "/api/local/approval/settings",
            method: "POST",
            body: body
        )
        return response.domainModel
    }

    public func fetchPendingApprovals() async throws -> [LocalConnectorPendingApproval] {
        let response: PendingApprovalsDTO = try await localClient.request("/api/local/approval/pending")
        return response.items.map(\.domainModel) + response.reviewing.map(\.domainModel)
    }

    public func resolveApproval(id: String, decision: String) async throws {
        let encodedID = id.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? id
        if decision == "decline" || decision == "cancel" {
            let body = try encoder.encode(DenyApprovalDTO(reason: decision == "cancel" ? "用户取消" : "用户拒绝"))
            try await localClient.requestVoid(
                "/api/local/approval/pending/\(encodedID)/deny",
                method: "POST",
                body: body
            )
            return
        }
        let body = try encoder.encode(
            ApproveApprovalDTO(
                decision: decision,
                rememberAllow: decision == "acceptForSession",
                riskAcknowledged: true
            )
        )
        try await localClient.requestVoid(
            "/api/local/approval/pending/\(encodedID)/approve",
            method: "POST",
            body: body
        )
    }

    public func fetchModelCatalog(refresh: Bool) async throws -> LocalConnectorModelCatalog {
        let endpoint = refresh ? "/api/local/model-configs/refresh" : "/api/local/model-configs"
        let response: ModelCatalogDTO = try await localClient.request(
            endpoint,
            method: refresh ? "POST" : "GET"
        )
        return response.domainModel
    }

    public func fetchSandboxBackends() async throws -> [LocalConnectorSandboxBackend] {
        let response: SandboxCapabilitiesDTO = try await localClient.request("/api/local/sandbox/capabilities")
        return response.backends.map(\.domainModel)
    }

    public func fetchSandboxSettings() async throws -> LocalConnectorSandboxSettings {
        let response: SandboxSettingsDTO = try await localClient.request("/api/local/sandbox/settings")
        return response.domainModel
    }

    public func updateSandboxSettings(
        enabled: Bool?,
        permissionProfileID: String?,
        approvalPolicy: String?,
        approvalReviewer: String?,
        networkAccess: String?
    ) async throws -> LocalConnectorSandboxSettings {
        let body = try encoder.encode(
            UpdateSandboxSettingsDTO(
                enabled: enabled,
                defaultPermissionProfileID: permissionProfileID,
                defaultApprovalPolicy: approvalPolicy,
                defaultApprovalReviewer: approvalReviewer,
                defaultNetworkAccess: networkAccess,
                riskAcknowledged: true
            )
        )
        let response: SandboxSettingsDTO = try await localClient.request(
            "/api/local/sandbox/settings",
            method: "PUT",
            body: body
        )
        return response.domainModel
    }

    public func fetchPlugins() async throws -> [LocalConnectorPlugin] {
        let response: PluginCatalogDTO = try await localClient.request("/api/local/plugins/catalog")
        return response.items.map(\.domainModel)
    }

    public func installPlugin(id: String) async throws {
        let encodedID = id.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? id
        try await localClient.requestVoid(
            "/api/local/plugins/\(encodedID)/install",
            method: "POST"
        )
    }

    public func uninstallPlugin(id: String) async throws {
        let encodedID = id.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? id
        let body = try encoder.encode(UninstallPluginDTO(acknowledgePluginDataRemoval: true))
        try await localClient.requestVoid(
            "/api/local/plugins/\(encodedID)",
            method: "DELETE",
            body: body
        )
    }

    public func updatePluginEnabled(id: String, enabled: Bool) async throws {
        let encodedID = id.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? id
        let body = try encoder.encode(UpdatePluginPreferenceDTO(enabled: enabled))
        try await localClient.requestVoid(
            "/api/local/plugins/\(encodedID)/preference",
            method: "PUT",
            body: body
        )
    }

    public func requestPluginPermission(pluginID: String, permissionID: String) async throws {
        let systemPermissionID: String
        switch permissionID {
        case "computer.accessibility": systemPermissionID = "accessibility_control"
        case "computer.screen-recording": systemPermissionID = "screen_recording"
        default: return
        }
        _ = try await requestSystemPermission(id: systemPermissionID)
    }
}

private actor LocalConnectorAPIClient {
    private let baseURL: URL
    private let desktopToken: String
    private let transport: any HTTPTransport
    private let decoder = JSONDecoder()

    init(baseURL: URL, desktopToken: String, transport: any HTTPTransport) {
        self.baseURL = baseURL
        self.desktopToken = desktopToken
        self.transport = transport
    }

    func request<Response: Decodable & Sendable>(
        _ endpoint: String,
        method: String = "GET",
        body: Data? = nil
    ) async throws -> Response {
        let response = try await send(endpoint, method: method, body: body)
        do {
            return try decoder.decode(Response.self, from: response.body)
        } catch {
            throw ChatOSAPIError.decoding(error.localizedDescription)
        }
    }

    func requestVoid(
        _ endpoint: String,
        method: String,
        body: Data? = nil
    ) async throws {
        _ = try await send(endpoint, method: method, body: body)
    }

    private func send(_ endpoint: String, method: String, body: Data?) async throws -> HTTPResponse {
        let base = baseURL.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let path = endpoint.hasPrefix("/") ? endpoint : "/\(endpoint)"
        guard let url = URL(string: base + path) else {
            throw ChatOSAPIError.invalidEndpoint
        }
        let response = try await transport.send(
            HTTPRequest(
                url: url,
                method: method,
                headers: [
                    "Accept": "application/json",
                    "Content-Type": "application/json",
                    "Authorization": "Bearer \(desktopToken)",
                ],
                body: body
            )
        )
        guard (200..<300).contains(response.statusCode) else {
            throw ChatOSAPIError.server(
                statusCode: response.statusCode,
                message: String(decoding: response.body, as: UTF8.self)
            )
        }
        return response
    }
}
