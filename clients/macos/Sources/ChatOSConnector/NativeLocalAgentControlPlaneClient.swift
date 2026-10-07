import ChatOSCore
import Foundation

public struct LocalAgentModelConfigSnapshot: Codable, Sendable, Equatable {
    public let ownerUserID: String
    public let modelConfigRef: String
    public let modelConfigRevision: String
    public let credentialRef: String
    public let baseURL: String
    public let model: String
    public let provider: String
    public let supportsResponses: Bool
    public let supportsImages: Bool?
    public let instructions: String?
    public let temperature: Double?
    public let maxOutputTokens: Int64?
    public let thinkingLevel: String?
    public let includePromptCacheRetention: Bool
    public let requestBodyLimitBytes: UInt64?
    public let maxTransientRetries: UInt32?
    public let outputFormat: LocalAgentJSONSchemaOutputFormat?

    public init(
        ownerUserID: String,
        modelConfigRef: String,
        modelConfigRevision: String,
        credentialRef: String,
        baseURL: String,
        model: String,
        provider: String,
        supportsResponses: Bool,
        supportsImages: Bool? = nil,
        instructions: String? = nil,
        temperature: Double? = nil,
        maxOutputTokens: Int64? = nil,
        thinkingLevel: String? = nil,
        includePromptCacheRetention: Bool = false,
        requestBodyLimitBytes: UInt64? = nil,
        maxTransientRetries: UInt32? = nil,
        outputFormat: LocalAgentJSONSchemaOutputFormat? = nil
    ) {
        self.ownerUserID = ownerUserID
        self.modelConfigRef = modelConfigRef
        self.modelConfigRevision = modelConfigRevision
        self.credentialRef = credentialRef
        self.baseURL = baseURL
        self.model = model
        self.provider = provider
        self.supportsResponses = supportsResponses
        self.supportsImages = supportsImages
        self.instructions = instructions
        self.temperature = temperature
        self.maxOutputTokens = maxOutputTokens
        self.thinkingLevel = thinkingLevel
        self.includePromptCacheRetention = includePromptCacheRetention
        self.requestBodyLimitBytes = requestBodyLimitBytes
        self.maxTransientRetries = maxTransientRetries
        self.outputFormat = outputFormat
    }

    private enum CodingKeys: String, CodingKey {
        case model, provider, instructions, temperature
        case ownerUserID = "owner_user_id"
        case modelConfigRef = "model_config_ref"
        case modelConfigRevision = "model_config_revision"
        case credentialRef = "credential_ref"
        case baseURL = "base_url"
        case supportsResponses = "supports_responses"
        case supportsImages = "supports_images"
        case maxOutputTokens = "max_output_tokens"
        case thinkingLevel = "thinking_level"
        case includePromptCacheRetention = "include_prompt_cache_retention"
        case requestBodyLimitBytes = "request_body_limit_bytes"
        case maxTransientRetries = "max_transient_retries"
        case outputFormat = "output_format"
    }
}

public struct LocalAgentJSONSchemaOutputFormat: Codable, Sendable, Equatable {
    public let name: String
    public let description: String?
    public let schema: LocalAgentJSONValue
    public let strict: Bool

    public init(
        name: String,
        description: String? = nil,
        schema: LocalAgentJSONValue,
        strict: Bool
    ) {
        self.name = name
        self.description = description
        self.schema = schema
        self.strict = strict
    }
}

public struct LocalAgentCapabilityPolicySnapshot: Codable, Sendable, Equatable {
    public let ownerUserID: String
    public let profileKey: String
    public let capabilityPolicyRevision: String
    public let instructions: String?
    public let prefixedInputItems: [LocalAgentJSONValue]
    public let tools: [LocalAgentJSONValue]

    public init(
        ownerUserID: String,
        profileKey: String,
        capabilityPolicyRevision: String,
        instructions: String? = nil,
        prefixedInputItems: [LocalAgentJSONValue] = [],
        tools: [LocalAgentJSONValue] = []
    ) {
        self.ownerUserID = ownerUserID
        self.profileKey = profileKey
        self.capabilityPolicyRevision = capabilityPolicyRevision
        self.instructions = instructions
        self.prefixedInputItems = prefixedInputItems
        self.tools = tools
    }

