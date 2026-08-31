import Foundation

struct CompactHistoryResponseDTO: Decodable, Sendable {
    var items: [SessionMessageDTO]
    var hasMore: Bool
    var nextBefore: String?
    var snapshotRevision: Int64?

    enum CodingKeys: String, CodingKey {
        case items
        case hasMore = "has_more"
        case nextBefore = "next_before"
        case snapshotRevision = "snapshot_revision"
    }
}

struct SessionMessageDTO: Decodable, Sendable {
    var id: String
    var conversationID: String?
    var turnID: String?
    var sequenceNumber: Int64?
    var protocolRevision: Int64?
    var role: String
    var content: String
    var status: String?
    var metadata: [String: JSONValue]
    var messageMode: String?
    var messageSource: String?
    var toolCalls: [JSONValue]
    var createdAt: String?
    var updatedAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case conversationID = "conversation_id"
        case turnID = "turn_id"
        case sequenceNumber = "sequence_no"
        case protocolRevision = "revision"
        case role
        case content
        case status
        case metadata
        case messageMode = "message_mode"
        case messageSource = "message_source"
        case toolCalls = "tool_calls"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        conversationID = try container.decodeIfPresent(String.self, forKey: .conversationID)
        turnID = try container.decodeIfPresent(String.self, forKey: .turnID)
        sequenceNumber = try container.decodeIfPresent(Int64.self, forKey: .sequenceNumber)
        protocolRevision = try container.decodeIfPresent(Int64.self, forKey: .protocolRevision)
        role = try container.decode(String.self, forKey: .role)
        content = try container.decodeIfPresent(String.self, forKey: .content) ?? ""
        status = try container.decodeIfPresent(String.self, forKey: .status)
        metadata = try container.decodeIfPresent([String: JSONValue].self, forKey: .metadata) ?? [:]
        messageMode = try container.decodeIfPresent(String.self, forKey: .messageMode)
        messageSource = try container.decodeIfPresent(String.self, forKey: .messageSource)
        toolCalls = try container.decodeIfPresent([JSONValue].self, forKey: .toolCalls) ?? []
        createdAt = try container.decodeIfPresent(String.self, forKey: .createdAt)
        updatedAt = try container.decodeIfPresent(String.self, forKey: .updatedAt)
    }
}

struct WebSocketTicketDTO: Decodable, Sendable {
    var ticket: String
}
