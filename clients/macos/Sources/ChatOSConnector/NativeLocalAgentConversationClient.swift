import ChatOSCore
import Foundation

public struct LocalAgentConversationRecord: Decodable, Sendable, Equatable {
    public let conversationID: String
    public let ownerUserID: String
    public let title: String
    public let resource: LocalAgentConversationResourceBinding?
    public let version: UInt64
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case conversationID = "conversation_id"
        case ownerUserID = "owner_user_id"
        case title, resource, version
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

public struct LocalAgentConversationResourceBinding: Codable, Sendable, Equatable {
    public enum Kind: String, Codable, Sendable {
        case contact
        case project
    }

    public let kind: Kind
    public let resourceID: String

    public init(kind: Kind, resourceID: String) {
        self.kind = kind
        self.resourceID = resourceID
    }

    private enum CodingKeys: String, CodingKey {
        case kind
        case resourceID = "resource_id"
    }
}

public struct LocalAgentConversationTurnRecord: Decodable, Sendable, Equatable {
    public let turnID: String
    public let conversationID: String
    public let userMessageID: String
    public let runID: String
    public let status: String
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case turnID = "turn_id"
        case conversationID = "conversation_id"
        case userMessageID = "user_message_id"
        case runID = "run_id"
        case status
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

public enum LocalAgentJSONValue: Codable, Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([LocalAgentJSONValue])
    case object([String: LocalAgentJSONValue])

    public init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let decoded = try? value.decode(Bool.self) { self = .bool(decoded) }
        else if let decoded = try? value.decode(Double.self) { self = .number(decoded) }
        else if let decoded = try? value.decode(String.self) { self = .string(decoded) }
        else if let decoded = try? value.decode([LocalAgentJSONValue].self) { self = .array(decoded) }
        else { self = .object(try value.decode([String: LocalAgentJSONValue].self)) }
    }

    public func encode(to encoder: Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case .null: try value.encodeNil()
        case let .bool(decoded): try value.encode(decoded)
        case let .number(decoded): try value.encode(decoded)
        case let .string(decoded): try value.encode(decoded)
        case let .array(decoded): try value.encode(decoded)
        case let .object(decoded): try value.encode(decoded)
        }
    }
}

extension LocalAgentJSONValue {
    var conversationDisplayText: String {
        Self.displayText(from: self)
    }

    private static func displayText(from value: LocalAgentJSONValue) -> String {
        switch value {
        case .null:
            return ""
        case let .bool(value):
            return String(value)
        case let .number(value):
            return String(value)
        case let .string(value):
            return value
        case let .array(values):
            return values.map(displayText).filter { !$0.isEmpty }.joined(separator: "\n")
        case let .object(values):
            if case .string("task_graph_terminal")? = values["type"],
               case let .array(tasks)? = values["tasks"] {
                return taskGraphText(tasks)
            }
            return firstNonemptyText(in: values)
        }
    }

    private static func firstNonemptyText(
        in values: [String: LocalAgentJSONValue]
    ) -> String {
        for key in ["text", "content", "report", "answer", "error", "message"] {
            guard let value = values[key] else { continue }
            let text = displayText(from: value)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty { return text }
        }
        return ""
    }

    private static func taskGraphText(_ tasks: [LocalAgentJSONValue]) -> String {
        let rendered = tasks.compactMap { task -> (title: String, body: String)? in
            guard case let .object(values) = task else { return nil }
            let title = values["title"].map(displayText) ?? ""
            let status = values["status"].map(displayText) ?? ""
            let body = values["terminal_outcome"].map(displayText) ?? ""
            let fallback = [title, status].filter { !$0.isEmpty }.joined(separator: ": ")
            return (title, body.isEmpty ? fallback : body)
        }
        if rendered.count == 1 { return rendered[0].body }
        return rendered.map { item in
            item.title.isEmpty ? item.body : "### \(item.title)\n\n\(item.body)"
        }
        .filter { !$0.isEmpty }
        .joined(separator: "\n\n")
    }
}

public struct LocalAgentConversationMessageRecord: Decodable, Sendable, Equatable {
    public let messageID: String
    public let conversationID: String
    public let turnID: String
    public let ordinal: UInt64
    public let role: String
    public let content: LocalAgentJSONValue
    public let metadata: LocalAgentJSONValue
    public let createdAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case messageID = "message_id"
        case conversationID = "conversation_id"
        case turnID = "turn_id"
        case ordinal, role, content, metadata
        case createdAtUnixMs = "created_at_unix_ms"
    }
}

public struct LocalAgentConversationAttachmentRecord: Decodable, Sendable, Equatable {
    public let attachmentID: String
    public let conversationID: String
    public let turnID: String
    public let messageID: String
    public let ordinal: UInt64
    public let displayName: String
    public let mediaType: String
    public let byteSize: UInt64
    public let sha256: String
    public let authorizedLocalRef: String
    public let metadata: LocalAgentJSONValue
    public let createdAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case attachmentID = "attachment_id"
        case conversationID = "conversation_id"
        case turnID = "turn_id"
        case messageID = "message_id"
        case ordinal
        case displayName = "display_name"
        case mediaType = "media_type"
        case byteSize = "byte_size"
        case sha256
        case authorizedLocalRef = "authorized_local_ref"
        case metadata
        case createdAtUnixMs = "created_at_unix_ms"
    }
}

