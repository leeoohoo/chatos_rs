import ChatOSCore
import Foundation

struct NativeConnectorGateway: Sendable {
    let baseURL: URL
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    func exchange(ticket: String, deviceName: String) async throws -> GatewayLoginDTO {
        try await request(
            "/api/auth/local-connector-ticket/exchange",
            method: "POST",
            body: GatewayTicketExchangeRequest(
                ticket: ticket,
                deviceName: deviceName,
                clientVersion: "2.0.14-swift"
            )
        )
    }

    func createDevice(
        token: String,
        displayName: String,
        publicKey: String
    ) async throws -> GatewayDeviceDTO {
        try await request(
            "/api/local-connectors/devices",
            token: token,
            method: "POST",
            body: GatewayCreateDeviceRequest(
                displayName: displayName,
                publicKey: publicKey,
                clientVersion: "2.0.14-swift",
                operatingSystem: "macOS"
            )
        )
    }

    func device(token: String, id: String) async throws -> GatewayDeviceDTO {
        try await request(
            "/api/local-connectors/devices/\(id.urlPathEncoded)",
            token: token
        )
    }

    func disconnectDevice(token: String, id: String) async throws {
        let _: GatewayDeviceDTO = try await request(
            "/api/local-connectors/devices/\(id.urlPathEncoded)/disconnect",
            token: token,
            method: "POST",
            body: EmptyRequest()
        )
    }

    func listWorkspaces(token: String) async throws -> [GatewayWorkspaceDTO] {
        try await request("/api/local-connectors/workspaces", token: token)
    }

    func createWorkspace(
        token: String,
        deviceID: String,
        alias: String,
        fingerprint: String
    ) async throws -> GatewayWorkspaceDTO {
        try await request(
            "/api/local-connectors/workspaces",
            token: token,
            method: "POST",
            body: GatewayCreateWorkspaceRequest(
                deviceID: deviceID,
                displayName: alias,
                localPathAlias: alias,
                localPathFingerprint: fingerprint,
                capabilities: ["mcp", "terminal", "sandbox"]
            )
        )
    }

    func modelConfigs(token: String) async throws -> [GatewayModelConfigDTO] {
        try await request("/api/model-configs", token: token)
    }

    func modelConfig(token: String, id: String, includeSecret: Bool) async throws -> GatewayModelConfigDTO {
        let suffix = includeSecret ? "?include_secret=true" : ""
        return try await request(
            "/api/model-configs/\(id.urlPathEncoded)\(suffix)",
            token: token
        )
    }

    func updateModelConfig(
        token: String,
        id: String,
        update: LocalConnectorModelConfigUpdate
    ) async throws -> GatewayModelConfigDTO {
        try await request(
            "/api/model-configs/\(id.urlPathEncoded)",
            token: token,
            method: "PATCH",
            body: GatewayModelConfigUpdateRequest(update: update)
        )
    }

    func modelSettings(token: String) async throws -> GatewayModelSettingsDTO {
        try await request("/api/model-configs/settings", token: token)
    }

    func updateModelSettings(
        token: String,
        settings: LocalConnectorModelSettings
    ) async throws -> GatewayModelSettingsDTO {
        try await request(
            "/api/model-configs/settings",
            token: token,
            method: "PUT",
            body: GatewayModelSettingsUpdateRequest(settings: settings)
        )
    }

    func modelProviders(token: String) async throws -> [GatewayModelProviderDTO] {
        try await request("/api/model-providers", token: token)
    }

    func createModelProvider(
        token: String,
        draft: LocalConnectorModelProviderDraft
    ) async throws -> GatewayModelProviderDTO {
        try await request(
            "/api/model-providers",
            token: token,
            method: "POST",
            body: GatewayModelProviderMutationRequest(draft: draft, includeEmptyAPIKey: true)
        )
    }

    func updateModelProvider(
        token: String,
        id: String,
        draft: LocalConnectorModelProviderDraft
    ) async throws -> GatewayModelProviderDTO {
        try await request(
            "/api/model-providers/\(id.urlPathEncoded)",
            token: token,
            method: "PATCH",
            body: GatewayModelProviderMutationRequest(draft: draft, includeEmptyAPIKey: false)
        )
    }

