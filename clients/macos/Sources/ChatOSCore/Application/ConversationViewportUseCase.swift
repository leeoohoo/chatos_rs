public enum ConversationViewportPolicy {
    public static func shouldPublishPinnedState(
        currentIsPinned: Bool,
        detectedIsPinned: Bool,
        unreadNewerCount: Int
    ) -> Bool {
        currentIsPinned != detectedIsPinned
            || (detectedIsPinned && unreadNewerCount > 0)
    }
}

/// Application use case for persisted timeline position and unread-content state.
public struct ConversationViewportUseCase: Sendable {
    private let historyStore: any ConversationHistoryStoring

    public init(historyStore: any ConversationHistoryStoring) {
        self.historyStore = historyStore
    }

    public func updateTimelineAnchor(
        sessionID: String,
        latestTurnID: String?,
        isPinnedToBottom: Bool
    ) async -> ConversationHistorySnapshot {
        await historyStore.setViewportAnchor(
            ViewportAnchor(
                turnID: latestTurnID ?? sessionID,
                relativeOffset: 0,
                isPinnedToBottom: isPinnedToBottom
            ),
            sessionID: sessionID
        )
        return await historyStore.snapshot(sessionID: sessionID)
    }

    public func markNewerContentRead(
        sessionID: String
    ) async -> ConversationHistorySnapshot {
        await historyStore.markNewerContentRead(sessionID: sessionID)
        return await historyStore.snapshot(sessionID: sessionID)
    }
}
