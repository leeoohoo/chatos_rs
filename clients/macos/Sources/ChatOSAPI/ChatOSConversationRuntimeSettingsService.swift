import ChatOSCore
import Foundation

public struct ChatOSConversationRuntimeSettingsService: ConversationRuntimeSettingsServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchSettings(sessionID: String) async throws -> ConversationRuntimeSettings {
        let response: RuntimeSettingsDTO = try await client.request(
            "/conversations/\(sessionID.urlPathEncoded)/runtime-settings"
        )
        return response.model
    }

    public func fetchAvailableModels() async throws -> [ConversationModelOption] {
        let response: [ModelConfigDTO] = try await client.request("/ai-model-configs")
        var seenIDs = Set<String>()
        var seenDisplayModels = Set<String>()
        return response
            .filter { $0.enabled != false && $0.modelName.trimmedNonEmptyValue != nil }
            .compactMap {
                let displayName = $0.name.trimmedNonEmptyValue ?? $0.modelName
                let provider = $0.provider?.trimmedNonEmptyValue?.lowercased() ?? "gpt"
                let supportsReasoning = $0.supportsReasoning ?? ($0.thinkingLevel != nil)
                let normalizedID = $0.id.trimmingCharacters(in: .whitespacesAndNewlines)
                    .lowercased()
                let normalizedDisplayModel = "\(displayName.trimmingCharacters(in: .whitespacesAndNewlines).lowercased())\u{0}\($0.modelName.trimmingCharacters(in: .whitespacesAndNewlines).lowercased())"
                guard !normalizedID.isEmpty,
                      seenIDs.insert(normalizedID).inserted,
                      seenDisplayModels.insert(normalizedDisplayModel).inserted else {
                    return nil
                }
                return ConversationModelOption(
                    id: $0.id,
                    displayName: displayName,
                    modelName: $0.modelName,
                    provider: provider,
                    thinkingLevel: $0.thinkingLevel?.trimmedNonEmptyValue,
                    supportsReasoning: supportsReasoning,
                    thinkingLevels: supportsReasoning
                        ? resolvedThinkingLevels($0.thinkingLevels, provider: provider)
                        : []
                )
            }
    }

    public func updateModel(
        sessionID: String,
        modelID: String
    ) async throws -> ConversationRuntimeSettings {
        try await update(
            sessionID: sessionID,
            body: ModelUpdateDTO(selectedModelID: modelID)
        )
    }

    public func updateRemoteConnection(
        sessionID: String,
        connectionID: String?
    ) async throws -> ConversationRuntimeSettings {
        try await update(
            sessionID: sessionID,
            body: RemoteConnectionUpdateDTO(remoteConnectionID: connectionID)
        )
    }

    public func updateReasoning(
        sessionID: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        try await update(sessionID: sessionID, body: ReasoningUpdateDTO(reasoningEnabled: enabled))
    }

    public func updateReasoningLevel(
        sessionID: String,
        level: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        try await update(
            sessionID: sessionID,
            body: ReasoningLevelUpdateDTO(
                selectedThinkingLevel: level,
                reasoningEnabled: enabled
            )
        )
    }

    private func update<Body: Encodable>(
        sessionID: String,
        body: Body
    ) async throws -> ConversationRuntimeSettings {
        let response: RuntimeSettingsDTO = try await client.request(
            "/conversations/\(sessionID.urlPathEncoded)/runtime-settings",
            method: "PUT",
            body: try JSONEncoder().encode(body)
        )
        return response.model
    }
}

private func resolvedThinkingLevels(_ serverLevels: [String]?, provider: String) -> [String] {
    let levels: [String]
    if let serverLevels, !serverLevels.isEmpty {
        levels = serverLevels
    } else {
        switch provider.replacingOccurrences(of: "-", with: "_") {
        case "gpt", "openai":
            levels = ["none", "minimal", "low", "medium", "high", "xhigh"]
        case "deepseek":
            levels = ["none", "low", "medium", "high", "max"]
        case "kimi", "kimik2", "moonshot":
            levels = ["none", "auto", "low", "medium", "high", "xhigh"]
        default:
            levels = ["none", "low", "medium", "high", "xhigh"]
        }
    }

    var seen = Set<String>()
    return levels.compactMap { raw in
        let level = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !level.isEmpty, seen.insert(level).inserted else { return nil }
        return level
    }
}

private struct ModelUpdateDTO: Encodable {
    var selectedModelID: String

    enum CodingKeys: String, CodingKey {
        case selectedModelID = "selected_model_id"
    }
}

private struct RemoteConnectionUpdateDTO: Encodable {
    var remoteConnectionID: String?

    enum CodingKeys: String, CodingKey {
        case remoteConnectionID = "remote_connection_id"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        if let remoteConnectionID {
            try container.encode(remoteConnectionID, forKey: .remoteConnectionID)
        } else {
            try container.encodeNil(forKey: .remoteConnectionID)
        }
    }
}

private struct ReasoningUpdateDTO: Encodable {
    var reasoningEnabled: Bool

    enum CodingKeys: String, CodingKey {
        case reasoningEnabled = "reasoning_enabled"
    }
}

private struct ReasoningLevelUpdateDTO: Encodable {
    var selectedThinkingLevel: String
    var reasoningEnabled: Bool

    enum CodingKeys: String, CodingKey {
        case selectedThinkingLevel = "selected_thinking_level"
        case reasoningEnabled = "reasoning_enabled"
    }
}

extension RuntimeSettingsDTO {
    var model: ConversationRuntimeSettings {
        ConversationRuntimeSettings(
            selectedModelID: selectedModelID,
            selectedModelName: selectedModelName,
            selectedThinkingLevel: selectedThinkingLevel,
            remoteConnectionID: remoteConnectionID,
            reasoningEnabled: reasoningEnabled
        )
    }
}
