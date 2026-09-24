import Foundation

public enum LocalConnectorCompanionResourceKind: String, Codable, Sendable {
    case contact
    case project
}

public struct LocalConnectorCompanionResource: Codable, Sendable, Equatable, Identifiable {
    public var id: String
    public var kind: LocalConnectorCompanionResourceKind
    public var title: String
    public var subtitle: String?
    public var conversationID: String?
    public var messageCount: Int
    public var updatedAt: String?

    public init(
        id: String,
        kind: LocalConnectorCompanionResourceKind,
        title: String,
        subtitle: String?,
        conversationID: String?,
        messageCount: Int,
        updatedAt: String?
    ) {
        self.id = id
        self.kind = kind
        self.title = title
        self.subtitle = subtitle
        self.conversationID = conversationID
        self.messageCount = messageCount
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, kind, title, subtitle
        case conversationID = "conversation_id"
        case messageCount = "message_count"
        case updatedAt = "updated_at"
    }
}

public struct LocalConnectorCompanionApproval: Codable, Sendable, Equatable, Identifiable {
    public var id: String
    public var command: String
    public var context: String?
    public var source: String
    public var risk: String
    public var reason: String?
    public var createdAt: String
    public var availableDecisions: [String]

    public init(
        id: String,
        command: String,
        context: String?,
        source: String,
        risk: String,
        reason: String?,
        createdAt: String,
        availableDecisions: [String]
    ) {
        self.id = id
        self.command = command
        self.context = context
        self.source = source
        self.risk = risk
        self.reason = reason
        self.createdAt = createdAt
        self.availableDecisions = availableDecisions
    }

    enum CodingKeys: String, CodingKey {
        case id, command, context, source, risk, reason
        case createdAt = "created_at"
        case availableDecisions = "available_decisions"
    }
}

public struct LocalConnectorCompanionAgentSummary: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let description: String
    public let professionKey: String
    public let status: String
    public let heartbeatEnabled: Bool
    public let lastHeartbeatAtUnixMs: Int64?
    public let updatedAtUnixMs: Int64

    public init(
        id: String,
        name: String,
        description: String,
        professionKey: String,
        status: String,
        heartbeatEnabled: Bool,
        lastHeartbeatAtUnixMs: Int64?,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.name = name
        self.description = description
        self.professionKey = professionKey
        self.status = status
        self.heartbeatEnabled = heartbeatEnabled
        self.lastHeartbeatAtUnixMs = lastHeartbeatAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    enum CodingKeys: String, CodingKey {
        case id, name, description, status
        case professionKey = "profession_key"
        case heartbeatEnabled = "heartbeat_enabled"
        case lastHeartbeatAtUnixMs = "last_heartbeat_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

public struct LocalConnectorCompanionAgentMessageAttachment: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let mimeType: String
    public let size: Int
    public let kind: String

    public init(id: String, name: String, mimeType: String, size: Int, kind: String) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.kind = kind
    }

    enum CodingKeys: String, CodingKey {
        case id, name, size, kind
        case mimeType = "mime_type"
    }
}

public struct LocalConnectorCompanionAgentMessage: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let roomID: String
    public let senderKind: String
    public let senderID: String
    public let content: String
    public let mentionedAgentIDs: [String]
    public let replyToMessageID: String?
    public let createdAtUnixMs: Int64
    public let attachments: [LocalConnectorCompanionAgentMessageAttachment]

    public init(
        id: String,
        roomID: String,
        senderKind: String,
        senderID: String,
        content: String,
        mentionedAgentIDs: [String],
        replyToMessageID: String?,
        createdAtUnixMs: Int64,
        attachments: [LocalConnectorCompanionAgentMessageAttachment]
    ) {
        self.id = id
        self.roomID = roomID
        self.senderKind = senderKind
        self.senderID = senderID
        self.content = content
        self.mentionedAgentIDs = mentionedAgentIDs
        self.replyToMessageID = replyToMessageID
        self.createdAtUnixMs = createdAtUnixMs
        self.attachments = attachments
    }

    enum CodingKeys: String, CodingKey {
        case id, content, attachments
        case roomID = "room_id"
        case senderKind = "sender_kind"
        case senderID = "sender_id"
        case mentionedAgentIDs = "mentioned_agent_ids"
        case replyToMessageID = "reply_to_message_id"
        case createdAtUnixMs = "created_at_unix_ms"
    }
}

