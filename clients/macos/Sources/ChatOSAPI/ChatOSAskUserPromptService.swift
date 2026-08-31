import ChatOSCore
import Foundation

public struct ChatOSAskUserPromptService: AskUserPromptServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchPrompts(sessionID: String, limit: Int = 100) async throws -> [AskUserPrompt] {
        let normalizedLimit = max(1, min(limit, 500))
        let response: AskUserPromptListDTO = try await client.request(
            "/ask-user-prompts?conversation_id=\(sessionID.askUserQueryEncoded)&include_pending=true&limit=\(normalizedLimit)"
        )
        return response.prompts.compactMap(\.model)
    }

    public func submit(
        promptID: String,
        sessionID: String,
        submission: AskUserSubmission
    ) async throws -> AskUserPrompt {
        let body = AskUserPromptSubmissionDTO(
            conversationID: sessionID,
            values: submission.values.isEmpty ? nil : submission.values,
            selection: submission.selection.map(AskUserSelectionDTO.init)
        )
        let response: AskUserPromptMutationDTO = try await client.request(
            "/ask-user-prompts/\(promptID.askUserPathEncoded)/submit",
            method: "POST",
            body: try JSONEncoder().encode(body)
        )
        guard let prompt = response.prompt.model else {
            throw ChatOSAPIError.decoding("服务器未返回交互请求状态")
        }
        return prompt
    }

    public func cancel(promptID: String, sessionID: String) async throws -> AskUserPrompt {
        let response: AskUserPromptMutationDTO = try await client.request(
            "/ask-user-prompts/\(promptID.askUserPathEncoded)/cancel",
            method: "POST",
            body: try JSONEncoder().encode(
                AskUserPromptCancelDTO(
                    conversationID: sessionID,
                    reason: "user_cancelled"
                )
            )
        )
        guard let prompt = response.prompt.model else {
            throw ChatOSAPIError.decoding("服务器未返回交互请求状态")
        }
        return prompt
    }
}

private struct AskUserPromptListDTO: Decodable, Sendable {
    var prompts: [AskUserPromptRecordDTO]

    enum CodingKeys: String, CodingKey { case prompts }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        prompts = try container.decodeIfPresent([AskUserPromptRecordDTO].self, forKey: .prompts) ?? []
    }
}

private struct AskUserPromptMutationDTO: Decodable, Sendable {
    var prompt: AskUserPromptRecordDTO
}

private struct AskUserPromptSubmissionDTO: Encodable {
    var conversationID: String
    var values: [String: String]?
    var selection: AskUserSelectionDTO?

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case values, selection
    }
}

private struct AskUserPromptCancelDTO: Encodable {
    var conversationID: String
    var reason: String

    enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case reason
    }
}

private enum AskUserSelectionDTO: Encodable {
    case single(String)
    case multiple([String])

    init(_ selection: AskUserSelection) {
        switch selection {
        case let .single(value): self = .single(value)
        case let .multiple(values): self = .multiple(values)
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case let .single(value): try container.encode(value)
        case let .multiple(values): try container.encode(values)
        }
    }
}

