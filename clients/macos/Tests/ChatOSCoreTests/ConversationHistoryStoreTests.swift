// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation
import Testing
@testable import ChatOSCore

@Suite("Local-only conversation history store")
struct ConversationHistoryStoreTests {
    @Test("accepts only idempotent unpersisted turns")
    func acceptsOnlyOptimisticTurns() async throws {
        let store = ConversationHistoryStore()
        let optimistic = turn(id: "turn-1", sessionID: "session-a", sequence: 1)

        try await store.upsertOptimisticTurn(optimistic, sessionID: "session-a")
        try await store.upsertOptimisticTurn(optimistic, sessionID: "session-a")

        await #expect(throws: LocalAgentConversationHistoryError.invalidOptimisticTurn) {
            var changed = optimistic
            changed.userMessage.text = "conflicting text"
            try await store.upsertOptimisticTurn(changed, sessionID: "session-a")
        }
        await #expect(throws: LocalAgentConversationHistoryError.invalidOptimisticTurn) {
            var persisted = optimistic
            persisted.revision = 1
            try await store.upsertOptimisticTurn(persisted, sessionID: "session-a")
        }

        let snapshot = await store.snapshot(sessionID: "session-a")
        #expect(snapshot.turns == [optimistic])
    }

    @Test("discard removes only an unpersisted turn")
    func discardsOnlyOptimisticTurn() async throws {
        let store = ConversationHistoryStore()
        let optimistic = turn(id: "turn-1", sessionID: "session-a", sequence: 1)
        try await store.upsertOptimisticTurn(optimistic, sessionID: "session-a")

        await store.discardOptimisticTurn(sessionID: "session-a", turnID: optimistic.id)

        #expect(await store.snapshot(sessionID: "session-a").turns.isEmpty)
    }

    @Test("orders local turns and isolates viewport state by conversation")
    func ordersAndIsolatesConversationState() async throws {
        let store = ConversationHistoryStore()
        try await store.upsertOptimisticTurn(
            turn(id: "second", sessionID: "session-a", sequence: 2),
            sessionID: "session-a"
        )
        try await store.upsertOptimisticTurn(
            turn(id: "first", sessionID: "session-a", sequence: 1),
            sessionID: "session-a"
        )
        await store.setViewportAnchor(
            ViewportAnchor(turnID: "first", relativeOffset: 12, isPinnedToBottom: false),
            sessionID: "session-a"
        )

        let first = await store.snapshot(sessionID: "session-a")
        let second = await store.snapshot(sessionID: "session-b")
        #expect(first.turns.map(\.id) == ["first", "second"])
        #expect(first.viewportAnchor?.turnID == "first")
        #expect(second.turns.isEmpty)
        #expect(second.viewportAnchor == nil)
    }

    @Test("account reset removes optimistic messages and every conversation projection")
    func resetIsAccountIsolated() async throws {
        let store = ConversationHistoryStore()
        try await store.upsertOptimisticTurn(
            turn(id: "old-a", sessionID: "session-a", sequence: 1),
            sessionID: "session-a"
        )
        try await store.upsertOptimisticTurn(
            turn(id: "old-b", sessionID: "session-b", sequence: 1),
            sessionID: "session-b"
        )

        await store.reset()

        #expect(await store.snapshot(sessionID: "session-a").turns.isEmpty)
        #expect(await store.snapshot(sessionID: "session-b").turns.isEmpty)
        #expect(await store.snapshot(sessionID: "session-a").viewportAnchor == nil)
    }

    private func turn(id: String, sessionID: String, sequence: Int64) -> ConversationTurn {
        let createdAt = Date(timeIntervalSince1970: TimeInterval(sequence))
        return ConversationTurn(
            id: id,
            sessionID: sessionID,
            sequence: sequence,
            revision: 0,
            userMessage: ChatMessage(
                id: "message-\(id)",
                role: .user,
                text: id,
                createdAt: createdAt
            ),
            status: .streaming,
            startedAt: createdAt
        )
    }
}
