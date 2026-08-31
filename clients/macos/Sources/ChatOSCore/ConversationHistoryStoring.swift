public protocol ConversationHistoryStoring: Sendable {
    func mergeCachedTurns(_ turns: [ConversationTurn], sessionID: String) async
    func mergePage(
        _ page: HistoryPage,
        sessionID: String,
        origin: ConversationHistoryPageOrigin
    ) async
    func applyRealtime(_ event: RealtimeTurnEvent, userIsReadingOlderContent: Bool) async
    func setViewportAnchor(_ anchor: ViewportAnchor?, sessionID: String) async
    func markNewerContentRead(sessionID: String) async
    func discardOptimisticTurn(sessionID: String, turnID: String) async
    func snapshot(sessionID: String) async -> ConversationHistorySnapshot
}

public extension ConversationHistoryStoring {
    func mergePage(_ page: HistoryPage, sessionID: String) async {
        await mergePage(page, sessionID: sessionID, origin: .latest)
    }
}

extension ConversationHistoryStore: ConversationHistoryStoring {}
