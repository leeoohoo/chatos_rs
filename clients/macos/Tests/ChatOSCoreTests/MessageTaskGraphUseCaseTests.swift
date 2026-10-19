import ChatOSCore
import Foundation
import Testing

struct MessageTaskGraphUseCaseTests {
    @Test
    func transientEmptyGraphDoesNotReplaceStableGraph() async throws {
        let stable = graph(taskID: "task-1", status: "running")
        let service = MessageTaskGraphUseCaseStub(graphs: [.empty])
        let useCase = MessageTaskGraphUseCase(graphService: service)

        let loaded = try await useCase.loadGraph(
            messageID: "message-1",
            lookup: lookup,
            preservingStable: stable
        )

        #expect(loaded == stable)
        #expect(await service.graphRequestCount == 1)
    }

    @Test
    func initialEmptyAndLaterNonemptyGraphsRemainObservable() async throws {
        let populated = graph(taskID: "task-1", status: "completed")
        let service = MessageTaskGraphUseCaseStub(graphs: [.empty, populated])
        let useCase = MessageTaskGraphUseCase(graphService: service)

        let initial = try await useCase.loadGraph(
            messageID: "message-1",
            lookup: lookup,
            preservingStable: nil
        )
        let recovered = try await useCase.loadGraph(
            messageID: "message-1",
            lookup: lookup,
            preservingStable: initial
        )

        #expect(initial.nodes.isEmpty)
        #expect(recovered == populated)
    }

    @Test
    func realtimeRefreshPolicyMatchesMacBehavior() {
        let useCase = MessageTaskGraphUseCase(
            graphService: MessageTaskGraphUseCaseStub(graphs: [])
        )

        #expect(useCase.shouldRefreshWorkspace(
            for: signal(kind: .reconcile, turnID: nil),
            turnID: "turn-1"
        ))
        #expect(useCase.shouldRefreshWorkspace(
            for: signal(kind: .completed, turnID: "turn-1"),
            turnID: "turn-1"
        ))
        #expect(useCase.shouldRefreshWorkspace(
            for: signal(kind: .persisted, turnID: "turn-1"),
            turnID: "turn-1"
        ))
        #expect(!useCase.shouldRefreshWorkspace(
            for: signal(kind: .updated, turnID: "turn-1"),
            turnID: "turn-1"
        ))
        #expect(!useCase.shouldRefreshWorkspace(
            for: signal(kind: .failed, turnID: "turn-2"),
            turnID: "turn-1"
        ))
    }

    @Test
    func taskActivityPolicyIncludesReviewAndConfirmationGates() {
        #expect(MessageTask(id: "task", title: "Task", status: " reviewing_followup ").isActive)
        #expect(MessageTask(id: "task", title: "Task", status: "WAITING_CONFIRMATION").isActive)
        #expect(!MessageTask(id: "task", title: "Task", status: "cancelled").isActive)
        #expect(MessageTask(id: "task", title: "Task", status: nil).normalizedStatus == "unknown")
    }

    @Test
    func taskLoadingAndRetryStayBehindApplicationBoundary() async throws {
        let task = MessageTask(
            id: "task-1",
            title: "Task",
            status: "failed",
            lastRunID: "run-1"
        )
        let service = MessageTaskGraphUseCaseStub(graphs: [], task: task)
        let useCase = MessageTaskGraphUseCase(graphService: service)

        let loaded = try await useCase.loadTask(
            messageID: "message-1",
            taskID: "task-1",
            lookup: lookup
        )
        let retried = try await useCase.retryRun(
            messageID: "message-1",
            runID: "run-1",
            lookup: lookup,
            instruction: "Use the latest context"
        )

        #expect(loaded == task)
        #expect(retried.id == "run-1")
        #expect(await service.taskRequests == ["message-1:task-1"])
        #expect(await service.retryRequests == ["message-1:run-1:Use the latest context"])
    }

    private var lookup: MessageTaskLookup {
        MessageTaskLookup(
            sessionID: "session-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        )
    }

    private func graph(taskID: String, status: String) -> MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: [taskID],
            nodes: [
                MessageTaskGraphNode(
                    task: MessageTask(id: taskID, title: "Task", status: status),
                    depth: 0,
                    isRoot: true,
                    isCurrentMessage: true
                ),
            ],
            edges: [],
            sourceSessionID: "session-1",
            sourceTurnID: "turn-1",
            sourceUserMessageID: "message-1"
        )
    }

    private func signal(
        kind: ConversationRealtimeKind,
        turnID: String?
    ) -> ConversationRealtimeSignal {
        ConversationRealtimeSignal(
            eventID: UUID().uuidString,
            eventSequence: 1,
            sessionID: "session-1",
            turnID: turnID,
            kind: kind,
            eventName: kind.rawValue,
            timestamp: "2026-10-19T00:00:00Z"
        )
    }
}

private actor MessageTaskGraphUseCaseStub: MessageTaskGraphServicing {
    private var graphs: [MessageTaskGraphSnapshot]
    private let task: MessageTask?
    private(set) var graphRequestCount = 0
    private(set) var taskRequests: [String] = []
    private(set) var retryRequests: [String] = []

    init(graphs: [MessageTaskGraphSnapshot], task: MessageTask? = nil) {
        self.graphs = graphs
        self.task = task
    }

    func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        graphRequestCount += 1
        return graphs.removeFirst()
    }

    func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        taskRequests.append("\(messageID):\(taskID)")
        return task ?? MessageTask(id: taskID, title: "Task")
    }

    func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        fatalError("unused")
    }

    func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        retryRequests.append("\(messageID):\(runID):\(instruction ?? "")")
        return MessageTaskRun(id: runID, taskID: task?.id ?? "task-1", status: "queued")
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

private extension MessageTaskGraphSnapshot {
    static var empty: MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(rootTaskIDs: [], nodes: [], edges: [])
    }
}
