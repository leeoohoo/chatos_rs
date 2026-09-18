import Foundation

public enum ProjectAgentRoomStatus: String, Codable, Sendable {
    case active, archived
}

/// Project teams and private conversations share the same durable transcript, unread cursor,
/// delivery queue and Relay MCP. The kind only controls participants, routing and project access.
public enum LocalAgentConversationKind: String, Codable, Sendable {
    case projectTeam = "project_team"
    case humanAgentDirect = "human_agent_direct"
    case agentAgentDirect = "agent_agent_direct"

    public var isDirect: Bool { self != .projectTeam }
}

public struct ProjectAgentRoomDraft: Codable, Sendable, Equatable {
    public let name: String
    public let goal: String

    public init(name: String, goal: String = "") {
        self.name = name
        self.goal = goal
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(name, field: "name", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(goal, field: "goal", maximumLength: 8_000)
    }
}

public struct ProjectAgentRoom: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let projectID: String
    public let draft: ProjectAgentRoomDraft
    public let defaultAgentID: String?
    /// Explicit project-management authority for the shared team Todo board. This is independent
    /// from `defaultAgentID`, which only routes unmentioned chat messages.
    public let projectManagerAgentID: String?
    public let conversationKind: LocalAgentConversationKind
    public let directKey: String?
    public let status: ProjectAgentRoomStatus
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        id: String,
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft,
        defaultAgentID: String? = nil,
        projectManagerAgentID: String? = nil,
        conversationKind: LocalAgentConversationKind = .projectTeam,
        directKey: String? = nil,
        status: ProjectAgentRoomStatus = .active,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.draft = draft
        self.defaultAgentID = defaultAgentID
        self.projectManagerAgentID = projectManagerAgentID
        self.conversationKind = conversationKind
        self.directKey = directKey
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "id")
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try draft.validate()
        if let defaultAgentID {
            try AgentGroupChatValidation.identifier(defaultAgentID, field: "defaultAgentID")
        }
        if let projectManagerAgentID {
            try AgentGroupChatValidation.identifier(
                projectManagerAgentID,
                field: "projectManagerAgentID"
            )
            guard conversationKind == .projectTeam else {
                throw AgentGroupChatError.invalidField("projectManagerAgentID")
            }
        }
        switch conversationKind {
        case .projectTeam:
            guard directKey == nil else {
                throw AgentGroupChatError.invalidField("directKey")
            }
        case .humanAgentDirect, .agentAgentDirect:
            guard let directKey else {
                throw AgentGroupChatError.invalidField("directKey")
            }
            try AgentGroupChatValidation.identifier(directKey, field: "directKey")
        }
        try AgentGroupChatValidation.timestamps(createdAtUnixMs, updatedAtUnixMs)
    }
}

public enum ProjectAgentRoomMemberStatus: String, Codable, Sendable {
    case active, removed
}

public struct ProjectAgentRoomMemberDraft: Codable, Sendable, Equatable {
    public let role: String
    public let responsibility: String
    public let pluginAllowlist: [String]

    public init(role: String, responsibility: String = "", pluginAllowlist: [String] = []) {
        self.role = role
        self.responsibility = responsibility
        self.pluginAllowlist = pluginAllowlist
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(role, field: "role", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(
            responsibility,
            field: "responsibility",
            maximumLength: 8_000
        )
        try AgentGroupChatValidation.identifiers(
            pluginAllowlist,
            field: "pluginAllowlist",
            maximumCount: 100
        )
    }
}

public struct ProjectAgentRoomMember: Codable, Sendable, Equatable, Identifiable {
    public var id: String { "\(roomID):\(agentID)" }
    public let ownerUserID: String
    public let roomID: String
    public let agentID: String
    public let draft: ProjectAgentRoomMemberDraft
    public let status: ProjectAgentRoomMemberStatus
    public let joinedAtUnixMs: Int64

    public init(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        draft: ProjectAgentRoomMemberDraft,
        status: ProjectAgentRoomMemberStatus = .active,
        joinedAtUnixMs: Int64
    ) {
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.agentID = agentID
        self.draft = draft
        self.status = status
        self.joinedAtUnixMs = joinedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try draft.validate()
        guard joinedAtUnixMs >= 0 else { throw AgentGroupChatError.invalidField("joinedAtUnixMs") }
    }
}

public struct LocalAgentMembershipUpdateResult: Codable, Sendable, Equatable {
    public let profile: LocalAgentProfile
    public let member: ProjectAgentRoomMember

    public init(profile: LocalAgentProfile, member: ProjectAgentRoomMember) {
        self.profile = profile
        self.member = member
    }
}

public enum ProjectAgentMessageSenderKind: String, Codable, Sendable {
    case human, agent, system
}

public struct ProjectAgentMessageAttachmentDraft: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let mimeType: String
    public let kind: ConversationAttachmentKind
    public let origin: ConversationAttachmentOrigin
    public let data: Data

