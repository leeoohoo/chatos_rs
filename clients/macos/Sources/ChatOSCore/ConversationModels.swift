import Foundation

public enum TurnStatus: String, Codable, Sendable, CaseIterable {
    case queued
    case streaming
    case completed
    case failed
    case cancelled
}

public struct ChatMessage: Identifiable, Codable, Sendable, Equatable {
    public enum Role: String, Codable, Sendable {
        case user
        case assistant
        case system
    }

    public let id: String
    public let role: Role
    public var text: String
    public var createdAt: Date
    public var attachments: [ConversationAttachmentReference]

    public init(
        id: String,
        role: Role,
        text: String,
        createdAt: Date,
        attachments: [ConversationAttachmentReference] = []
    ) {
        self.id = id
        self.role = role
        self.text = text
        self.createdAt = createdAt
        self.attachments = attachments
    }
}

public struct TurnProcessEvent: Identifiable, Codable, Sendable, Equatable {
    public let id: String
    public var title: String
    public var detail: String?
    public var status: TurnStatus

    public init(id: String, title: String, detail: String? = nil, status: TurnStatus) {
        self.id = id
        self.title = title
        self.detail = detail
        self.status = status
    }
}

public struct TaskRunnerCallbackReference: Codable, Sendable, Equatable {
    public var taskID: String
    public var runID: String?
    public var event: String?
    public var status: String?
    public var sourceSessionID: String?
    public var sourceTurnID: String?
    public var sourceUserMessageID: String?

    public init(
        taskID: String,
        runID: String? = nil,
        event: String? = nil,
        status: String? = nil,
        sourceSessionID: String? = nil,
        sourceTurnID: String? = nil,
        sourceUserMessageID: String? = nil
    ) {
        self.taskID = taskID
        self.runID = runID
        self.event = event
        self.status = status
        self.sourceSessionID = sourceSessionID
        self.sourceTurnID = sourceTurnID
        self.sourceUserMessageID = sourceUserMessageID
    }
}

public struct ConversationAssistantReply: Identifiable, Codable, Sendable, Equatable {
    public var id: String { message.id }
    public var message: ChatMessage
    public var taskCallback: TaskRunnerCallbackReference?

    public init(
        message: ChatMessage,
        taskCallback: TaskRunnerCallbackReference? = nil
    ) {
        self.message = message
        self.taskCallback = taskCallback
    }
}

public struct ConversationTurn: Identifiable, Codable, Sendable, Equatable {
    public let id: String
    public let sessionID: String
    public var sequence: Int64
    public var revision: Int64
    public var userMessage: ChatMessage
    public var processEvents: [TurnProcessEvent]
    public var finalAssistantMessage: ChatMessage?
    public var assistantReplies: [ConversationAssistantReply]
    public var messageTaskLookup: MessageTaskLookup?
    public var projectExecutionContext: ProjectExecutionContext?
    public var isTaskGraphAvailable: Bool
    public var status: TurnStatus
    public var startedAt: Date
    public var completedAt: Date?

    public init(
        id: String,
        sessionID: String,
        sequence: Int64,
        revision: Int64,
        userMessage: ChatMessage,
        processEvents: [TurnProcessEvent] = [],
        finalAssistantMessage: ChatMessage? = nil,
        assistantReplies: [ConversationAssistantReply] = [],
        messageTaskLookup: MessageTaskLookup? = nil,
        projectExecutionContext: ProjectExecutionContext? = nil,
        isTaskGraphAvailable: Bool = true,
        status: TurnStatus,
        startedAt: Date,
        completedAt: Date? = nil
    ) {
        self.id = id
        self.sessionID = sessionID
        self.sequence = sequence
        self.revision = revision
        self.userMessage = userMessage
        self.processEvents = processEvents
        self.finalAssistantMessage = finalAssistantMessage
        self.assistantReplies = assistantReplies
        self.messageTaskLookup = messageTaskLookup
        self.projectExecutionContext = projectExecutionContext
        self.isTaskGraphAvailable = isTaskGraphAvailable
        self.status = status
        self.startedAt = startedAt
        self.completedAt = completedAt
    }
}

public struct ViewportAnchor: Codable, Sendable, Equatable {
    public var turnID: String
    public var relativeOffset: Double
    public var isPinnedToBottom: Bool

    public init(turnID: String, relativeOffset: Double, isPinnedToBottom: Bool) {
        self.turnID = turnID
        self.relativeOffset = relativeOffset
        self.isPinnedToBottom = isPinnedToBottom
    }
}

public struct HistoryPage: Sendable, Equatable {
    public var turns: [ConversationTurn]
    public var olderCursor: String?
    public var hasOlder: Bool
    public var snapshotRevision: Int64
    public var requestGeneration: Int64

    public init(
        turns: [ConversationTurn],
        olderCursor: String?,
        hasOlder: Bool,
        snapshotRevision: Int64,
        requestGeneration: Int64
    ) {
        self.turns = turns
        self.olderCursor = olderCursor
        self.hasOlder = hasOlder
        self.snapshotRevision = snapshotRevision
        self.requestGeneration = requestGeneration
    }
}

public enum ConversationHistoryPageOrigin: Sendable, Equatable {
    case latest
    case older
}

public struct RealtimeTurnEvent: Sendable, Equatable {
    public var eventID: String
    public var eventSequence: Int64
    public var turn: ConversationTurn

    public init(eventID: String, eventSequence: Int64, turn: ConversationTurn) {
        self.eventID = eventID
        self.eventSequence = eventSequence
        self.turn = turn
    }
}

public struct ConversationHistorySnapshot: Sendable, Equatable {
    public var sessionID: String
    public var turns: [ConversationTurn]
    public var olderCursor: String?
    public var hasOlder: Bool
    public var snapshotRevision: Int64
    public var viewportAnchor: ViewportAnchor?
    public var unreadNewerCount: Int

    public init(
        sessionID: String,
        turns: [ConversationTurn],
        olderCursor: String?,
        hasOlder: Bool,
        snapshotRevision: Int64,
        viewportAnchor: ViewportAnchor?,
        unreadNewerCount: Int
    ) {
        self.sessionID = sessionID
        self.turns = turns
        self.olderCursor = olderCursor
        self.hasOlder = hasOlder
        self.snapshotRevision = snapshotRevision
        self.viewportAnchor = viewportAnchor
        self.unreadNewerCount = unreadNewerCount
    }
}
