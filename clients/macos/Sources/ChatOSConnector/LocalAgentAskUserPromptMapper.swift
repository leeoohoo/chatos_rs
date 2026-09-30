import ChatOSCore
import Foundation

enum LocalAgentAskUserPromptMapper {
    static func map(
        _ value: LocalAgentJSONValue,
        run: LocalAgentRunRecord,
        context: LocalAskUserRunContext,
        event: LocalAgentEventRecord
    ) -> AskUserPrompt {
        let stored = value.object ?? [:]
        let payload = stored["payload"]?.object ?? stored
        let fields = (payload["fields"]?.array ?? []).enumerated().compactMap {
            field($0.element, index: $0.offset)
        }
        let choice = payload["choice"].flatMap(choice)
        return AskUserPrompt(
            id: "local-ask:\(run.runID)",
            sessionID: context.conversationID,
            turnID: context.turnID,
            toolCallID: stored["tool_call_id"]?.string ?? run.runID,
            kind: stored["kind"]?.string ?? kind(fields: fields, choice: choice),
            status: .pending,
            title: stored["title"]?.string ?? "需要你的回复",
            message: stored["message"]?.string
                ?? stored["question"]?.string
                ?? payload["message"]?.string
                ?? payload["question"]?.string
                ?? value.string
                ?? "请提供继续执行所需的信息。",
            allowsCancel: stored["allow_cancel"]?.bool ?? true,
            timeoutMilliseconds: stored["timeout_ms"]?.int64,
            fields: fields,
            choice: choice,
            createdAt: date(event.createdAtUnixMs),
            updatedAt: date(run.updatedAtUnixMs)
        )
    }

    private static func field(_ value: LocalAgentJSONValue, index: Int) -> AskUserField? {
        guard let object = value.object else { return nil }
        let explicitKey = object["key"]?.string
            ?? object["name"]?.string
            ?? object["id"]?.string
        let label = object["label"]?.string?.trimmingCharacters(in: .whitespacesAndNewlines)
        let key = explicitKey ?? fieldKey(label ?? "field_\(index + 1)")
        guard !key.isEmpty else { return nil }
        return AskUserField(
            key: key,
            label: label?.isEmpty == false ? label! : key,
            description: object["description"]?.string,
            placeholder: object["placeholder"]?.string,
            defaultValue: object["default_value"]?.string ?? object["default"]?.string ?? "",
            isRequired: object["required"]?.bool ?? false,
            isMultiline: object["multiline"]?.bool ?? false,
            isSecret: object["secret"]?.bool ?? false
        )
    }

    private static func choice(_ value: LocalAgentJSONValue) -> AskUserChoice? {
        guard let object = value.object else { return nil }
        let options = (object["options"]?.array ?? []).compactMap { item -> AskUserChoiceOption? in
            guard let option = item.object, let value = option["value"]?.string else { return nil }
            return AskUserChoiceOption(
                value: value,
                label: option["label"]?.string ?? value,
                description: option["description"]?.string
            )
        }
        guard !options.isEmpty else { return nil }
        let allowsMultiple = object["multiple"]?.bool ?? false
        let defaults: [String]
        if let values = object["default"]?.array {
            defaults = values.compactMap(\.string)
        } else if let value = object["default"]?.string {
            defaults = [value]
        } else {
            defaults = []
        }
        return AskUserChoice(
            allowsMultiple: allowsMultiple,
            options: options,
            defaultSelection: defaults,
            minimumSelectionCount: object["min_selections"]?.int ?? 0,
            maximumSelectionCount: object["max_selections"]?.int
        )
    }

    private static func kind(fields: [AskUserField], choice: AskUserChoice?) -> String {
        if !fields.isEmpty, choice != nil { return "mixed" }
        if !fields.isEmpty { return "fields" }
        if choice != nil { return "choice" }
        return "confirmation"
    }

    private static func fieldKey(_ value: String) -> String {
        String(value.lowercased().map { character in
            character.isLetter || character.isNumber || character == "_" ? character : "_"
        }).trimmingCharacters(in: CharacterSet(charactersIn: "_"))
    }

    private static func date(_ unixMilliseconds: Int64) -> Date {
        Date(timeIntervalSince1970: Double(unixMilliseconds) / 1_000)
    }
}

extension AskUserPrompt {
    func updating(status: AskUserPromptStatus) -> AskUserPrompt {
        var updated = self
        updated.status = status
        updated.updatedAt = Date()
        return updated
    }
}

extension LocalAgentJSONValue {
    fileprivate var object: [String: LocalAgentJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    fileprivate var array: [LocalAgentJSONValue]? {
        guard case let .array(value) = self else { return nil }
        return value
    }

    fileprivate var string: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }

    fileprivate var bool: Bool? {
        guard case let .bool(value) = self else { return nil }
        return value
    }

    fileprivate var int64: Int64? {
        guard case let .number(value) = self else { return nil }
        return Int64(value)
    }

    fileprivate var int: Int? {
        guard case let .number(value) = self else { return nil }
        return Int(value)
    }
}
