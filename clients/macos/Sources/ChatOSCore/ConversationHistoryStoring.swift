public protocol ConversationHistoryStoring: Sendable {
    func upsertOptimisticTurn(_ turn: ConversationTurn, sessionID: String) async throws
    func setViewportAnchor(_ anchor: ViewportAnchor?, sessionID: String) async
    func markNewerContentRead(sessionID: String) async
    func discardOptimisticTurn(sessionID: String, turnID: String) async
    func snapshot(sessionID: String) async -> ConversationHistorySnapshot
}

extension ConversationHistoryStore: ConversationHistoryStoring {}
