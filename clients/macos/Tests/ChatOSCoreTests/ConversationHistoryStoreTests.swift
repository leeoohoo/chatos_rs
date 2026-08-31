import XCTest
@testable import ChatOSCore

final class ConversationHistoryStoreTests: XCTestCase {
    func testReplacementBatchKeepsHistoryAndDisablesOnlyOldTaskGraph() async {
        let store = ConversationHistoryStore()
        let old = projectExecutionTurn(id: "old-group", replacedGroupID: nil, sequence: 1)
        let replacement = projectExecutionTurn(
            id: "new-group",
            replacedGroupID: "old-group",
            sequence: 2
        )

        await store.mergeCachedTurns([old, replacement], sessionID: "session-a")

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.map(\.id), ["old-group", "new-group"])
        XCTAssertFalse(snapshot.turns[0].isTaskGraphAvailable)
        XCTAssertTrue(snapshot.turns[1].isTaskGraphAvailable)
    }

    func testOlderPageRestoresSupersededHistoryWithoutTaskGraph() async {
        let store = ConversationHistoryStore()
        let replacement = projectExecutionTurn(
            id: "new-group",
            replacedGroupID: "old-group",
            sequence: 2
        )
        await store.mergeCachedTurns([replacement], sessionID: "session-a")
        await store.mergeCachedTurns(
            [projectExecutionTurn(id: "old-group", replacedGroupID: nil, sequence: 1)],
            sessionID: "session-a"
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.map(\.id), ["old-group", "new-group"])
        XCTAssertFalse(snapshot.turns[0].isTaskGraphAvailable)
        XCTAssertTrue(snapshot.turns[1].isTaskGraphAvailable)
    }

    func testNewerRevisionCannotRestoreSupersededTaskGraphButton() async {
        let store = ConversationHistoryStore()
        let old = projectExecutionTurn(id: "old-group", replacedGroupID: nil, sequence: 1)
        let replacement = projectExecutionTurn(
            id: "new-group",
            replacedGroupID: "old-group",
            sequence: 2
        )
        await store.mergeCachedTurns([old, replacement], sessionID: "session-a")

        var refreshedOld = old
        refreshedOld.revision = 2
        refreshedOld.isTaskGraphAvailable = true
        await store.mergeCachedTurns([refreshedOld], sessionID: "session-a")

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.map(\.id), ["old-group", "new-group"])
        XCTAssertFalse(snapshot.turns[0].isTaskGraphAvailable)
    }

    func testDiscardOptimisticTurnNeverDeletesPersistedTurn() async {
        let store = ConversationHistoryStore()
        let optimistic = turn(
            id: "optimistic",
            sessionID: "session-a",
            sequence: 1,
            revision: 0
        )
        let persisted = turn(
            id: "persisted",
            sessionID: "session-a",
            sequence: 2,
            revision: 1
        )
        await store.mergeCachedTurns([optimistic, persisted], sessionID: "session-a")

        await store.discardOptimisticTurn(sessionID: "session-a", turnID: optimistic.id)
        await store.discardOptimisticTurn(sessionID: "session-a", turnID: persisted.id)

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.map(\.id), ["persisted"])
    }

    func testPageMergeNeverReplacesAlreadyLoadedTurns() async {
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns(
            [turn(id: "1", sequence: 1), turn(id: "2", sequence: 2)],
            sessionID: "session-a"
        )

        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "2", sequence: 2, revision: 2), turn(id: "3", sequence: 3)],
                olderCursor: "cursor-a",
                hasOlder: true,
                snapshotRevision: 10,
                requestGeneration: 1
            ),
            sessionID: "session-a"
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.map(\.id), ["1", "2", "3"])
        XCTAssertEqual(snapshot.turns[1].revision, 2)
    }

    func testOlderRevisionCannotOverwriteNewerTurn() async {
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns(
            [turn(id: "1", sequence: 1, revision: 8, assistantText: "new")],
            sessionID: "session-a"
        )
        await store.mergeCachedTurns(
            [turn(id: "1", sequence: 1, revision: 3, assistantText: "old")],
            sessionID: "session-a"
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.first?.revision, 8)
        XCTAssertEqual(snapshot.turns.first?.finalAssistantMessage?.text, "new")
    }

    func testStalePageGenerationCannotMoveCursorBackward() async {
        let store = ConversationHistoryStore()
        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "2", sequence: 2)],
                olderCursor: "new-cursor",
                hasOlder: true,
                snapshotRevision: 9,
                requestGeneration: 12
            ),
            sessionID: "session-a"
        )
        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "1", sequence: 1)],
                olderCursor: "stale-cursor",
                hasOlder: false,
                snapshotRevision: 4,
                requestGeneration: 8
            ),
            sessionID: "session-a"
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.map(\.id), ["1", "2"])
        XCTAssertEqual(snapshot.olderCursor, "new-cursor")
        XCTAssertTrue(snapshot.hasOlder)
    }

    func testLatestRefreshDoesNotResetCursorAfterLoadingOlderPage() async {
        let store = ConversationHistoryStore()
        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "11", sequence: 11), turn(id: "12", sequence: 12)],
                olderCursor: "turn-11",
                hasOlder: true,
                snapshotRevision: 1,
                requestGeneration: 1
            ),
            sessionID: "session-a",
            origin: .latest
        )
        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "9", sequence: 9), turn(id: "10", sequence: 10)],
                olderCursor: "turn-9",
                hasOlder: true,
                snapshotRevision: 2,
                requestGeneration: 2
            ),
            sessionID: "session-a",
            origin: .older
        )
        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "12", sequence: 12, revision: 2)],
                olderCursor: "turn-11",
                hasOlder: true,
                snapshotRevision: 3,
                requestGeneration: 3
            ),
            sessionID: "session-a",
            origin: .latest
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.olderCursor, "turn-9")
        XCTAssertTrue(snapshot.hasOlder)
        XCTAssertEqual(snapshot.turns.map(\.id), ["9", "10", "11", "12"])
        XCTAssertEqual(snapshot.turns.last?.revision, 2)
    }

    func testPagesWithoutGlobalSequenceRemainChronologicalAfterLoadingOlder() async {
        let store = ConversationHistoryStore()

        // Older gateways omit sequence_no, so each mapped page independently receives
        // the same 1...N fallback values. Those duplicate values must not interleave pages.
        await store.mergePage(
            HistoryPage(
                turns: [
                    turn(id: "latest-1", sequence: 1, timestamp: 300),
                    turn(id: "latest-2", sequence: 2, timestamp: 400),
                ],
                olderCursor: "older-page",
                hasOlder: true,
                snapshotRevision: 1,
                requestGeneration: 1
            ),
            sessionID: "session-a",
            origin: .latest
        )
        await store.mergePage(
            HistoryPage(
                turns: [
                    turn(id: "older-1", sequence: 1, timestamp: 100),
                    turn(id: "older-2", sequence: 2, timestamp: 200),
                ],
                olderCursor: nil,
                hasOlder: false,
                snapshotRevision: 2,
                requestGeneration: 2
            ),
            sessionID: "session-a",
            origin: .older
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(
            snapshot.turns.map(\.id),
            ["older-1", "older-2", "latest-1", "latest-2"]
        )
    }

    func testMissingCreationTimesFallBackToServerSequence() async {
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns(
            [
                turn(
                    id: "second",
                    sequence: 2,
                    timestamp: Date.distantPast.timeIntervalSince1970
                ),
                turn(
                    id: "first",
                    sequence: 1,
                    timestamp: Date.distantPast.timeIntervalSince1970
                ),
            ],
            sessionID: "session-a"
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.map(\.id), ["first", "second"])
    }

    func testSessionViewportAndUnreadStateAreIsolated() async {
        let store = ConversationHistoryStore()
        await store.setViewportAnchor(
            ViewportAnchor(turnID: "a-1", relativeOffset: 18, isPinnedToBottom: false),
            sessionID: "session-a"
        )
        await store.applyRealtime(
            RealtimeTurnEvent(
                eventID: "event-1",
                eventSequence: 1,
                turn: turn(id: "a-2", sessionID: "session-a", sequence: 2)
            ),
            userIsReadingOlderContent: true
        )
        await store.applyRealtime(
            RealtimeTurnEvent(
                eventID: "event-1",
                eventSequence: 1,
                turn: turn(id: "a-2", sessionID: "session-a", sequence: 2)
            ),
            userIsReadingOlderContent: true
        )

        let first = await store.snapshot(sessionID: "session-a")
        let second = await store.snapshot(sessionID: "session-b")
        XCTAssertEqual(first.viewportAnchor?.turnID, "a-1")
        XCTAssertEqual(first.unreadNewerCount, 1)
        XCTAssertNil(second.viewportAnchor)
        XCTAssertEqual(second.unreadNewerCount, 0)
    }

    func testDifferentEventIDForSameRevisionDoesNotCreateUnreadNoise() async {
        let store = ConversationHistoryStore()
        let stableTurn = turn(id: "1", sequence: 1, revision: 3)

        await store.applyRealtime(
            RealtimeTurnEvent(eventID: "event-1", eventSequence: 1, turn: stableTurn),
            userIsReadingOlderContent: true
        )
        await store.applyRealtime(
            RealtimeTurnEvent(eventID: "event-2", eventSequence: 2, turn: stableTurn),
            userIsReadingOlderContent: true
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.count, 1)
        XCTAssertEqual(snapshot.unreadNewerCount, 1)
    }

    func testLatestPageUpdateCreatesUnreadWhenViewportIsNotPinned() async {
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns(
            [turn(id: "1", sequence: 1, revision: 1)],
            sessionID: "session-a"
        )
        await store.setViewportAnchor(
            ViewportAnchor(turnID: "1", relativeOffset: 0, isPinnedToBottom: false),
            sessionID: "session-a"
        )

        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "1", sequence: 1, revision: 2)],
                olderCursor: nil,
                hasOlder: false,
                snapshotRevision: 2,
                requestGeneration: 1
            ),
            sessionID: "session-a",
            origin: .latest
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.unreadNewerCount, 1)
    }

    func testLatestPageUpdateStaysReadWhenViewportIsPinned() async {
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns(
            [turn(id: "1", sequence: 1, revision: 1)],
            sessionID: "session-a"
        )
        await store.setViewportAnchor(
            ViewportAnchor(turnID: "1", relativeOffset: 0, isPinnedToBottom: true),
            sessionID: "session-a"
        )

        await store.mergePage(
            HistoryPage(
                turns: [turn(id: "1", sequence: 1, revision: 2)],
                olderCursor: nil,
                hasOlder: false,
                snapshotRevision: 2,
                requestGeneration: 1
            ),
            sessionID: "session-a",
            origin: .latest
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.unreadNewerCount, 0)
    }

    func testEqualRevisionCannotOverwriteAcceptedTurn() async {
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns(
            [turn(id: "1", sequence: 1, revision: 5, assistantText: "accepted")],
            sessionID: "session-a"
        )
        await store.mergeCachedTurns(
            [turn(id: "1", sequence: 1, revision: 5, assistantText: "conflicting replay")],
            sessionID: "session-a"
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertEqual(snapshot.turns.first?.finalAssistantMessage?.text, "accepted")
    }

    func testTurnFromAnotherSessionCannotLeakIntoSnapshot() async {
        let store = ConversationHistoryStore()
        await store.mergeCachedTurns(
            [turn(id: "foreign", sessionID: "session-b", sequence: 1)],
            sessionID: "session-a"
        )

        let snapshot = await store.snapshot(sessionID: "session-a")
        XCTAssertTrue(snapshot.turns.isEmpty)
    }

    private func turn(
        id: String,
        sessionID: String = "session-a",
        sequence: Int64,
        revision: Int64 = 1,
        assistantText: String = "assistant",
        timestamp: TimeInterval? = nil
    ) -> ConversationTurn {
        let date = timestamp.map(Date.init(timeIntervalSince1970:))
            ?? Date(timeIntervalSince1970: TimeInterval(sequence))
        return ConversationTurn(
            id: id,
            sessionID: sessionID,
            sequence: sequence,
            revision: revision,
            userMessage: ChatMessage(id: "user-\(id)", role: .user, text: "user", createdAt: date),
            finalAssistantMessage: ChatMessage(
                id: "assistant-\(id)",
                role: .assistant,
                text: assistantText,
                createdAt: date
            ),
            status: .completed,
            startedAt: date,
            completedAt: date
        )
    }

    private func projectExecutionTurn(
        id: String,
        replacedGroupID: String?,
        sequence: Int64
    ) -> ConversationTurn {
        let date = Date(timeIntervalSince1970: TimeInterval(sequence))
        return ConversationTurn(
            id: id,
            sessionID: "session-a",
            sequence: sequence,
            revision: 1,
            userMessage: ChatMessage(id: id, role: .user, text: "执行批次", createdAt: date),
            projectExecutionContext: ProjectExecutionContext(
                projectID: "project-1",
                requirementID: "requirement-1",
                executionGroupID: id,
                replacedExecutionGroupID: replacedGroupID
            ),
            status: .completed,
            startedAt: date,
            completedAt: date
        )
    }
}
