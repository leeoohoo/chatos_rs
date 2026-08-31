public struct ConversationHistoryQuery: Sendable, Equatable {
    public var sessionID: String
    public var limit: Int
    public var before: String?
    public var requestGeneration: Int64

    public init(
        sessionID: String,
        limit: Int = 10,
        before: String? = nil,
        requestGeneration: Int64
    ) {
        self.sessionID = sessionID
        self.limit = limit
        self.before = before
        self.requestGeneration = requestGeneration
    }
}

public protocol ConversationRemoteServicing: Sendable {
    func fetchHistory(_ query: ConversationHistoryQuery) async throws -> HistoryPage
    func issueWebSocketTicket() async throws -> String
}