    func refreshModelProvider(token: String, id: String) async throws -> GatewayModelProviderDTO {
        try await request(
            "/api/model-providers/\(id.urlPathEncoded)/refresh",
            token: token,
            method: "POST",
            body: EmptyRequest()
        )
    }

    func deleteModelProvider(token: String, id: String) async throws {
        try await requestWithoutResponse(
            "/api/model-providers/\(id.urlPathEncoded)",
            token: token,
            method: "DELETE"
        )
    }

    func pluginSources(token: String) async throws -> GatewayPluginSourceListDTO {
        try await request(
            "/api/plugin-management/plugins/install-sources",
            token: token
        )
    }

    func updatePluginPreference(
        token: String,
        pluginID: String,
        deviceID: String,
        enabled: Bool
    ) async throws {
        let _: GatewayPluginPreferenceResponse = try await request(
            "/api/plugin-management/plugins/\(pluginID.urlPathEncoded)/preference",
            token: token,
            method: "PUT",
            body: GatewayPluginPreferenceRequest(deviceID: deviceID, enabled: enabled)
        )
    }

    func managedRuntimeConfig(token: String) async throws -> GatewayManagedRuntimeConfigDTO {
        try await request("/api/local-connectors/config/runtime", token: token)
    }

    func downloadPluginArtifact(
        token: String,
        pluginID: String,
        releaseID: String
    ) async throws -> URL {
        let endpoint = "/api/plugin-management/plugins/\(pluginID.urlPathEncoded)/releases/\(releaseID.urlPathEncoded)/artifact"
        guard let url = makeURL(endpoint) else { throw NativeConnectorError.invalidEndpoint }
        var request = URLRequest(url: url)
        request.timeoutInterval = 5 * 60
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/gzip, application/octet-stream", forHTTPHeaderField: "Accept")
        request.setValue("local-connector-swift", forHTTPHeaderField: "X-Chatos-Client-Surface")
        let (temporaryURL, response) = try await URLSession.shared.download(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw NativeConnectorError.invalidResponse("缺少 HTTP 状态")
        }
        guard (200..<300).contains(http.statusCode) else {
            throw NativeConnectorError.server(
                status: http.statusCode,
                message: "Plugin 安装包下载失败"
            )
        }
        let retainedURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-plugin-\(UUID().uuidString).tgz")
        try FileManager.default.moveItem(at: temporaryURL, to: retainedURL)
        return retainedURL
    }

    private func request<Response: Decodable & Sendable>(
        _ endpoint: String,
        token: String? = nil,
        method: String = "GET"
    ) async throws -> Response {
        try await send(endpoint, token: token, method: method, body: nil)
    }

    private func request<Response: Decodable & Sendable, Body: Encodable>(
        _ endpoint: String,
        token: String? = nil,
        method: String,
        body: Body
    ) async throws -> Response {
        try await send(endpoint, token: token, method: method, body: encoder.encode(body))
    }

