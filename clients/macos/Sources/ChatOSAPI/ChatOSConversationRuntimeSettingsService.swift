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
        return response
            .filter { $0.enabled != false && $0.modelName.trimmedNonEmptyValue != nil }
            .map {
                ConversationModelOption(
                    id: $0.id,
                    displayName: $0.name.trimmedNonEmptyValue ?? $0.modelName,
                    modelName: $0.modelName,
                    thinkingLevel: $0.thinkingLevel?.trimmedNonEmptyValue
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

    public func updatePlanMode(
        sessionID: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        try await update(sessionID: sessionID, body: PlanModeUpdateDTO(planModeEnabled: enabled))
    }

    public func updateReasoning(
        sessionID: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        try await update(sessionID: sessionID, body: ReasoningUpdateDTO(reasoningEnabled: enabled))
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

private struct ModelUpdateDTO: Encodable {
    var selectedModelID: String

    enum CodingKeys: String, CodingKey {
        case selectedModelID = "selected_model_id"
    }
}

private struct PlanModeUpdateDTO: Encodable {
    var planModeEnabled: Bool

    enum CodingKeys: String, CodingKey {
        case planModeEnabled = "plan_mode_enabled"
    }
}

private struct ReasoningUpdateDTO: Encodable {
    var reasoningEnabled: Bool

    enum CodingKeys: String, CodingKey {
        case reasoningEnabled = "reasoning_enabled"
    }
}

extension RuntimeSettingsDTO {
    var model: ConversationRuntimeSettings {
        ConversationRuntimeSettings(
            selectedModelID: selectedModelID,
            selectedModelName: selectedModelName,
            selectedThinkingLevel: selectedThinkingLevel,
            reasoningEnabled: reasoningEnabled,
            planModeEnabled: planModeEnabled
        )
    }
}