    public init(
        id: String = UUID().uuidString.lowercased(),
        name: String,
        mimeType: String,
        kind: ConversationAttachmentKind,
        origin: ConversationAttachmentOrigin,
        data: Data
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.kind = kind
        self.origin = origin
        self.data = data
    }

    public init(_ attachment: ConversationAttachmentDraft) {
        self.init(
            id: attachment.id,
            name: attachment.name,
            mimeType: attachment.mimeType,
            kind: attachment.kind,
            origin: attachment.origin,
            data: attachment.data
        )
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "attachment.id")
        try AgentGroupChatValidation.text(name, field: "attachment.name", maximumLength: 512)
        try AgentGroupChatValidation.text(
            mimeType,
            field: "attachment.mimeType",
            maximumLength: 255
        )
        guard !data.isEmpty, data.count <= 20 * 1_024 * 1_024 else {
            throw AgentGroupChatError.invalidField("attachment.data")
        }
    }
}

public struct ProjectAgentMessageAttachment: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let mimeType: String
    public let size: Int
    public let kind: ConversationAttachmentKind
    public let origin: ConversationAttachmentOrigin
    public let sha256: String?
    public let syncStatus: ProjectAgentMessageAttachmentSyncStatus
    public let artifactID: String?
    public let storageProvider: String?
    public let bucket: String?
    public let objectKey: String?
    public let remoteViewPath: String?
    public let uploadError: String?
    public let syncedAtUnixMs: Int64?

    public init(
        id: String,
        name: String,
        mimeType: String,
        size: Int,
        kind: ConversationAttachmentKind,
        origin: ConversationAttachmentOrigin,
        sha256: String? = nil,
        syncStatus: ProjectAgentMessageAttachmentSyncStatus = .localOnly,
        artifactID: String? = nil,
        storageProvider: String? = nil,
        bucket: String? = nil,
        objectKey: String? = nil,
        remoteViewPath: String? = nil,
        uploadError: String? = nil,
        syncedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.kind = kind
        self.origin = origin
        self.sha256 = sha256
        self.syncStatus = syncStatus
        self.artifactID = artifactID
        self.storageProvider = storageProvider
        self.bucket = bucket
        self.objectKey = objectKey
        self.remoteViewPath = remoteViewPath
        self.uploadError = uploadError
        self.syncedAtUnixMs = syncedAtUnixMs
    }

    private enum CodingKeys: String, CodingKey {
        case id, name, mimeType, size, kind, origin, sha256, syncStatus, artifactID
        case storageProvider, bucket, objectKey, remoteViewPath, uploadError, syncedAtUnixMs
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decode(String.self, forKey: .id)
        name = try values.decode(String.self, forKey: .name)
        mimeType = try values.decode(String.self, forKey: .mimeType)
        size = try values.decode(Int.self, forKey: .size)
        kind = try values.decode(ConversationAttachmentKind.self, forKey: .kind)
        origin = try values.decode(ConversationAttachmentOrigin.self, forKey: .origin)
        sha256 = try values.decodeIfPresent(String.self, forKey: .sha256)
        syncStatus = try values.decodeIfPresent(
            ProjectAgentMessageAttachmentSyncStatus.self,
            forKey: .syncStatus
        ) ?? .localOnly
        artifactID = try values.decodeIfPresent(String.self, forKey: .artifactID)
        storageProvider = try values.decodeIfPresent(String.self, forKey: .storageProvider)
        bucket = try values.decodeIfPresent(String.self, forKey: .bucket)
        objectKey = try values.decodeIfPresent(String.self, forKey: .objectKey)
        remoteViewPath = try values.decodeIfPresent(String.self, forKey: .remoteViewPath)
        uploadError = try values.decodeIfPresent(String.self, forKey: .uploadError)
        syncedAtUnixMs = try values.decodeIfPresent(Int64.self, forKey: .syncedAtUnixMs)
    }
}

public struct ProjectAgentMessageAttachmentPayload: Sendable, Equatable {
    public let attachment: ProjectAgentMessageAttachment
    public let localFileURL: URL

    public init(attachment: ProjectAgentMessageAttachment, localFileURL: URL) {
        self.attachment = attachment
        self.localFileURL = localFileURL
    }
}

public struct ProjectAgentMessageDraft: Codable, Sendable, Equatable {
    public let senderKind: ProjectAgentMessageSenderKind
    public let senderID: String
    public let content: String
    public let mentionedAgentIDs: [String]
    public let replyToMessageID: String?
    public let sourceRunID: String?
    public let causationID: String?
    public let rootMessageID: String?
    public let hopCount: Int
    public let attachments: [ProjectAgentMessageAttachmentDraft]?

    public init(
        senderKind: ProjectAgentMessageSenderKind,
        senderID: String,
        content: String,
        mentionedAgentIDs: [String] = [],
        replyToMessageID: String? = nil,
        sourceRunID: String? = nil,
        causationID: String? = nil,
        rootMessageID: String? = nil,
        hopCount: Int = 0,
        attachments: [ProjectAgentMessageAttachmentDraft] = []
    ) {
        self.senderKind = senderKind
        self.senderID = senderID
        self.content = content
        self.mentionedAgentIDs = mentionedAgentIDs
        self.replyToMessageID = replyToMessageID
        self.sourceRunID = sourceRunID
        self.causationID = causationID
        self.rootMessageID = rootMessageID
        self.hopCount = hopCount
        self.attachments = attachments.isEmpty ? nil : attachments
    }