    private func requestWithoutResponse(
        _ endpoint: String,
        token: String?,
        method: String
    ) async throws {
        guard let url = makeURL(endpoint) else {
            throw NativeConnectorError.invalidEndpoint
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.timeoutInterval = 30
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("local-connector-swift", forHTTPHeaderField: "X-Chatos-Client-Surface")
        if let token {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw NativeConnectorError.invalidResponse("缺少 HTTP 状态")
        }
        guard (200..<300).contains(http.statusCode) else {
            let payload = try? decoder.decode(GatewayErrorDTO.self, from: data)
            throw NativeConnectorError.server(
                status: http.statusCode,
                message: payload?.message ?? String(decoding: data, as: UTF8.self)
            )
        }
    }

    private func send<Response: Decodable & Sendable>(
        _ endpoint: String,
        token: String?,
        method: String,
        body: Data?
    ) async throws -> Response {
        guard let url = makeURL(endpoint) else {
            throw NativeConnectorError.invalidEndpoint
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.httpBody = body
        request.timeoutInterval = 30
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("local-connector-swift", forHTTPHeaderField: "X-Chatos-Client-Surface")
        if let token {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw NativeConnectorError.invalidResponse("缺少 HTTP 状态")
        }
        guard (200..<300).contains(http.statusCode) else {
            let payload = try? decoder.decode(GatewayErrorDTO.self, from: data)
            throw NativeConnectorError.server(
                status: http.statusCode,
                message: payload?.message ?? String(decoding: data, as: UTF8.self)
            )
        }
        do {
            return try decoder.decode(Response.self, from: data)
        } catch {
            throw NativeConnectorError.invalidResponse(error.localizedDescription)
        }
    }

    private func makeURL(_ endpoint: String) -> URL? {
        URL(
            string: baseURL.absoluteString
                .trimmingCharacters(in: CharacterSet(charactersIn: "/")) + endpoint
        )
    }
}

struct GatewayTicketExchangeRequest: Encodable {
    var ticket: String
    var deviceName: String
    var clientVersion: String
    enum CodingKeys: String, CodingKey {
        case ticket
        case deviceName = "device_name"
        case clientVersion = "client_version"
    }
}

struct GatewayLoginDTO: Decodable, Sendable {
    var token: String
    var user: GatewayUserDTO
}

struct GatewayUserDTO: Decodable, Sendable {
    var id: String
    var username: String
    var displayName: String?
    var role: String?
    enum CodingKeys: String, CodingKey {
        case id, username, role
        case displayName = "display_name"
    }

    var domainModel: LocalConnectorUser {
        .init(id: id, username: username, displayName: displayName, role: role ?? "user")
    }
}

struct GatewayCreateDeviceRequest: Encodable {
    var displayName: String
    var publicKey: String
    var clientVersion: String
    var operatingSystem: String
    enum CodingKeys: String, CodingKey {
        case displayName = "display_name"
        case publicKey = "public_key"
        case clientVersion = "client_version"
        case operatingSystem = "os"
    }
}

struct GatewayDeviceDTO: Decodable, Sendable {
    var id: String
    var ownerUserID: String
    var displayName: String
    var publicKey: String
    var status: String
    enum CodingKeys: String, CodingKey {
        case id, status
        case ownerUserID = "owner_user_id"
        case displayName = "display_name"
        case publicKey = "public_key"
    }
}

struct GatewayCreateWorkspaceRequest: Encodable {
    var deviceID: String
    var displayName: String
    var localPathAlias: String
    var localPathFingerprint: String
    var capabilities: [String]
    enum CodingKeys: String, CodingKey {
        case capabilities
        case deviceID = "device_id"
        case displayName = "display_name"
        case localPathAlias = "local_path_alias"
        case localPathFingerprint = "local_path_fingerprint"
    }
}

struct GatewayWorkspaceDTO: Decodable, Sendable {
    var id: String
    var deviceID: String
    var localPathAlias: String
    var localPathFingerprint: String
    enum CodingKeys: String, CodingKey {
        case id
        case deviceID = "device_id"
        case localPathAlias = "local_path_alias"
        case localPathFingerprint = "local_path_fingerprint"
    }
}

struct GatewayModelConfigDTO: Decodable, Sendable {
    var id: String
    var sourceProviderID: String?
    var name: String
    var provider: String
    var promptVendor: String?
    var model: String
    var apiKey: String?
    var baseURL: String?
    var taskUsageScenario: String?
    var taskThinkingLevel: String?
    var temperature: Double?
    var maxOutputTokens: Int?
    var enabled: Bool?
    var taskEnabled: Bool?
    var hasAPIKey: Bool?
    var supportsImages: Bool?
    var supportsReasoning: Bool?
    var supportsResponses: Bool?
    enum CodingKeys: String, CodingKey {
        case id, name, provider, model, enabled
        case taskEnabled = "task_enabled"
        case sourceProviderID = "source_provider_id"
        case promptVendor = "prompt_vendor"
        case apiKey = "api_key"
        case baseURL = "base_url"
        case taskUsageScenario = "task_usage_scenario"
        case taskThinkingLevel = "task_thinking_level"
        case temperature
        case maxOutputTokens = "max_output_tokens"
        case hasAPIKey = "has_api_key"
        case supportsImages = "supports_images"
        case supportsReasoning = "supports_reasoning"
        case supportsResponses = "supports_responses"
    }
}

struct GatewayModelSettingsDTO: Decodable, Sendable {
    var modelRequestMaxRetries: Int?
    var memorySummaryModelConfigID: String?
    var memorySummaryThinkingLevel: String?
    var projectManagementAgentModelConfigID: String?
    var projectManagementAgentThinkingLevel: String?
    var commandApprovalModelConfigID: String?
    var commandApprovalThinkingLevel: String?
    enum CodingKeys: String, CodingKey {
        case modelRequestMaxRetries = "model_request_max_retries"
        case memorySummaryModelConfigID = "memory_summary_model_config_id"
        case memorySummaryThinkingLevel = "memory_summary_thinking_level"
        case projectManagementAgentModelConfigID = "project_management_agent_model_config_id"
        case projectManagementAgentThinkingLevel = "project_management_agent_thinking_level"
        case commandApprovalModelConfigID = "command_approval_model_config_id"
        case commandApprovalThinkingLevel = "command_approval_thinking_level"
    }
}

struct GatewayModelProviderDTO: Decodable, Sendable {
    var id: String
    var name: String
    var provider: String
    var promptVendor: String?
    var baseURL: String?
    var hasAPIKey: Bool?
    var enabled: Bool?
    var supportsImages: Bool?
    var supportsReasoning: Bool?
    var supportsResponses: Bool?
    var lastSyncStatus: String?
    var lastSyncError: String?
    var importedModelCount: Int?

    enum CodingKeys: String, CodingKey {
        case id, name, provider, enabled
        case promptVendor = "prompt_vendor"
        case baseURL = "base_url"
        case hasAPIKey = "has_api_key"
        case supportsImages = "supports_images"
        case supportsReasoning = "supports_reasoning"
        case supportsResponses = "supports_responses"
        case lastSyncStatus = "last_sync_status"
        case lastSyncError = "last_sync_error"
        case importedModelCount = "imported_model_count"
    }
}

private struct GatewayModelProviderMutationRequest: Encodable {
    var name: String
    var provider: String
    var promptVendor: String
    var apiKey: String?
    var clearAPIKey: Bool
    var baseURL: String
    var enabled: Bool
    var supportsImages: Bool
    var supportsReasoning: Bool
    var supportsResponses: Bool

    init(draft: LocalConnectorModelProviderDraft, includeEmptyAPIKey: Bool) {
        name = draft.name
        provider = draft.provider
        promptVendor = draft.promptVendor
        apiKey = includeEmptyAPIKey || !draft.apiKey.isEmpty ? draft.apiKey : nil
        clearAPIKey = draft.clearAPIKey
        baseURL = draft.baseURL
        enabled = draft.enabled
        supportsImages = draft.supportsImages
        supportsReasoning = draft.supportsReasoning
        supportsResponses = draft.supportsResponses
    }

    enum CodingKeys: String, CodingKey {
        case name, provider, enabled
        case promptVendor = "prompt_vendor"
        case apiKey = "api_key"
        case clearAPIKey = "clear_api_key"
        case baseURL = "base_url"
        case supportsImages = "supports_images"
        case supportsReasoning = "supports_reasoning"
        case supportsResponses = "supports_responses"
    }
}

private struct GatewayModelConfigUpdateRequest: Encodable {
    var taskUsageScenario: String?
    var taskThinkingLevel: String?
    var temperature: Double?
    var clearTemperature: Bool
    var maxOutputTokens: Int?
    var clearMaxOutputTokens: Bool
    var taskEnabled: Bool

    init(update: LocalConnectorModelConfigUpdate) {
        taskUsageScenario = update.taskUsageScenario
        taskThinkingLevel = update.taskThinkingLevel
        temperature = update.temperature
        clearTemperature = update.temperature == nil
        maxOutputTokens = update.maxOutputTokens
        clearMaxOutputTokens = update.maxOutputTokens == nil
        taskEnabled = update.taskEnabled
    }

    enum CodingKeys: String, CodingKey {
        case taskEnabled = "task_enabled"
        case temperature
        case taskUsageScenario = "task_usage_scenario"
        case taskThinkingLevel = "task_thinking_level"
        case clearTemperature = "clear_temperature"
        case maxOutputTokens = "max_output_tokens"
        case clearMaxOutputTokens = "clear_max_output_tokens"
    }
}

private struct GatewayModelSettingsUpdateRequest: Encodable {
    var settings: LocalConnectorModelSettings

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(settings.modelRequestMaxRetries ?? 5, forKey: .modelRequestMaxRetries)
        try container.encode(settings.memorySummaryModelConfigID, forKey: .memorySummaryModelConfigID)
        try container.encode(settings.memorySummaryThinkingLevel, forKey: .memorySummaryThinkingLevel)
        try container.encode(
            settings.projectManagementAgentModelConfigID,
            forKey: .projectManagementAgentModelConfigID
        )
        try container.encode(
            settings.projectManagementAgentThinkingLevel,
            forKey: .projectManagementAgentThinkingLevel
        )
    }

    enum CodingKeys: String, CodingKey {
        case modelRequestMaxRetries = "model_request_max_retries"
        case memorySummaryModelConfigID = "memory_summary_model_config_id"
        case memorySummaryThinkingLevel = "memory_summary_thinking_level"
        case projectManagementAgentModelConfigID = "project_management_agent_model_config_id"
        case projectManagementAgentThinkingLevel = "project_management_agent_thinking_level"
    }
}

struct GatewayPluginSourceListDTO: Decodable, Sendable {
    var items: [GatewayPluginSourceDTO]
}

struct GatewayPluginSourceDTO: Decodable, Sendable {
    var catalog: GatewayPluginCatalogDTO
    var release: GatewayPluginReleaseDTO
    var preference: GatewayPluginPreferenceDTO?
}

struct GatewayPluginCatalogDTO: Decodable, Sendable {
    var id: String
    var displayName: String?
    var name: String?
    var description: String?
    var publisher: GatewayPluginPublisherDTO?
    var interface: GatewayPluginInterfaceDTO?
    var pluginKey: String? = nil
    enum CodingKeys: String, CodingKey {
        case id, name, description, publisher, interface
        case pluginKey = "plugin_key"
        case displayName = "display_name"
    }
}

struct GatewayPluginPublisherDTO: Decodable, Sendable {
    var id: String?
    var name: String?
}

struct GatewayPluginInterfaceDTO: Decodable, Sendable {
    var category: String?
    var developerName: String?
}

struct GatewayPluginReleaseDTO: Decodable, Sendable {
    var id: String
    var version: String?
    var artifactSHA256: String?
    var npmPackage: GatewayPluginNPMPackageDTO?

