import Foundation

public actor ConversationHistoryStore {
    private struct SessionState: Sendable {
        var turnsByID: [String: ConversationTurn] = [:]
        var olderCursor: String?
        var hasOlder = false
        var snapshotRevision: Int64 = 0
        var newestAcceptedLatestGeneration: Int64 = 0
        var newestAcceptedOlderGeneration: Int64 = 0
        var hasAcceptedLatestPage = false
        var hasLoadedOlderPage = false
        var lastAppliedEventSequence: Int64 = 0
        var appliedEventIDs: Set<String> = []
        var viewportAnchor: ViewportAnchor?
        var unreadNewerCount = 0
    }

    private var sessions: [String: SessionState] = [:]

    public init() {}

    public func mergeCachedTurns(_ turns: [ConversationTurn], sessionID: String) {
        var state = sessions[sessionID] ?? SessionState()
        merge(
            turns,
            sessionID: sessionID,
            replacingChangedEqualRevisions: false,
            into: &state
        )
        sessions[sessionID] = state
    }

    public func mergePage(
        _ page: HistoryPage,
        sessionID: String,
        origin: ConversationHistoryPageOrigin = .latest
    ) {
        var state = sessions[sessionID] ?? SessionState()
        let acceptsLatestSnapshot = origin == .latest
            && page.requestGeneration >= state.newestAcceptedLatestGeneration
        let mergeResult = merge(
            page.turns,
            sessionID: sessionID,
            replacingChangedEqualRevisions: acceptsLatestSnapshot,
            into: &state
        )

        if state.hasAcceptedLatestPage,
           origin == .latest,
           state.viewportAnchor?.isPinnedToBottom == false,
           mergeResult.newContentCount > 0 {
            state.unreadNewerCount += mergeResult.newContentCount
        }

        switch origin {
        case .latest:
            if page.requestGeneration >= state.newestAcceptedLatestGeneration {
                state.newestAcceptedLatestGeneration = page.requestGeneration
                state.hasAcceptedLatestPage = true
                if !state.hasLoadedOlderPage {
                    state.olderCursor = page.olderCursor
                    state.hasOlder = page.hasOlder
                }
            }
        case .older:
            if page.requestGeneration >= state.newestAcceptedOlderGeneration {
                state.newestAcceptedOlderGeneration = page.requestGeneration
                state.hasLoadedOlderPage = true
                state.olderCursor = page.olderCursor
                state.hasOlder = page.hasOlder
            }
        }
        state.snapshotRevision = max(state.snapshotRevision, page.snapshotRevision)

        sessions[sessionID] = state
    }

    public func applyRealtime(_ event: RealtimeTurnEvent, userIsReadingOlderContent: Bool) {
        let sessionID = event.turn.sessionID
        var state = sessions[sessionID] ?? SessionState()

        guard !state.appliedEventIDs.contains(event.eventID) else {
            return
        }

        state.appliedEventIDs.insert(event.eventID)
        state.lastAppliedEventSequence = max(state.lastAppliedEventSequence, event.eventSequence)
        let mergeResult = merge(
            [event.turn],
            sessionID: sessionID,
            replacingChangedEqualRevisions: false,
            into: &state
        )

        if userIsReadingOlderContent, mergeResult.newContentCount > 0 {
            state.unreadNewerCount += mergeResult.newContentCount
        }

        sessions[sessionID] = state
    }

    public func setViewportAnchor(_ anchor: ViewportAnchor?, sessionID: String) {
        var state = sessions[sessionID] ?? SessionState()
        state.viewportAnchor = anchor
        if anchor?.isPinnedToBottom == true {
            state.unreadNewerCount = 0
        }
        sessions[sessionID] = state
    }

    public func markNewerContentRead(sessionID: String) {
        var state = sessions[sessionID] ?? SessionState()
        state.unreadNewerCount = 0
        sessions[sessionID] = state
    }

    public func discardOptimisticTurn(sessionID: String, turnID: String) {
        var state = sessions[sessionID] ?? SessionState()
        guard state.turnsByID[turnID]?.revision == 0 else { return }
        state.turnsByID.removeValue(forKey: turnID)
        sessions[sessionID] = state
    }

    public func snapshot(sessionID: String) -> ConversationHistorySnapshot {
        let state = sessions[sessionID] ?? SessionState()
        return ConversationHistorySnapshot(
            sessionID: sessionID,
            turns: state.turnsByID.values.sorted(by: ConversationTurn.isOrderedBefore),
            olderCursor: state.olderCursor,
            hasOlder: state.hasOlder,
            snapshotRevision: state.snapshotRevision,
            viewportAnchor: state.viewportAnchor,
            unreadNewerCount: state.unreadNewerCount
        )
    }

    private struct MergeResult {
        var newContentCount = 0
    }

    @discardableResult
    private func merge(
        _ incomingTurns: [ConversationTurn],
        sessionID: String,
        replacingChangedEqualRevisions: Bool,
        into state: inout SessionState
    ) -> MergeResult {
        var result = MergeResult()

        for incomingTurn in incomingTurns {
            var turn = incomingTurn
            guard turn.sessionID == sessionID else { continue }

            guard let existing = state.turnsByID[turn.id] else {
                state.turnsByID[turn.id] = turn
                result.newContentCount += 1
                continue
            }

            if turn.revision > existing.revision
                || (replacingChangedEqualRevisions
                    && turn.revision == existing.revision
                    && turn != existing) {
                if !existing.isTaskGraphAvailable {
                    turn.isTaskGraphAvailable = false
                }
                result.newContentCount += Self.newReplyCount(from: existing, to: turn)
                state.turnsByID[turn.id] = turn
            }
        }

        return result
    }

    private static func newReplyCount(
        from existing: ConversationTurn,
        to incoming: ConversationTurn
    ) -> Int {
        let existingIDs = visibleReplyIDs(for: existing)
        return visibleReplyIDs(for: incoming).subtracting(existingIDs).count
    }

    private static func visibleReplyIDs(for turn: ConversationTurn) -> Set<String> {
        if !turn.assistantReplies.isEmpty {
            return Set(turn.assistantReplies.map(\.id))
        }
        return Set(turn.finalAssistantMessage.map { [$0.id] } ?? [])
    }
}

private extension ConversationTurn {
    static func isOrderedBefore(_ lhs: ConversationTurn, _ rhs: ConversationTurn) -> Bool {
        let lhsHasStartedAt = lhs.startedAt != .distantPast
        let rhsHasStartedAt = rhs.startedAt != .distantPast

        // Some older gateways do not return sequence_no. The mapper can only assign a
        // page-local fallback in that case, so sequence values repeat after loading an
        // older page. A real creation time is therefore the stable cross-page order.
        if lhsHasStartedAt, rhsHasStartedAt, lhs.startedAt != rhs.startedAt {
            return lhs.startedAt < rhs.startedAt
        }

        // Preserve server ordering for legacy records with no usable timestamp and use
        // it as the tie-breaker when multiple turns share the same creation time.
        if lhs.sequence != rhs.sequence {
            return lhs.sequence < rhs.sequence
        }

        return lhs.id < rhs.id
    }
}
