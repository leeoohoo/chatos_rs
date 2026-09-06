import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class ConversationSessionViewModelTests: XCTestCase {
    func testReconnectReconcileRefreshesConversationHistory() async throws {
        let remoteService = ConversationRemoteServiceStub()
        let realtimeService = ConversationRealtimeServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            remoteService: remoteService,
            realtimeService: realtimeService
        )

        try await waitUntil {
            let hasSubscriber = await realtimeService.hasSubscriber(sessionID: "session-1")
            return viewModel.turns.first?.revision == 1
                && !viewModel.isRefreshing
                && hasSubscriber
        }

        await realtimeService.yield(
            ConversationRealtimeSignal(
                eventID: "reconcile-1",
                eventSequence: 0,
                sessionID: "session-1",
                turnID: nil,
                kind: .reconcile,
                eventName: "conversation.reconcile",
                timestamp: "2026-09-03T08:00:00Z"
            )
        )

        try await waitUntil {
            viewModel.turns.first?.revision == 2
                && viewModel.turns.first?.finalAssistantMessage?.text == "任务已完成"
        }

        let requestedSessionIDs = await remoteService.requestedSessionIDs()
        XCTAssertEqual(requestedSessionIDs, ["session-1", "session-1"])
    }

    private func waitUntil(
        timeoutIterations: Int = 100,
        condition: @escaping @MainActor () async -> Bool
    ) async throws {
        for _ in 0..<timeoutIterations {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTFail("Timed out waiting for asynchronous conversation state")
    }
}

private actor ConversationRemoteServiceStub: ConversationRemoteServicing {
    private var queries: [ConversationHistoryQuery] = []

    func fetchHistory(_ query: ConversationHistoryQuery) async throws -> HistoryPage {
        queries.append(query)
        let revision = Int64(queries.count)
        return HistoryPage(
            turns: [Self.turn(revision: revision)],
            olderCursor: nil,
            hasOlder: false,
            snapshotRevision: revision,
            requestGeneration: query.requestGeneration
        )
    }

    func issueWebSocketTicket() async throws -> String {
        "ticket"
    }

    func requestedSessionIDs() -> [String] {
        queries.map(\.sessionID)
    }

    private static func turn(revision: Int64) -> ConversationTurn {
        ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
            sequence: 1,
            revision: revision,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行任务",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            finalAssistantMessage: revision > 1
                ? ChatMessage(
                    id: "assistant-1",
                    role: .assistant,
                    text: "任务已完成",
                    createdAt: Date(timeIntervalSince1970: 2)
                )
                : nil,
            status: revision > 1 ? .completed : .streaming,
            startedAt: Date(timeIntervalSince1970: 1),
            completedAt: revision > 1 ? Date(timeIntervalSince1970: 2) : nil
        )
    }
}

private actor ConversationRealtimeServiceStub: ConversationRealtimeStreaming {
    private var continuations: [
        String: AsyncThrowingStream<ConversationRealtimeSignal, Error>.Continuation
    ] = [:]

    func events(
        sessionID: String
    ) async -> AsyncThrowingStream<ConversationRealtimeSignal, Error> {
        let (stream, continuation) = AsyncThrowingStream.makeStream(
            of: ConversationRealtimeSignal.self,
            throwing: Error.self
        )
        continuations[sessionID] = continuation
        return stream
    }

    func hasSubscriber(sessionID: String) -> Bool {
        continuations[sessionID] != nil
    }

    func yield(_ signal: ConversationRealtimeSignal) {
        continuations[signal.sessionID]?.yield(signal)
    }
}