public struct LocalConnectorCompanionAgentConversationSummary: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let kind: String
    public let title: String
    public let goal: String
    public let projectID: String
    public let defaultAgentID: String?
    public let memberCount: Int
    public let canSend: Bool
    public let updatedAtUnixMs: Int64
    public let lastMessage: LocalConnectorCompanionAgentMessage?

    public init(
        id: String,
        kind: String,
        title: String,
        goal: String,
        projectID: String,
        defaultAgentID: String?,
        memberCount: Int,
        canSend: Bool,
        updatedAtUnixMs: Int64,
        lastMessage: LocalConnectorCompanionAgentMessage?
    ) {
        self.id = id
        self.kind = kind
        self.title = title
        self.goal = goal
        self.projectID = projectID
        self.defaultAgentID = defaultAgentID
        self.memberCount = memberCount
        self.canSend = canSend
        self.updatedAtUnixMs = updatedAtUnixMs
        self.lastMessage = lastMessage
    }

    enum CodingKeys: String, CodingKey {
        case id, kind, title, goal
        case projectID = "project_id"
        case defaultAgentID = "default_agent_id"
        case memberCount = "member_count"
        case canSend = "can_send"
        case updatedAtUnixMs = "updated_at_unix_ms"
        case lastMessage = "last_message"
    }
}

public struct LocalConnectorCompanionAgentMemberSummary: Codable, Sendable, Equatable, Identifiable {
    public var id: String { agent.id }
    public let agent: LocalConnectorCompanionAgentSummary
    public let role: String
    public let responsibility: String

    public init(
        agent: LocalConnectorCompanionAgentSummary,
        role: String,
        responsibility: String
    ) {
        self.agent = agent
        self.role = role
        self.responsibility = responsibility
    }
}

public struct LocalConnectorCompanionAgentConversationDetail: Codable, Sendable, Equatable {
    public let conversation: LocalConnectorCompanionAgentConversationSummary
    public let members: [LocalConnectorCompanionAgentMemberSummary]

    public init(
        conversation: LocalConnectorCompanionAgentConversationSummary,
        members: [LocalConnectorCompanionAgentMemberSummary]
    ) {
        self.conversation = conversation
        self.members = members
    }
}

public struct LocalConnectorCompanionAgentWorkspace: Codable, Sendable, Equatable {
    public let teams: [LocalConnectorCompanionAgentConversationSummary]
    public let directConversations: [LocalConnectorCompanionAgentConversationSummary]
    public let agents: [LocalConnectorCompanionAgentSummary]

    public init(
        teams: [LocalConnectorCompanionAgentConversationSummary],
        directConversations: [LocalConnectorCompanionAgentConversationSummary],
        agents: [LocalConnectorCompanionAgentSummary]
    ) {
        self.teams = teams
        self.directConversations = directConversations
        self.agents = agents
    }

    enum CodingKeys: String, CodingKey {
        case teams, agents
        case directConversations = "direct_conversations"
    }
}

public struct LocalConnectorCompanionAgentMessagePage: Codable, Sendable, Equatable {
    public let messages: [LocalConnectorCompanionAgentMessage]
    public let nextCursorMessageID: String?
    public let hasMore: Bool

    public init(
        messages: [LocalConnectorCompanionAgentMessage],
        nextCursorMessageID: String?,
        hasMore: Bool
    ) {
        self.messages = messages
        self.nextCursorMessageID = nextCursorMessageID
        self.hasMore = hasMore
    }

    enum CodingKeys: String, CodingKey {
        case messages
        case nextCursorMessageID = "next_cursor_message_id"
        case hasMore = "has_more"
    }
}

public struct LocalConnectorCompanionAgentSendResponse: Codable, Sendable, Equatable {
    public let accepted: Bool
    public let message: LocalConnectorCompanionAgentMessage
    public let deduplicated: Bool

    public init(
        accepted: Bool = true,
        message: LocalConnectorCompanionAgentMessage,
        deduplicated: Bool
    ) {
        self.accepted = accepted
        self.message = message
        self.deduplicated = deduplicated
    }
}

@MainActor
public protocol LocalConnectorCompanionRuntimeProviding: AnyObject, Sendable {
    func companionResources() -> [LocalConnectorCompanionResource]
    func resolveCompanionResource(id: String) async throws -> LocalConnectorCompanionResource
}