struct AskUserPromptRecordDTO: Decodable, Sendable {
    var id: String
    var conversationID: String
    var conversationTurnID: String
    var toolCallID: String?
    var kind: String
    var status: String
    var prompt: JSONValue
    var createdAt: String?
    var updatedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, kind, status, prompt
        case conversationID = "conversation_id"
        case conversationTurnID = "conversation_turn_id"
        case toolCallID = "tool_call_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    var model: AskUserPrompt? {
        guard let status = AskUserPromptStatus(rawValue: status.lowercased()) else { return nil }
        let stored = prompt.objectValue ?? [:]
        let payload = stored["payload"]?.objectValue ?? [:]
        let fields = (payload["fields"]?.arrayValue ?? []).enumerated().compactMap {
            AskUserField(dto: $0.element, index: $0.offset)
        }
        let choice = payload["choice"].flatMap(AskUserChoice.init(dto:))
        return AskUserPrompt(
            id: id,
            sessionID: conversationID,
            turnID: conversationTurnID,
            toolCallID: toolCallID ?? stored["tool_call_id"]?.stringValue,
            kind: kind,
            status: status,
            title: stored["title"]?.untrimmedStringValue ?? "",
            message: stored["message"]?.untrimmedStringValue ?? "",
            allowsCancel: stored["allow_cancel"]?.boolValue ?? true,
            timeoutMilliseconds: stored["timeout_ms"]?.int64Value,
            fields: fields,
            choice: choice,
            createdAt: createdAt.flatMap(Self.parseDate),
            updatedAt: updatedAt.flatMap(Self.parseDate)
        )
    }

    private static func parseDate(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
}

private extension AskUserField {
    init?(dto: JSONValue, index: Int) {
        guard let object = dto.objectValue else { return nil }
        let explicitKey = object["key"]?.stringValue
            ?? object["name"]?.stringValue
            ?? object["id"]?.stringValue
        let label = object["label"]?.untrimmedStringValue?.trimmingCharacters(in: .whitespacesAndNewlines)
        let key = explicitKey ?? label?.askUserFieldKey ?? "field_\(index + 1)"
        guard !key.isEmpty else { return nil }
        self.init(
            key: key,
            label: label?.isEmpty == false ? label! : key,
            description: object["description"]?.stringValue,
            placeholder: object["placeholder"]?.untrimmedStringValue,
            defaultValue: object["default_value"]?.untrimmedStringValue
                ?? object["default"]?.untrimmedStringValue
                ?? "",
            isRequired: object["required"]?.boolValue ?? false,
            isMultiline: object["multiline"]?.boolValue ?? false,
            isSecret: object["secret"]?.boolValue ?? false
        )
    }
}

private extension AskUserChoice {
    init?(dto: JSONValue) {
        guard let object = dto.objectValue else { return nil }
        let options = (object["options"]?.arrayValue ?? []).compactMap { value -> AskUserChoiceOption? in
            guard let option = value.objectValue,
                  let rawValue = option["value"]?.stringValue else { return nil }
            return AskUserChoiceOption(
                value: rawValue,
                label: option["label"]?.untrimmedStringValue ?? rawValue,
                description: option["description"]?.stringValue
            )
        }
        guard !options.isEmpty else { return nil }
        let allowsMultiple = object["multiple"]?.boolValue ?? false
        let defaults: [String]
        if let values = object["default"]?.arrayValue {
            defaults = values.compactMap(\.stringValue)
        } else if let value = object["default"]?.stringValue {
            defaults = [value]
        } else {
            defaults = []
        }
        self.init(
            allowsMultiple: allowsMultiple,
            options: options,
            defaultSelection: defaults,
            minimumSelectionCount: object["min_selections"]?.intValue ?? 0,
            maximumSelectionCount: object["max_selections"]?.intValue
        )
    }
}

private extension JSONValue {
    var objectValue: [String: JSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    var arrayValue: [JSONValue]? {
        guard case let .array(value) = self else { return nil }
        return value
    }

    var untrimmedStringValue: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }

    var int64Value: Int64? {
        guard case let .number(value) = self else { return nil }
        return Int64(value)
    }
}

private extension String {
    var askUserPathEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .askUserUnreserved) ?? self
    }

    var askUserQueryEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .askUserUnreserved) ?? self
    }

    var askUserFieldKey: String {
        let mapped = lowercased().map { character -> Character in
            if character.isLetter || character.isNumber || character == "_" { return character }
            return "_"
        }
        return String(mapped).trimmingCharacters(in: CharacterSet(charactersIn: "_"))
    }
}

private extension CharacterSet {
    static let askUserUnreserved = CharacterSet(
        charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~"
    )
}
