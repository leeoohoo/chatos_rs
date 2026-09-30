import ChatOSCore
import Foundation

public struct LocalAgentConversationRuntimeSettings: Decodable, Sendable, Equatable {
    public let ownerUserID: String
    public let conversationID: String
    public let selectedModelConfigRef: String
    public let selectedModelConfigRevision: String
    public let selectedThinkingLevel: String?
    public let remoteConnectionID: String?
    public let reasoningEnabled: Bool
    public let version: UInt64
    public let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case ownerUserID = "owner_user_id"
        case conversationID = "conversation_id"
        case selectedModelConfigRef = "selected_model_config_ref"
        case selectedModelConfigRevision = "selected_model_config_revision"
        case selectedThinkingLevel = "selected_thinking_level"
        case remoteConnectionID = "remote_connection_id"
        case reasoningEnabled = "reasoning_enabled"
        case version
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

public struct LocalAgentConversationRuntimeSettingsInput: Sendable, Equatable {
    public let ownerUserID: String
    public let conversationID: String
    public let selectedModelConfigRef: String
    public let selectedModelConfigRevision: String
    public let selectedThinkingLevel: String?
    public let remoteConnectionID: String?
    public let reasoningEnabled: Bool
    public let expectedVersion: UInt64?

    public init(
        ownerUserID: String,
        conversationID: String,
        selectedModelConfigRef: String,
        selectedModelConfigRevision: String,
        selectedThinkingLevel: String?,
        remoteConnectionID: String?,
        reasoningEnabled: Bool,
        expectedVersion: UInt64?
    ) {
        self.ownerUserID = ownerUserID
        self.conversationID = conversationID
        self.selectedModelConfigRef = selectedModelConfigRef
        self.selectedModelConfigRevision = selectedModelConfigRevision
        self.selectedThinkingLevel = selectedThinkingLevel
        self.remoteConnectionID = remoteConnectionID
        self.reasoningEnabled = reasoningEnabled
        self.expectedVersion = expectedVersion
    }
}

public struct NativeLocalAgentConversationRuntimeSettingsClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    public func get(
        ownerUserID: String,
        conversationID: String
    ) async throws -> LocalAgentConversationRuntimeSettings {
        let response: RuntimeSettingsResult = try await host.request(GetRuntimeSettingsCommand(
            type: "get_conversation_runtime_settings",
            ownerUserID: ownerUserID,
            conversationID: conversationID
        ))
        try require(response.type)
        return response.settings
    }

    public func put(
        _ input: LocalAgentConversationRuntimeSettingsInput
    ) async throws -> LocalAgentConversationRuntimeSettings {
        let response: RuntimeSettingsResult = try await host.request(
            PutRuntimeSettingsCommand(input)
        )
        try require(response.type)
        return response.settings
    }

    private func require(_ type: String) throws {
        guard type == "conversation_runtime_settings" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
    }
}

private struct GetRuntimeSettingsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let conversationID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case conversationID = "conversation_id"
    }
}

private struct PutRuntimeSettingsCommand: Encodable, Sendable {
    let type = "put_conversation_runtime_settings"
    let input: LocalAgentConversationRuntimeSettingsInput

    init(_ input: LocalAgentConversationRuntimeSettingsInput) {
        self.input = input
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(type, forKey: .type)
        try container.encode(input.ownerUserID, forKey: .ownerUserID)
        try container.encode(input.conversationID, forKey: .conversationID)
        try container.encode(input.selectedModelConfigRef, forKey: .selectedModelConfigRef)
        try container.encode(
            input.selectedModelConfigRevision,
            forKey: .selectedModelConfigRevision
        )
        try container.encodeIfPresent(
            input.selectedThinkingLevel,
            forKey: .selectedThinkingLevel
        )
        if let remoteConnectionID = input.remoteConnectionID {
            try container.encode(remoteConnectionID, forKey: .remoteConnectionID)
        } else {
            try container.encodeNil(forKey: .remoteConnectionID)
        }
        try container.encode(input.reasoningEnabled, forKey: .reasoningEnabled)
        try container.encodeIfPresent(input.expectedVersion, forKey: .expectedVersion)
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case conversationID = "conversation_id"
        case selectedModelConfigRef = "selected_model_config_ref"
        case selectedModelConfigRevision = "selected_model_config_revision"
        case selectedThinkingLevel = "selected_thinking_level"
        case remoteConnectionID = "remote_connection_id"
        case reasoningEnabled = "reasoning_enabled"
        case expectedVersion = "expected_version"
    }
}

private struct RuntimeSettingsResult: Decodable, Sendable {
    let type: String
    let settings: LocalAgentConversationRuntimeSettings
}
