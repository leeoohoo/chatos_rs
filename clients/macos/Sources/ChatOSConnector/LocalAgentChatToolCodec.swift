import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    static func arguments(_ call: AgentToolCall) throws -> [String: Any] {
        guard let data = call.arguments.data(using: .utf8),
              let value = try? JSONSerialization.jsonObject(with: data),
              let object = value as? [String: Any] else {
            throw AgentGroupChatError.invalidField("arguments")
        }
        return object
    }

    static func requiredString(_ object: [String: Any], key: String) throws -> String {
        guard let value = object[key] as? String else {
            throw AgentGroupChatError.invalidField(key)
        }
        return value
    }

    static func optionalString(_ object: [String: Any], key: String) throws -> String? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let string = value as? String else {
            throw AgentGroupChatError.invalidField(key)
        }
        return string
    }

    static func optionalInteger(_ object: [String: Any], key: String) throws -> Int64? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            throw AgentGroupChatError.invalidField(key)
        }
        let integer = number.int64Value
        guard number.doubleValue == Double(integer) else {
            throw AgentGroupChatError.invalidField(key)
        }
        return integer
    }

    static func optionalBoolean(_ object: [String: Any], key: String) throws -> Bool? {
        guard let value = object[key] else { return nil }
        guard !(value is NSNull), let number = value as? NSNumber,
              CFGetTypeID(number) == CFBooleanGetTypeID() else {
            throw AgentGroupChatError.invalidField(key)
        }
        return number.boolValue
    }

    static func optionalStringArray(_ object: [String: Any], key: String) throws -> [String] {
        guard let value = object[key] else { return [] }
        guard let values = value as? [String] else {
            throw AgentGroupChatError.invalidField(key)
        }
        return values
    }

    static func optionalObjectArray(
        _ object: [String: Any],
        key: String
    ) throws -> [[String: Any]] {
        guard let value = object[key] else { return [] }
        guard let values = value as? [[String: Any]] else {
            throw AgentGroupChatError.invalidField(key)
        }
        return values
    }


    static func messageLengthFailure(_ content: String) -> AgentToolOutcome? {
        let maximum = AgentCommunicationPolicy.standard.maximumMessageCharacters
        guard content.count > maximum else { return nil }
        return structuredFailure(
            code: "message_too_long",
            field: "content",
            message: "消息正文超过 \(maximum) 字符。请保留结论、风险和下一步，把详细内容写入 Markdown 文档后附加发送。",
            retryable: true,
            nextTool: createDocumentToolName
        )
    }

    static func sanitizedMarkdownDocumentName(_ rawValue: String) -> String? {
        let trimmed = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let forbidden = CharacterSet.controlCharacters.union(CharacterSet(charactersIn: "/\\:"))
        var scalars = String.UnicodeScalarView()
        for scalar in trimmed.unicodeScalars {
            if forbidden.contains(scalar) {
                scalars.append("-")
            } else {
                scalars.append(scalar)
            }
        }
        var value = String(scalars)
        while value.contains("..") {
            value = value.replacingOccurrences(of: "..", with: ".")
        }
        value = value.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines.union(
            CharacterSet(charactersIn: ".-")
        ))
        guard !value.isEmpty else { return nil }
        if !value.lowercased().hasSuffix(".md") {
            value += ".md"
        }
        if value.count > 240 {
            let stem = String(value.dropLast(3).prefix(237))
            value = stem + ".md"
        }
        return value
    }

    static func outcome<Value: Encodable>(_ value: Value) throws -> AgentToolOutcome {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return .init(String(decoding: try encoder.encode(value), as: UTF8.self))
    }

    static func structuredFailure(
        code: String,
        field: String?,
        message: String,
        retryable: Bool,
        nextTool: String? = nil
    ) -> AgentToolOutcome {
        let response = StructuredToolFailure(error: .init(
            code: code,
            field: field,
            message: message,
            retryable: retryable,
            nextTool: nextTool
        ))
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let content = (try? encoder.encode(response)).map {
            String(decoding: $0, as: UTF8.self)
        } ?? #"{"ok":false,"error":{"code":"internal_error","message":"工具校验失败。","retryable":false}}"#
        return .failure(content)
    }

    static func errorCode(_ error: AgentGroupChatError) -> String {
        switch error {
        case .invalidField: "invalid_argument"
        case .notFound: "resource_not_found"
        case .conflict: "state_conflict"
        case .notMember: "agent_not_in_team"
        case .permissionDenied: "permission_denied"
        case .storage: "storage_error"
        }
    }

    static func errorField(_ error: AgentGroupChatError) -> String? {
        guard case let .invalidField(field) = error else { return nil }
        return field
    }

}