    public var attachmentItems: [ProjectAgentMessageAttachmentDraft] { attachments ?? [] }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(senderID, field: "senderID")
        if content.isEmpty, !attachmentItems.isEmpty {
            // An attachment-only Human message is valid and renders without placeholder text.
        } else {
            try AgentGroupChatValidation.text(content, field: "content", maximumLength: 64_000)
        }
        try AgentGroupChatValidation.identifiers(
            mentionedAgentIDs,
            field: "mentionedAgentIDs",
            maximumCount: 32
        )
        for (value, field) in [
            (replyToMessageID, "replyToMessageID"),
            (sourceRunID, "sourceRunID"),
            (causationID, "causationID"),
            (rootMessageID, "rootMessageID"),
        ] where value != nil {
            try AgentGroupChatValidation.identifier(value!, field: field)
        }
        guard (0...64).contains(hopCount) else {
            throw AgentGroupChatError.invalidField("hopCount")
        }
        guard attachmentItems.count <= 20,
              attachmentItems.reduce(0, { $0 + $1.data.count }) <= 20 * 1_024 * 1_024,
              Set(attachmentItems.map(\.id)).count == attachmentItems.count else {
            throw AgentGroupChatError.invalidField("attachments")
        }
        try attachmentItems.forEach { try $0.validate() }
    }
}

public struct ProjectAgentMessage: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let senderKind: ProjectAgentMessageSenderKind
    public let senderID: String
    public let content: String
    public let mentionedAgentIDs: [String]
    public let replyToMessageID: String?
    public let sourceRunID: String?
    public let causationID: String?
    public let rootMessageID: String
    public let hopCount: Int
    public let createdAtUnixMs: Int64
    public let attachments: [ProjectAgentMessageAttachment]?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft,
        rootMessageID: String,
        attachments: [ProjectAgentMessageAttachment] = [],
        createdAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        senderKind = draft.senderKind
        senderID = draft.senderID
        content = draft.content
        mentionedAgentIDs = draft.mentionedAgentIDs
        replyToMessageID = draft.replyToMessageID
        sourceRunID = draft.sourceRunID
        causationID = draft.causationID
        self.rootMessageID = rootMessageID
        hopCount = draft.hopCount
        self.attachments = attachments.isEmpty ? nil : attachments
        self.createdAtUnixMs = createdAtUnixMs
    }

    public var attachmentItems: [ProjectAgentMessageAttachment] { attachments ?? [] }
}

/// Stable room pagination uses the persisted message identity instead of a timestamp-only cursor.
/// Message ids disambiguate messages written in the same millisecond and are resolved inside the
/// identity-bound room by the store.
public struct ProjectAgentMessagePage: Codable, Sendable, Equatable {
    public let messages: [ProjectAgentMessage]
    public let nextCursorMessageID: String?
    public let hasMore: Bool

    public init(
        messages: [ProjectAgentMessage],
        nextCursorMessageID: String?,
        hasMore: Bool
    ) {
        self.messages = messages
        self.nextCursorMessageID = nextCursorMessageID
        self.hasMore = hasMore
    }
}

/// Each Agent owns an independent read cursor for each room. The cursor is monotonic and cannot
/// be moved backwards by a stale or retried MCP call.
public struct ProjectAgentReadCursor: Codable, Sendable, Equatable {
    public let ownerUserID: String
    public let roomID: String
    public let agentID: String
    public let messageID: String
    public let messageCreatedAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        messageID: String,
        messageCreatedAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.agentID = agentID
        self.messageID = messageID
        self.messageCreatedAtUnixMs = messageCreatedAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

public struct ProjectAgentUnreadPage: Codable, Sendable, Equatable {
    public let messages: [ProjectAgentMessage]
    public let nextCursorMessageID: String?
    public let hasMore: Bool
    public let readThroughMessageID: String?

    public init(
        messages: [ProjectAgentMessage],
        nextCursorMessageID: String?,
        hasMore: Bool,
        readThroughMessageID: String?
    ) {
        self.messages = messages
        self.nextCursorMessageID = nextCursorMessageID
        self.hasMore = hasMore
        self.readThroughMessageID = readThroughMessageID
    }
}

/// One conversation batch returned by the account-wide Agent inbox. IDs remain inside the
/// client; Relay converts them to run-scoped opaque references before exposing the batch.
public struct LocalAgentUnreadConversation: Codable, Sendable, Equatable {
    public let room: ProjectAgentRoom
    public let messages: [ProjectAgentMessage]

    public init(room: ProjectAgentRoom, messages: [ProjectAgentMessage]) {
        self.room = room
        self.messages = messages
    }
}
