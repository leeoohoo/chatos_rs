import ChatOSCore
import Foundation
import Testing

struct ConversationHistoryUseCaseTests {
    @Test
    func latestPageUsesFixedQueryAndReturnsMergedSnapshot() async throws {
        let page = HistoryPage(
            turns: [turn(id: "latest", sequence: 2)],
            olderCursor: "10",
            hasOlder: true,
            snapshotRevision: 7,
            requestGeneration: 3
        )
        let remote = ConversationHistoryRemoteStub(pages: [page])
        let store = ConversationHistoryStore()
        let useCase = ConversationHistoryUseCase(
            remoteService: remote,
            historyStore: store
        )

        let snapshot = try await useCase.loadLatest(
            sessionID: "session-1",
            requestGeneration: 3
        )

        #expect(snapshot.turns.map(\.id) == ["latest"])
        #expect(snapshot.olderCursor == "10")
        #expect(snapshot.hasOlder)
        #expect(await remote.queries == [
            ConversationHistoryQuery(
                sessionID: "session-1",
                limit: 10,
                requestGeneration: 3
            ),
        ])
    }

    @Test
    func olderPageUsesCursorAndPreservesExistingLatestTurns() async throws {
        let latest = HistoryPage(
            turns: [turn(id: "latest", sequence: 2)],
            olderCursor: "10",
            hasOlder: true,
            snapshotRevision: 1,
            requestGeneration: 1
        )
        let older = HistoryPage(
            turns: [turn(id: "older", sequence: 1)],
            olderCursor: nil,
            hasOlder: false,
            snapshotRevision: 2,
            requestGeneration: 2
        )
        let remote = ConversationHistoryRemoteStub(pages: [latest, older])
        let store = ConversationHistoryStore()
        let useCase = ConversationHistoryUseCase(
            remoteService: remote,
            historyStore: store
        )
        _ = try await useCase.loadLatest(
            sessionID: "session-1",
            requestGeneration: 1
        )

        let snapshot = try await useCase.loadOlder(
            sessionID: "session-1",
            before: "10",
            requestGeneration: 2
        )

        #expect(snapshot.turns.map(\.id) == ["older", "latest"])
        #expect(!snapshot.hasOlder)
        #expect(await remote.queries.last?.before == "10")
        #expect(await remote.queries.last?.limit == 10)
    }

    @Test
    func retryScheduleRemainsBounded() {
        #expect(ConversationHistoryUseCase.retryDelay(at: 0) == .seconds(2))
        #expect(ConversationHistoryUseCase.retryDelay(at: 1) == .seconds(5))
        #expect(ConversationHistoryUseCase.retryDelay(at: 2) == .seconds(10))
        #expect(ConversationHistoryUseCase.retryDelay(at: 3) == .seconds(20))
        #expect(ConversationHistoryUseCase.retryDelay(at: 4) == nil)
        #expect(ConversationHistoryUseCase.retryDelay(at: -1) == nil)
    }

    private func turn(id: String, sequence: Int64) -> ConversationTurn {
        ConversationTurn(
            id: id,
            sessionID: "session-1",
            sequence: sequence,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-\(id)",
                role: .user,
                text: id,
                createdAt: Date(timeIntervalSince1970: TimeInterval(sequence))
            ),
            status: .completed,
            startedAt: Date(timeIntervalSince1970: TimeInterval(sequence))
        )
    }
}

private actor ConversationHistoryRemoteStub: ConversationRemoteServicing {
    private var pages: [HistoryPage]
    private(set) var queries: [ConversationHistoryQuery] = []

    init(pages: [HistoryPage]) {
        self.pages = pages
    }

    func fetchHistory(_ query: ConversationHistoryQuery) async throws -> HistoryPage {
        queries.append(query)
        return pages.removeFirst()
    }

    func issueWebSocketTicket() async throws -> String {
        "unused"
    }
}