    enum CodingKeys: String, CodingKey {
        case id, version
        case artifactSHA256 = "artifact_sha256"
        case npmPackage = "npm_package"
    }
}

struct GatewayPluginNPMPackageDTO: Decodable, Sendable {
    var name: String
    var version: String
    var integrity: String
}

struct GatewayPluginPreferenceDTO: Decodable, Sendable {
    var enabled: Bool
}

struct GatewayPluginPreferenceRequest: Encodable {
    var deviceID: String
    var enabled: Bool
    enum CodingKeys: String, CodingKey {
        case enabled
        case deviceID = "device_id"
    }
}

struct GatewayPluginPreferenceResponse: Decodable, Sendable {
    var enabled: Bool?
}

struct GatewayManagedRuntimeConfigDTO: Decodable, Sendable {
    var remoteControlTrust: GatewayRemoteControlTrustDTO
    enum CodingKeys: String, CodingKey {
        case remoteControlTrust = "remote_control_trust"
    }
}

struct GatewayRemoteControlTrustDTO: Decodable, Sendable {
    var requireSignedMessages: Bool
    var signatureMaxSkewSeconds: Int
    var trustedRelayPublicKeys: [String: String]
    enum CodingKeys: String, CodingKey {
        case requireSignedMessages = "require_signed_messages"
        case signatureMaxSkewSeconds = "signature_max_skew_seconds"
        case trustedRelayPublicKeys = "trusted_relay_public_keys"
    }
}

private struct GatewayErrorDTO: Decodable {
    var error: String?
    var message: String? { error }
}

private struct EmptyRequest: Encodable {}

private struct GatewaySuccessDTO: Decodable, Sendable {
    var success: Bool?
}

private extension String {
    var urlPathEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? self
    }
}
