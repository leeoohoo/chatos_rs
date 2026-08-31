import Foundation

public actor ConversationHistoryStore {
    private struct SessionState: Sendable {
        var turnsByID: [String: ConversationTurn] = [:]
        var olderCursor: String?
        var hasOlder = false
        var snapshotRevision: Int64 = 0
        var newestAcceptedLatestGeneration: Int64 = 0
        var newestAcceptedOlderGeneration: Int64 = 0
        var hasLoadedOlderPage = false
        var lastAppliedEventSequence: Int64 = 0
        var appliedEventIDs: Set<String> = []
        var viewportAnchor: ViewportAnchor?
        var unreadNewerCount = 0
        var supersededExecutionGroupIDs: Set<String> = []
    }

    private var sessions: [String: SessionState] = [:]

    public init() {}

    public func mergeCachedTurns(_ turns: [ConversationTurn], sessionID: String) {
        var state = sessions[sessionID] ?? SessionState()
        merge(turns, sessionID: sessionID, into: &state)
        sessions[sessionID] = state
    }

    public func mergePage(
        _ page: HistoryPage,
        sessionID: String,
        origin: ConversationHistoryPageOrigin = .latest
    ) {
        var state = sessions[sessionID] ?? SessionState()
        let didChange = merge(page.turns, sessionID: sessionID, into: &state)

        if didChange,
           origin == .latest,
           state.viewportAnchor?.isPinnedToBottom == false {
            state.unreadNewerCount += 1
        }

        switch origin {
        case .latest:
            if page.requestGeneration >= state.newestAcceptedLatestGeneration {
                state.newestAcceptedLatestGeneration = page.requestGeneration
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
        let didChange = merge([event.turn], sessionID: sessionID, into: &state)

        if didChange, userIsReadingOlderContent {
            state.unreadNewerCount += 1
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

    @discardableResult
    private func merge(
        _ incomingTurns: [ConversationTurn],
        sessionID: String,
        into state: inout SessionState
    ) -> Bool {
        var didChange = false

        let newlySuperseded = Set(
            incomingTurns.compactMap {
                $0.projectExecutionContext?.replacedExecutionGroupID?.trimmedNonEmpty
            }
        )
        if !newlySuperseded.isEmpty {
            state.supersededExecutionGroupIDs.formUnion(newlySuperseded)
            for (turnID, existingTurn) in state.turnsByID {
                guard state.supersededExecutionGroupIDs.contains(existingTurn.executionGroupIdentity),
                      existingTurn.isTaskGraphAvailable else {
                    continue
                }
                var updatedTurn = existingTurn
                updatedTurn.isTaskGraphAvailable = false
                state.turnsByID[turnID] = updatedTurn
                didChange = true
            }
        }

        for incomingTurn in incomingTurns {
            var turn = incomingTurn
            guard turn.sessionID == sessionID else { continue }
            if state.supersededExecutionGroupIDs.contains(turn.executionGroupIdentity) {
                turn.isTaskGraphAvailable = false
            }

            guard let existing = state.turnsByID[turn.id] else {
                state.turnsByID[turn.id] = turn
                didChange = true
                continue
            }

            if turn.revision > existing.revision {
                if !existing.isTaskGraphAvailable {
                    turn.isTaskGraphAvailable = false
                }
                state.turnsByID[turn.id] = turn
                didChange = true
            }
        }

        return didChange
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

    var executionGroupIdentity: String {
        projectExecutionContext?.executionGroupID?.trimmedNonEmpty ?? userMessage.id
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
