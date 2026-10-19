import Foundation

/// Application use case for Conversation history pagination.
///
/// Remote fetch, ordered store merge, and snapshot read are one operation so UI clients
/// cannot accidentally render a fetched page before it has passed through history rules.
public struct ConversationHistoryUseCase: Sendable {
    public static let pageSize = 10

    private static let retryDelays: [Duration] = [
        .seconds(2),
        .seconds(5),
        .seconds(10),
        .seconds(20),
    ]

    private let remoteService: any ConversationRemoteServicing
    private let historyStore: any ConversationHistoryStoring

    public init(
        remoteService: any ConversationRemoteServicing,
        historyStore: any ConversationHistoryStoring
    ) {
        self.remoteService = remoteService
        self.historyStore = historyStore
    }

    public func loadLatest(
        sessionID: String,
        requestGeneration: Int64
    ) async throws -> ConversationHistorySnapshot {
        try await load(
            sessionID: sessionID,
            before: nil,
            requestGeneration: requestGeneration,
            origin: .latest
        )
    }

    public func loadOlder(
        sessionID: String,
        before cursor: String,
        requestGeneration: Int64
    ) async throws -> ConversationHistorySnapshot {
        try await load(
            sessionID: sessionID,
            before: cursor,
            requestGeneration: requestGeneration,
            origin: .older
        )
    }

    public static func retryDelay(at attempt: Int) -> Duration? {
        guard retryDelays.indices.contains(attempt) else { return nil }
        return retryDelays[attempt]
    }

    private func load(
        sessionID: String,
        before: String?,
        requestGeneration: Int64,
        origin: ConversationHistoryPageOrigin
    ) async throws -> ConversationHistorySnapshot {
        let page = try await remoteService.fetchHistory(
            ConversationHistoryQuery(
                sessionID: sessionID,
                limit: Self.pageSize,
                before: before,
                requestGeneration: requestGeneration
            )
        )
        await historyStore.mergePage(
            page,
            sessionID: sessionID,
            origin: origin
        )
        return await historyStore.snapshot(sessionID: sessionID)
    }
}