public struct LocalAgentConversationDetail: Decodable, Sendable, Equatable {
    public let conversation: LocalAgentConversationRecord
    public let turns: [LocalAgentConversationTurnRecord]
    public let messages: [LocalAgentConversationMessageRecord]
    public let attachments: [LocalAgentConversationAttachmentRecord]
}

public struct LocalAgentConversationPage: Decodable, Sendable, Equatable {
    public let conversations: [LocalAgentConversationRecord]
    public let nextBeforeUpdatedAtUnixMs: Int64?
    public let nextBeforeConversationID: String?

    private enum CodingKeys: String, CodingKey {
        case conversations
        case nextBeforeUpdatedAtUnixMs = "next_before_updated_at_unix_ms"
        case nextBeforeConversationID = "next_before_conversation_id"
    }
}

public struct LocalAgentConversationHistoryPage: Decodable, Sendable, Equatable {
    public let conversation: LocalAgentConversationRecord
    public let turns: [LocalAgentConversationTurnRecord]
    public let messages: [LocalAgentConversationMessageRecord]
    public let attachments: [LocalAgentConversationAttachmentRecord]
    public let nextBeforeOrdinal: UInt64?

    private enum CodingKeys: String, CodingKey {
        case conversation, turns, messages, attachments
        case nextBeforeOrdinal = "next_before_ordinal"
    }
}

public struct NativeLocalAgentConversationClient: Sendable {
    let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    public func create(
        ownerUserID: String,
        conversationID: String,
        title: String,
        resource: LocalAgentConversationResourceBinding? = nil
    ) async throws -> LocalAgentConversationDetail {
        let response: ConversationResult = try await host.request(CreateCommand(
            type: "create_conversation",
            conversationID: conversationID,
            ownerUserID: ownerUserID,
            title: title,
            resource: resource
        ))
        try Self.require(response.type, expected: "conversation")
        return response.conversation
    }

    public func get(
        ownerUserID: String,
        conversationID: String
    ) async throws -> LocalAgentConversationDetail {
        let response: ConversationResult = try await host.request(GetCommand(
            type: "get_conversation",
            ownerUserID: ownerUserID,
            conversationID: conversationID
        ))
        try Self.require(response.type, expected: "conversation")
        return response.conversation
    }

    public func list(
        ownerUserID: String,
        beforeUpdatedAtUnixMs: Int64? = nil,
        beforeConversationID: String? = nil,
        limit: UInt32 = 100
    ) async throws -> LocalAgentConversationPage {
        let response: ConversationsResult = try await host.request(ListCommand(
            type: "list_conversations",
            ownerUserID: ownerUserID,
            beforeUpdatedAtUnixMs: beforeUpdatedAtUnixMs,
            beforeConversationID: beforeConversationID,
            limit: limit
        ))
        try Self.require(response.type, expected: "conversations")
        return response.page
    }

    public func history(
        ownerUserID: String,
        conversationID: String,
        beforeOrdinal: UInt64? = nil,
        limit: UInt32 = 100
    ) async throws -> LocalAgentConversationHistoryPage {
        let response: HistoryResult = try await host.request(HistoryCommand(
            type: "get_conversation_history",
            ownerUserID: ownerUserID,
            conversationID: conversationID,
            beforeOrdinal: beforeOrdinal,
            limit: limit
        ))
        try Self.require(response.type, expected: "conversation_history")
        return response.page
    }

    private static func require(_ actual: String, expected: String) throws {
        guard actual == expected else { throw NativeLocalAgentHostError.invalidResponse }
    }
}

private struct CreateCommand: Encodable, Sendable {
    let type: String
    let conversationID: String
    let ownerUserID: String
    let title: String
    let resource: LocalAgentConversationResourceBinding?

    private enum CodingKeys: String, CodingKey {
        case type, title, resource
        case conversationID = "conversation_id"
        case ownerUserID = "owner_user_id"
    }
}

private struct GetCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let conversationID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case conversationID = "conversation_id"
    }
}

private struct ListCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let beforeUpdatedAtUnixMs: Int64?
    let beforeConversationID: String?
    let limit: UInt32

    private enum CodingKeys: String, CodingKey {
        case type, limit
        case ownerUserID = "owner_user_id"
        case beforeUpdatedAtUnixMs = "before_updated_at_unix_ms"
        case beforeConversationID = "before_conversation_id"
    }
}

private struct HistoryCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let conversationID: String
    let beforeOrdinal: UInt64?
    let limit: UInt32

    private enum CodingKeys: String, CodingKey {
        case type, limit
        case ownerUserID = "owner_user_id"
        case conversationID = "conversation_id"
        case beforeOrdinal = "before_ordinal"
    }
}

private struct ConversationResult: Decodable, Sendable {
    let type: String
    let conversation: LocalAgentConversationDetail
}

private struct ConversationsResult: Decodable, Sendable {
    let type: String
    let page: LocalAgentConversationPage
}

private struct HistoryResult: Decodable, Sendable {
    let type: String
    let page: LocalAgentConversationHistoryPage
}
