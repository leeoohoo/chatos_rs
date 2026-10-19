import Foundation

/// Application use case for the Run Inspector event timeline.
///
/// Pagination policy belongs here rather than in SwiftUI: the first page always contains
/// the latest events, while subsequent pages request older events using the number already
/// displayed as their offset. Connector adapters remain responsible for mapping Host data
/// and reporting whether another page exists.
public struct MessageTaskRunUseCase: Sendable {
    public static let latestEventLimit = 40
    public static let olderEventLimit = 50

    private let graphService: any MessageTaskGraphServicing

    public init(graphService: any MessageTaskGraphServicing) {
        self.graphService = graphService
    }

    public func loadLatestRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskRunDetail {
        try await graphService.fetchRun(
            messageID: messageID,
            runID: runID,
            lookup: lookup,
            includeEvents: true,
            eventLimit: Self.latestEventLimit,
            eventOffset: 0
        )
    }

    public func loadOlderEvents(
        messageID: String,
        lookup: MessageTaskLookup?,
        current: MessageTaskRunDetail
    ) async throws -> MessageTaskRunDetail {
        let page = try await graphService.fetchRun(
            messageID: messageID,
            runID: current.run.id,
            lookup: lookup,
            includeEvents: true,
            eventLimit: Self.olderEventLimit,
            eventOffset: current.events.count
        )

        var merged = current
        var eventIDs = Set(current.events.map(\.id))
        merged.events.append(contentsOf: page.events.filter { eventIDs.insert($0.id).inserted })
        merged.eventsTotal = page.eventsTotal
        merged.eventsHasMore = page.eventsHasMore
        merged.task = page.task
        merged.run = page.run
        return merged
    }
}
