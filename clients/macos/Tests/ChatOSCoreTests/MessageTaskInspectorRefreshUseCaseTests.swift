import ChatOSCore
import Foundation
import Testing

struct MessageTaskInspectorRefreshUseCaseTests {
    @Test
    func taskWithoutRunClearsStaleRunForEverySection() async throws {
        let service = MessageTaskInspectorServiceStub(responses: [])
        let useCase = MessageTaskInspectorRefreshUseCase(graphService: service)
        let task = MessageTask(id: "task-1", title: "Task", status: "pending")

        for section in [
            MessageTaskInspectorSection.process,
            .detail,
            .run,
        ] {
            let update = try await useCase.refreshSection(
                section,
                messageID: "message-1",
                lookup: lookup,
                task: task
            )
            #expect(update == .clearRun)
        }
        #expect(await service.requests.isEmpty)
    }

    @Test
    func processSectionPreservesLoadedRunWithoutQueryingHost() async throws {
        let service = MessageTaskInspectorServiceStub(responses: [])
        let useCase = MessageTaskInspectorRefreshUseCase(graphService: service)

        let update = try await useCase.refreshSection(
            .process,
            messageID: "message-1",
            lookup: lookup,
            task: task
        )

        #expect(update == .preserve)
        #expect(await service.requests.isEmpty)
    }

    @Test
    func detailSectionLoadsOnlyModelOutput() async throws {
        let service = MessageTaskInspectorServiceStub(
            responses: [runDetail(status: "completed")]
        )
        let useCase = MessageTaskInspectorRefreshUseCase(graphService: service)

        let update = try await useCase.refreshSection(
            .detail,
            messageID: "message-1",
            lookup: lookup,
            task: task
        )

        guard case let .modelOutput(updatedTask, runID) = update else {
            Issue.record("Expected model output update")
            return
        }
        #expect(runID == "run-1")
        #expect(updatedTask.lastRunStatus == "completed")
        #expect(await service.requests == [
            .init(includeEvents: false, eventLimit: 1, eventOffset: 0),
        ])
    }

    @Test
    func runSectionLoadsLatestEventPage() async throws {
        let detail = runDetail(status: "running")
        let service = MessageTaskInspectorServiceStub(responses: [detail])
        let useCase = MessageTaskInspectorRefreshUseCase(graphService: service)

        let update = try await useCase.refreshSection(
            .run,
            messageID: "message-1",
            lookup: lookup,
            task: task
        )

        guard case let .run(updatedTask, loadedDetail) = update else {
            Issue.record("Expected Run Inspector update")
            return
        }
        #expect(loadedDetail == detail)
        #expect(updatedTask.lastRunID == "run-1")
        #expect(await service.requests == [
            .init(includeEvents: true, eventLimit: 40, eventOffset: 0),
        ])
    }

    private var lookup: MessageTaskLookup {
        MessageTaskLookup(
            sessionID: "session-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        )
    }

    private var task: MessageTask {
        MessageTask(
            id: "task-1",
            title: "Task",
            status: "running",
            lastRunID: "run-1"
        )
    }

    private func runDetail(status: String) -> MessageTaskRunDetail {
        MessageTaskRunDetail(
            task: task,
            run: MessageTaskRun(id: "run-1", taskID: "task-1", status: status),
            events: [MessageTaskRunEvent(id: "event-1", eventType: "progress")],
            eventsTotal: 1,
            eventsHasMore: false
        )
    }
}

private actor MessageTaskInspectorServiceStub: MessageTaskGraphServicing {
    struct Request: Sendable, Equatable {
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
