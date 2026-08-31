import ChatOSCore
import Foundation

struct RuntimeSettingsDTO: Decodable, Sendable {
    var selectedModelID: String?
    var selectedModelName: String?
    var selectedThinkingLevel: String?
    var remoteConnectionID: String?
    var workspaceRoot: String?
    var reasoningEnabled: Bool
    var planModeEnabled: Bool

    enum CodingKeys: String, CodingKey {
        case selectedModelID = "selected_model_id"
        case selectedModelName = "selected_model_name"
        case selectedThinkingLevel = "selected_thinking_level"
        case remoteConnectionID = "remote_connection_id"
        case workspaceRoot = "workspace_root"
        case reasoningEnabled = "reasoning_enabled"
        case planModeEnabled = "plan_mode_enabled"
    }
}

struct ModelConfigDTO: Decodable, Sendable {
    var id: String
    var name: String
    var provider: String?
    var model: String?
    var modelNameValue: String?
    var thinkingLevel: String?
    var temperature: Double?
    var enabled: Bool?

    enum CodingKeys: String, CodingKey {
        case id, name, provider, model, temperature, enabled
        case modelNameValue = "model_name"
        case thinkingLevel = "thinking_level"
    }

    var modelName: String {
        modelNameValue?.trimmedNonEmptyValue ?? model?.trimmedNonEmptyValue ?? name
    }
}

struct ChatCommandRequestDTO: Encodable {
    var conversationID: String
    var content: String
    var attachments: [ConversationAttachmentReference]
    var reasoningEnabled: Bool
    var planMode: Bool
    var turnID: String
    var remoteConnectionID: String?
    var workspaceRoot: String?
    var modelConfigID: String
    var aiModelConfig: AIModelConfigDTO

    enum CodingKeys: String, CodingKey {
        case content, attachments
        case conversationID = "conversation_id"
        case reasoningEnabled = "reasoning_enabled"
        case planMode = "plan_mode"
        case turnID = "turn_id"
        case remoteConnectionID = "remote_connection_id"
        case workspaceRoot = "workspace_root"
        case modelConfigID = "model_config_id"
        case aiModelConfig = "ai_model_config"
    }
}

struct AIModelConfigDTO: Encodable {
    var temperature: Double
    var modelName: String
    var thinkingLevel: String?

    enum CodingKeys: String, CodingKey {
        case temperature
        case modelName = "model_name"
        case thinkingLevel = "thinking_level"
    }
}

struct GuidanceRequestDTO: Encodable {
    var conversationID: String
    var turnID: String
    var content: String
    var attachments: [ConversationAttachmentReference]

    enum CodingKeys: String, CodingKey {
        case content, attachments
        case conversationID = "conversation_id"
        case turnID = "turn_id"
    }
}

struct StopChatRequestDTO: Encodable {
    var conversationID: String
    var turnID: String?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case turnID = "turn_id"
    }
}

struct StopChatResponseDTO: Decodable, Sendable {
    var success: Bool
    var message: String?
}

struct ChatCommandResponseDTO: Decodable, Sendable {
    var accepted: Bool?
    var turnID: String?
    var userMessageID: String?
    var sourceUserMessageID: String?

    enum CodingKeys: String, CodingKey {
        case accepted
        case turnID = "turn_id"
        case userMessageID = "user_message_id"
        case sourceUserMessageID = "source_user_message_id"
    }

    func ack(fallbackTurnID: String) -> ConversationCommandAck {
        ConversationCommandAck(
            accepted: accepted != false,
            turnID: turnID?.trimmedNonEmptyValue ?? fallbackTurnID,
            userMessageID: sourceUserMessageID?.trimmedNonEmptyValue
                ?? userMessageID?.trimmedNonEmptyValue
        )
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
