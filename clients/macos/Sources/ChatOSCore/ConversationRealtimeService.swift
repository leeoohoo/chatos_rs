public enum ConversationRealtimeKind: String, Sendable, Equatable {
    case started
    case updated
    case persisted
    case completed
    case failed
    case cancelled
    case unknown
}

public struct ConversationRealtimeProcessUpdate: Sendable, Equatable, Identifiable {
    public var id: String
    public var title: String
    public var detail: String?
    public var status: String
    public var timestamp: String

    public init(
        id: String,
        title: String,
        detail: String? = nil,
        status: String,
        timestamp: String
    ) {
        self.id = id
        self.title = title
        self.detail = detail
        self.status = status
        self.timestamp = timestamp
    }
}

public struct ConversationRealtimeSignal: Sendable, Equatable {
    public var eventID: String
    public var eventSequence: Int64
    public var sessionID: String
    public var turnID: String?
    public var kind: ConversationRealtimeKind
    public var eventName: String
    public var timestamp: String
    public var askUserPromptUpdate: AskUserPromptRealtimeUpdate?
    public var processUpdate: ConversationRealtimeProcessUpdate?

    public init(
        eventID: String,
        eventSequence: Int64,
        sessionID: String,
        turnID: String?,
        kind: ConversationRealtimeKind,
        eventName: String,
        timestamp: String,
        askUserPromptUpdate: AskUserPromptRealtimeUpdate? = nil,
        processUpdate: ConversationRealtimeProcessUpdate? = nil
    ) {
        self.eventID = eventID
        self.eventSequence = eventSequence
        self.sessionID = sessionID
        self.turnID = turnID
        self.kind = kind
        self.eventName = eventName
        self.timestamp = timestamp
        self.askUserPromptUpdate = askUserPromptUpdate
        self.processUpdate = processUpdate
    }
}

public protocol ConversationRealtimeStreaming: Sendable {
    func events(
        sessionID: String
    ) async -> AsyncThrowingStream<ConversationRealtimeSignal, Error>
}
