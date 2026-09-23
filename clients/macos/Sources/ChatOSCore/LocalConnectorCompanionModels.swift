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

@MainActor
public protocol LocalConnectorCompanionRuntimeProviding: AnyObject, Sendable {
    func companionResources() -> [LocalConnectorCompanionResource]
    func resolveCompanionResource(id: String) async throws -> LocalConnectorCompanionResource
}