    private enum CodingKeys: String, CodingKey {
        case instructions, tools
        case ownerUserID = "owner_user_id"
        case profileKey = "profile_key"
        case capabilityPolicyRevision = "capability_policy_revision"
        case prefixedInputItems = "prefixed_input_items"
    }
}

public struct NativeLocalAgentControlPlaneClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    @discardableResult
    public func publishModel(
        _ snapshot: LocalAgentModelConfigSnapshot
    ) async throws -> LocalAgentModelConfigSnapshot {
        let response: ModelResult = try await host.request(PutModelCommand(
            type: "put_model_config_snapshot",
            snapshot: snapshot
        ))
        try require(response.type, expected: "model_config_snapshot")
        return response.snapshot
    }

    public func model(
        ownerUserID: String,
        modelConfigRef: String,
        modelConfigRevision: String
    ) async throws -> LocalAgentModelConfigSnapshot {
        let response: ModelResult = try await host.request(GetModelCommand(
            type: "get_model_config_snapshot",
            ownerUserID: ownerUserID,
            modelConfigRef: modelConfigRef,
            modelConfigRevision: modelConfigRevision
        ))
        try require(response.type, expected: "model_config_snapshot")
        return response.snapshot
    }

    public func latestModels(
        ownerUserID: String
    ) async throws -> [LocalAgentModelConfigSnapshot] {
        let response: ModelsResult = try await host.request(ListModelsCommand(
            type: "list_latest_model_config_snapshots",
            ownerUserID: ownerUserID
        ))
        try require(response.type, expected: "model_config_snapshots")
        return response.snapshots
    }

    @discardableResult
    public func publishCapabilities(
        _ snapshot: LocalAgentCapabilityPolicySnapshot
    ) async throws -> LocalAgentCapabilityPolicySnapshot {
        let response: CapabilityResult = try await host.request(PutCapabilityCommand(
            type: "put_capability_policy_snapshot",
            snapshot: snapshot
        ))
        try require(response.type, expected: "capability_policy_snapshot")
        return response.snapshot
    }

    public func capabilities(
        ownerUserID: String,
        profileKey: String,
        capabilityPolicyRevision: String
    ) async throws -> LocalAgentCapabilityPolicySnapshot {
        let response: CapabilityResult = try await host.request(GetCapabilityCommand(
            type: "get_capability_policy_snapshot",
            ownerUserID: ownerUserID,
            profileKey: profileKey,
            capabilityPolicyRevision: capabilityPolicyRevision
        ))
        try require(response.type, expected: "capability_policy_snapshot")
        return response.snapshot
    }

    public func latestCapabilities(
        ownerUserID: String,
        profileKey: String
    ) async throws -> LocalAgentCapabilityPolicySnapshot {
        let response: CapabilityResult = try await host.request(GetLatestCapabilityCommand(
            type: "get_latest_capability_policy_snapshot",
            ownerUserID: ownerUserID,
            profileKey: profileKey
        ))
        try require(response.type, expected: "capability_policy_snapshot")
        return response.snapshot
    }

    private func require(_ actual: String, expected: String) throws {
        guard actual == expected else { throw NativeLocalAgentHostError.invalidResponse }
    }
}

private struct PutModelCommand: Encodable, Sendable {
    let type: String
    let snapshot: LocalAgentModelConfigSnapshot
}

private struct GetModelCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let modelConfigRef: String
    let modelConfigRevision: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case modelConfigRef = "model_config_ref"
        case modelConfigRevision = "model_config_revision"
    }
}

private struct ListModelsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
    }
}

private struct PutCapabilityCommand: Encodable, Sendable {
    let type: String
    let snapshot: LocalAgentCapabilityPolicySnapshot
}

private struct GetCapabilityCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let profileKey: String
    let capabilityPolicyRevision: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case profileKey = "profile_key"
        case capabilityPolicyRevision = "capability_policy_revision"
    }
}

private struct GetLatestCapabilityCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let profileKey: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case profileKey = "profile_key"
    }
}

private struct ModelResult: Decodable, Sendable {
    let type: String
    let snapshot: LocalAgentModelConfigSnapshot
}

private struct ModelsResult: Decodable, Sendable {
    let type: String
    let snapshots: [LocalAgentModelConfigSnapshot]
}

private struct CapabilityResult: Decodable, Sendable {
    let type: String
    let snapshot: LocalAgentCapabilityPolicySnapshot
}
