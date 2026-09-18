import Foundation

public enum AgentGroupChatError: Error, Equatable, LocalizedError {
    case invalidField(String)
    case notFound
    case conflict
    case notMember
    case permissionDenied
    case storage(String)

    public var errorDescription: String? {
        switch self {
        case let .invalidField(field): "无效的 Agent 群聊字段：\(field)"
        case .notFound: "Agent 群聊资源不存在。"
        case .conflict: "Agent 群聊状态已经变化，请刷新后重试。"
        case .notMember: "Agent 不是当前群聊成员。"
        case .permissionDenied: "当前身份不能执行这个群聊操作。"
        case let .storage(message): "本地 Agent 群聊存储不可用：\(message)"
        }
    }
}

public enum AgentGroupChatValidation {
    public static func identifier(_ value: String, field: String) throws {
        guard !value.isEmpty,
              value.count <= 512,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              value.rangeOfCharacter(from: .controlCharacters) == nil else {
            throw AgentGroupChatError.invalidField(field)
        }
    }

    public static func identifiers(_ values: [String], field: String, maximumCount: Int) throws {
        guard values.count <= maximumCount, Set(values).count == values.count else {
            throw AgentGroupChatError.invalidField(field)
        }
        for value in values { try identifier(value, field: field) }
    }

    public static func text(_ value: String, field: String, maximumLength: Int) throws {
        guard !value.isEmpty,
              value.count <= maximumLength,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.contains("\0") else {
            throw AgentGroupChatError.invalidField(field)
        }
    }

    public static func optionalText(_ value: String, field: String, maximumLength: Int) throws {
        guard value.count <= maximumLength,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.contains("\0") else {
            throw AgentGroupChatError.invalidField(field)
        }
    }

    public static func timestamps(_ createdAtUnixMs: Int64, _ updatedAtUnixMs: Int64) throws {
        guard createdAtUnixMs >= 0, updatedAtUnixMs >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("timestamps")
        }
    }
}
