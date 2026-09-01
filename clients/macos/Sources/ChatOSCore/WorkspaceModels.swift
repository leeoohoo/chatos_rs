import Foundation

public struct WorkspaceProject: Sendable, Equatable, Identifiable {
    public var id: String
    public var name: String
    public var rootPath: String?
    public var displayRootPath: String?
    public var latestConversationID: String?

    public init(
        id: String,
        name: String,
        rootPath: String?,
        displayRootPath: String? = nil,
        latestConversationID: String?
    ) {
        self.id = id
        self.name = name
        self.rootPath = rootPath
        self.displayRootPath = displayRootPath
        self.latestConversationID = latestConversationID
    }
}

public struct WorkspaceContact: Sendable, Equatable, Identifiable {
    public var id: String
    public var agentID: String
    public var name: String
    public var status: String?

    public init(id: String, agentID: String, name: String, status: String?) {
        self.id = id
        self.agentID = agentID
        self.name = name
        self.status = status
    }
}

public struct WorkspaceConversation: Sendable, Equatable, Identifiable {
    public var id: String
    public var title: String
    public var projectID: String?
    public var contactID: String?
    public var contactAgentID: String?
    public var messageCount: Int
    public var updatedAt: Date
    public var isArchived: Bool

    public init(
        id: String,
        title: String,
        projectID: String?,
        contactID: String?,
        contactAgentID: String?,
        messageCount: Int,
        updatedAt: Date,
        isArchived: Bool
    ) {
        self.id = id
        self.title = title
        self.projectID = projectID
        self.contactID = contactID
        self.contactAgentID = contactAgentID
        self.messageCount = messageCount
        self.updatedAt = updatedAt
        self.isArchived = isArchived
    }
}

public struct WorkspaceSnapshot: Sendable, Equatable {
    public var projects: [WorkspaceProject]
    public var contacts: [WorkspaceContact]
    public var conversations: [WorkspaceConversation]

    public init(
        projects: [WorkspaceProject],
        contacts: [WorkspaceContact],
        conversations: [WorkspaceConversation]
    ) {
        self.projects = projects
        self.contacts = contacts
        self.conversations = conversations
    }
}

public protocol WorkspaceRemoteServicing: Sendable {
    func fetchWorkspace() async throws -> WorkspaceSnapshot
}
