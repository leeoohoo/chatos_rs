import ChatOSCore
import Foundation
import Testing

struct ConversationViewportUseCaseTests {
    @Test
    func timelineAnchorUsesLatestTurnAndClearsUnreadWhenPinned() async {
        let store = ConversationHistoryStore()
        let useCase = ConversationViewportUseCase(historyStore: store)
        await store.applyRealtime(
            RealtimeTurnEvent(
                eventID: "event-1",
                eventSequence: 1,
                turn: turn(id: "turn-1")
            ),
            userIsReadingOlderContent: true
        )

        let snapshot = await useCase.updateTimelineAnchor(
            sessionID: "session-1",
            latestTurnID: "turn-1",
            isPinnedToBottom: true
        )

        #expect(snapshot.viewportAnchor == ViewportAnchor(
            turnID: "turn-1",
            relativeOffset: 0,
            isPinnedToBottom: true
        ))
        #expect(snapshot.unreadNewerCount == 0)
    }

    @Test
    func emptyTimelineUsesSessionAsStableAnchor() async {
        let useCase = ConversationViewportUseCase(
            historyStore: ConversationHistoryStore()
        )

        let snapshot = await useCase.updateTimelineAnchor(
            sessionID: "session-1",
            latestTurnID: nil,
            isPinnedToBottom: false
        )

        #expect(snapshot.viewportAnchor?.turnID == "session-1")
        #expect(snapshot.viewportAnchor?.isPinnedToBottom == false)
    }

    @Test
    func markReadReturnsUpdatedSnapshot() async {
        let store = ConversationHistoryStore()
        let useCase = ConversationViewportUseCase(historyStore: store)
        await store.applyRealtime(
            RealtimeTurnEvent(
                eventID: "event-1",
                eventSequence: 1,
                turn: turn(id: "turn-1")
            ),
            userIsReadingOlderContent: true
        )

        let snapshot = await useCase.markNewerContentRead(sessionID: "session-1")

        #expect(snapshot.unreadNewerCount == 0)
    }

    @Test(arguments: [
        (true, false, 0, true),
        (false, true, 0, true),
        (true, true, 1, true),
        (true, true, 0, false),
        (false, false, 1, false),
    ])
    func pinnedStatePublicationPolicy(
        current: Bool,
        detected: Bool,
        unread: Int,
        expected: Bool
    ) {
        #expect(ConversationViewportPolicy.shouldPublishPinnedState(
            currentIsPinned: current,
            detectedIsPinned: detected,
            unreadNewerCount: unread
        ) == expected)
    }

    private func turn(id: String) -> ConversationTurn {
        ConversationTurn(
            id: id,
            sessionID: "session-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            status: .streaming,
            startedAt: Date(timeIntervalSince1970: 1)
        )
    }
}
