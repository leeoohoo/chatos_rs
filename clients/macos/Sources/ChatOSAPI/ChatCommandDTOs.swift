import ChatOSCore
import Foundation

struct RuntimeSettingsDTO: Decodable, Sendable {
    var selectedModelID: String?
    var selectedModelName: String?
    var selectedThinkingLevel: String?
    var remoteConnectionID: String?
    var reasoningEnabled: Bool

    enum CodingKeys: String, CodingKey {
        case selectedModelID = "selected_model_id"
        case selectedModelName = "selected_model_name"
        case selectedThinkingLevel = "selected_thinking_level"
        case remoteConnectionID = "remote_connection_id"
        case reasoningEnabled = "reasoning_enabled"
    }
}

struct ModelConfigDTO: Decodable, Sendable {
    var id: String
    var name: String
    var provider: String?
    var model: String?
    var modelNameValue: String?
    var thinkingLevel: String?
    var thinkingLevels: [String]?
    var enabled: Bool?
    var supportsReasoning: Bool?

    enum CodingKeys: String, CodingKey {
        case id, name, provider, model, enabled
        case modelNameValue = "model_name"
        case thinkingLevel = "thinking_level"
        case thinkingLevels = "thinking_levels"
        case supportsReasoning = "supports_reasoning"
    }

    var modelName: String {
        modelNameValue?.trimmedNonEmptyValue ?? model?.trimmedNonEmptyValue ?? name
    }
}

extension String {
    var trimmedNonEmptyValue: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    var urlPathEncoded: String {
        let segmentAllowed = CharacterSet.urlPathAllowed
            .subtracting(CharacterSet(charactersIn: "/"))
        return addingPercentEncoding(withAllowedCharacters: segmentAllowed) ?? self
    }
}
