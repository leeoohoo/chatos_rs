import Foundation

public enum PetActivitySource: String, Sendable, Equatable, Codable {
    case localApproval
    case askUserPrompt
    case chat
    case taskBoard
    case taskRunner
    case projectExecution
}

public enum PetActivityKind: String, Sendable, Equatable, Codable {
    case working
    case reviewing
    case waitingForApproval
    case waitingForUser
    case succeeded
    case failed
    case blocked
    case cancelled

    public var animationState: PetAnimationState {
        switch self {
        case .working: .running
        case .reviewing: .review
        case .waitingForApproval, .waitingForUser: .waiting
        case .succeeded: .succeeded
        case .failed, .blocked: .failed
        case .cancelled: .idle
        }
    }

    public var requiresAttention: Bool {
        switch self {
        case .waitingForApproval, .waitingForUser, .failed, .blocked: true
        case .working, .reviewing, .succeeded, .cancelled: false
        }
    }

    var presentationPriority: Int {
        switch self {
        case .waitingForApproval, .waitingForUser: 500
        case .failed, .blocked: 400
        case .succeeded: 300
        case .reviewing: 200
        case .working: 100
        case .cancelled: 50
        }
    }
}

public enum PetActivityInboxStatus: String, Sendable, Equatable, Codable {
    case unread
    case displayed
    case acknowledged
    case ignored
    case handled
    case resolved
    case expired
}

public enum PetActivityDisposition: String, Sendable, Equatable, Codable {
    case acknowledged
    case ignored
    case handled
}

public enum PetAnimationState: String, Sendable, Equatable, Codable {
    case idle
    case running
    case review
    case waiting
    case succeeded
    case failed
}

public struct PetActivityRoute: Sendable, Equatable, Codable {
    public var projectID: String?
    public var conversationID: String?
    public var turnID: String?
    public var messageID: String?
    public var promptID: String?
    public var taskID: String?
    public var runID: String?

    public init(
        projectID: String? = nil,
        conversationID: String? = nil,
        turnID: String? = nil,
        messageID: String? = nil,
        promptID: String? = nil,
        taskID: String? = nil,
        runID: String? = nil
    ) {
        self.projectID = projectID
        self.conversationID = conversationID
        self.turnID = turnID
        self.messageID = messageID
        self.promptID = promptID
        self.taskID = taskID
        self.runID = runID
    }
}

public struct PetActivity: Identifiable, Sendable, Equatable, Codable {
    public var id: String
    public var source: PetActivitySource
    public var kind: PetActivityKind
    public var title: String
    public var detail: String?
    public var route: PetActivityRoute
    public var eventID: String?
    public var eventSequence: Int64?
    public var inboxID: String?
    public var inboxStatus: PetActivityInboxStatus?
    public var activityVersion: String?
    public var updatedAt: Date
    public var expiresAt: Date?

    public init(
        id: String,
        source: PetActivitySource,
        kind: PetActivityKind,
        title: String,
        detail: String? = nil,
        route: PetActivityRoute = .init(),
        eventID: String? = nil,
        eventSequence: Int64? = nil,
        inboxID: String? = nil,
        inboxStatus: PetActivityInboxStatus? = nil,
        activityVersion: String? = nil,
        updatedAt: Date = Date(),
        expiresAt: Date? = nil
    ) {
        self.id = id
        self.source = source
        self.kind = kind
        self.title = title
        self.detail = detail
        self.route = route
        self.eventID = eventID
        self.eventSequence = eventSequence
        self.inboxID = inboxID
        self.inboxStatus = inboxStatus
        self.activityVersion = activityVersion
        self.updatedAt = updatedAt
        self.expiresAt = expiresAt
    }
}

public enum PetActivityEvent: Sendable, Equatable {
    case upsert(PetActivity)
    case remove(id: String)
    case removeSource(PetActivitySource)
    case reconcile
}

public struct PetPresentation: Sendable, Equatable {
    public var animationState: PetAnimationState
    public var primaryActivity: PetActivity?
    public var activeWorkCount: Int
    public var attentionCount: Int

    public init(
        animationState: PetAnimationState,
        primaryActivity: PetActivity?,
        activeWorkCount: Int,
        attentionCount: Int
    ) {
        self.animationState = animationState
        self.primaryActivity = primaryActivity
        self.activeWorkCount = activeWorkCount
        self.attentionCount = attentionCount
    }

    public static let idle = PetPresentation(
        animationState: .idle,
        primaryActivity: nil,
        activeWorkCount: 0,
        attentionCount: 0
    )
}

public protocol PetActivityStreaming: Sendable {
    func petActivityEvents() async -> AsyncThrowingStream<PetActivityEvent, Error>
}
