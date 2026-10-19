import ChatOSCore
import Foundation
import Testing

struct MessageTaskRunUseCaseTests {
    @Test
    func latestRunRequestsNewestPagePolicy() async throws {
        let service = MessageTaskRunServiceStub(
            responses: [runDetail(eventIDs: ["newest"], hasMore: true)]
        )
        let useCase = MessageTaskRunUseCase(graphService: service)

        let loaded = try await useCase.loadLatestRun(
            messageID: "message-1",
            runID: "run-1",
            lookup: lookup
        )

        #expect(loaded.events.map(\.id) == ["newest"])
        #expect(loaded.eventsHasMore)
        #expect(await service.requests == [
            .init(
                messageID: "message-1",
                runID: "run-1",
                includeEvents: true,
                eventLimit: 40,
                eventOffset: 0
            ),
        ])
    }

    @Test
    func olderEventsUseCurrentCountAndMergeUniqueIDs() async throws {
        let page = runDetail(
            eventIDs: ["event-2", "event-3", "event-3", "event-4"],
            hasMore: true,
            status: "completed"
        )
        let service = MessageTaskRunServiceStub(responses: [page])
        let useCase = MessageTaskRunUseCase(graphService: service)
        let current = runDetail(
            eventIDs: ["event-1", "event-2"],
            hasMore: true,
            status: "running"
        )

        let merged = try await useCase.loadOlderEvents(
            messageID: "message-1",
            lookup: lookup,
            current: current
        )

        #expect(merged.events.map(\.id) == ["event-1", "event-2", "event-3", "event-4"])
        #expect(merged.eventsTotal == page.eventsTotal)
        #expect(merged.eventsHasMore)
        #expect(merged.run.status == "completed")
        #expect(await service.requests == [
            .init(
                messageID: "message-1",
                runID: "run-1",
                includeEvents: true,
                eventLimit: 50,
                eventOffset: 2
            ),
        ])
    }

    private var lookup: MessageTaskLookup {
        MessageTaskLookup(
            sessionID: "session-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        )
    }

    private func runDetail(
        eventIDs: [String],
        hasMore: Bool,
        status: String = "running"
    ) -> MessageTaskRunDetail {
        let task = MessageTask(
            id: "task-1",
            title: "Task",
            status: status,
            lastRunID: "run-1"
        )
        return MessageTaskRunDetail(
            task: task,
            run: MessageTaskRun(id: "run-1", taskID: task.id, status: status),
            events: eventIDs.map {
                MessageTaskRunEvent(id: $0, eventType: "progress")
            },
            eventsTotal: eventIDs.count + (hasMore ? 1 : 0),
            eventsHasMore: hasMore
        )
    }
}

private actor MessageTaskRunServiceStub: MessageTaskGraphServicing {
    struct Request: Sendable, Equatable {
        let messageID: String
        let runID: String
        let includeEvents: Bool
        let eventLimit: Int
        let eventOffset: Int
    }

    private var responses: [MessageTaskRunDetail]
    private(set) var requests: [Request] = []

    init(responses: [MessageTaskRunDetail]) {
        self.responses = responses
    }

    func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        fatalError("unused")
    }

    func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        fatalError("unused")
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        requests.append(
            .init(
                messageID: messageID,
                runID: runID,
                includeEvents: includeEvents,
                eventLimit: eventLimit,
                eventOffset: eventOffset
            )
        )
        return responses.removeFirst()
    }

    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        fatalError("unused")
    }

    func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {
        fatalError("unused")
    }
}
