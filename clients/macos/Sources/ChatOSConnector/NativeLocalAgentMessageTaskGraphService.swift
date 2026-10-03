import ChatOSCore
import Foundation

public actor NativeLocalAgentMessageTaskGraphService: MessageTaskGraphServicing {
    private let client: NativeLocalAgentTaskClient
    private var ownerUserID: String?

    public init(host: any LocalAgentHostClientServicing) {
        self.client = NativeLocalAgentTaskClient(host: host)
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
    }

    public func reset() {
        ownerUserID = nil
    }

    public func fetchGraph(
        messageID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTaskGraphSnapshot {
        let graphs = try await matchingGraphs(lookup: lookup)
        return mapGraphs(graphs, messageID: messageID, lookup: lookup)
    }

    public func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        let graphs = try await matchingGraphs(lookup: lookup, requiredTaskID: taskID)
        guard let graph = graphs.first(where: { graph in
            graph.tasks.contains(where: { $0.taskID == taskID })
        }), let task = graph.tasks.first(where: { $0.taskID == taskID }) else {
            throw NativeLocalAgentMessageTaskGraphServiceError.taskNotFound
        }
        return try await mapTask(task, graph: graph)
    }

    public func fetchRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        let owner = try requireOwner()
        let run = try await client.run(ownerUserID: owner, runID: runID)
        let task = try await fetchTask(
            messageID: messageID,
            taskID: run.ownerEntityID,
            lookup: lookup
        )
        let page = includeEvents
            ? try await client.events(
                ownerUserID: owner,
                runID: runID,
                limit: UInt32(max(1, min(100, eventOffset + eventLimit)))
            )
            : LocalAgentEventPage(events: [], nextCursor: 0)
        let offset = max(0, eventOffset)
        let limit = max(1, min(100, eventLimit))
        let selected = page.events.dropFirst(offset).prefix(limit)
        return MessageTaskRunDetail(
            task: task,
            run: mapRun(run),
            events: selected.map(mapEvent),
            eventsTotal: page.events.count,
            eventsHasMore: page.events.count > offset + selected.count
        )
    }

    public func retryRun(
        messageID: String,
        runID: String,
        lookup: MessageTaskLookup?,
        instruction: String?
    ) async throws -> MessageTaskRun {
        let owner = try requireOwner()
        let run = try await client.run(ownerUserID: owner, runID: runID)
        let graphs = try await matchingGraphs(
            lookup: lookup,
            requiredTaskID: run.ownerEntityID
        )
        guard let current = graphs.lazy.flatMap(\.tasks).first(where: {
            $0.taskID == run.ownerEntityID
        }) else {
            throw NativeLocalAgentMessageTaskGraphServiceError.taskNotFound
        }
        let graph = try await client.retry(
            ownerUserID: owner,
            taskID: current.taskID,
            expectedVersion: current.version,
            retryInstruction: instruction?.trimmingCharacters(in: .whitespacesAndNewlines)
                .nilIfEmpty
        )
        guard let retried = graph.tasks.first(where: { $0.taskID == current.taskID }) else {
            throw NativeLocalAgentMessageTaskGraphServiceError.taskNotFound
        }
        return MessageTaskRun(
            id: retried.activeRunID ?? "pending-\(retried.taskID)-\(retried.version)",
            taskID: retried.taskID,
            status: retried.status,
            startedAt: Self.date(retried.updatedAtUnixMs)
        )
    }

    public func cancelTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?,
        reason: String?
    ) async throws {
        let owner = try requireOwner()
        let graphs = try await matchingGraphs(lookup: lookup, requiredTaskID: taskID)
        guard let task = graphs.lazy.flatMap(\.tasks).first(where: { $0.taskID == taskID }) else {
            throw NativeLocalAgentMessageTaskGraphServiceError.taskNotFound
        }
        _ = try await client.cancel(
            ownerUserID: owner,
            taskID: taskID,
            expectedVersion: task.version,
            reason: reason?.trimmingCharacters(in: .whitespacesAndNewlines)
                .nilIfEmpty ?? "user requested cancellation"
        )
    }

    private func matchingGraphs(
        lookup: MessageTaskLookup?,
        requiredTaskID: String? = nil
    ) async throws -> [LocalAgentTaskGraph] {
        let owner = try requireOwner()
        var summaries: [LocalAgentTaskGraphSummary] = []
        var beforeTimestamp: Int64?
        var beforeID: String?
        repeat {
            let page = try await client.listGraphs(
                ownerUserID: owner,
                sourceEntityType: lookup?.turnID == nil ? nil : "conversation_turn",
                sourceEntityID: lookup?.turnID,
                beforeUpdatedAtUnixMs: beforeTimestamp,
                beforeGraphID: beforeID
            )
            summaries.append(contentsOf: page.graphs)
            beforeTimestamp = page.nextBeforeUpdatedAtUnixMs
            beforeID = page.nextBeforeGraphID
        } while beforeTimestamp != nil && summaries.count < 500

        var graphs: [LocalAgentTaskGraph] = []
        for summary in summaries {
            let graph = try await client.graph(ownerUserID: owner, graphID: summary.graphID)
            if requiredTaskID == nil
                || graph.tasks.contains(where: { $0.taskID == requiredTaskID }) {
                graphs.append(graph)
            }
        }
        return graphs
    }

    private func mapGraphs(
        _ graphs: [LocalAgentTaskGraph],
        messageID: String,
        lookup: MessageTaskLookup?
    ) -> MessageTaskGraphSnapshot {
        let tasks = graphs.flatMap(\.tasks)
        let dependencies = graphs.flatMap(\.dependencies)
        let prerequisiteIDs = Set(dependencies.map(\.taskID))
        let roots = tasks.map(\.taskID).filter { !prerequisiteIDs.contains($0) }
        let depths = Self.depths(tasks: tasks, dependencies: dependencies)
        let nodes = tasks.map { task in
            MessageTaskGraphNode(
                task: mapTaskWithoutRun(task),
                depth: depths[task.taskID] ?? 0,
                isRoot: roots.contains(task.taskID),
                isCurrentMessage: true
            )
        }
        let edges = dependencies.map { dependency in
            MessageTaskGraphEdge(
                id: "\(dependency.prerequisiteTaskID)->\(dependency.taskID)",
                sourceID: dependency.prerequisiteTaskID,
                targetID: dependency.taskID
            )
        }
        return MessageTaskGraphSnapshot(
            rootTaskIDs: roots,
            nodes: nodes,
            edges: edges,
            sourceSessionID: lookup?.sessionID,
            sourceTurnID: lookup?.turnID,
            sourceUserMessageID: lookup?.sourceUserMessageID ?? messageID
        )
    }

    private func mapTask(
        _ task: LocalAgentTaskRecord,
        graph: LocalAgentTaskGraph
    ) async throws -> MessageTask {
        var mapped = mapTaskWithoutRun(task, dependencies: graph.dependencies)
        let owner = try requireOwner()
        if let run = try await client.runs(ownerUserID: owner, taskID: task.taskID).first {
            mapped = mapped.merging(run: mapRun(run))
        }
        return mapped
    }

    private func mapTaskWithoutRun(
        _ task: LocalAgentTaskRecord,
        dependencies: [LocalAgentTaskDependency] = []
    ) -> MessageTask {
        MessageTask(
            id: task.taskID,
            title: task.title,
            description: Self.string("description", in: task.input),
            objective: Self.string("objective", in: task.input),
            status: task.status,
            defaultModelConfigID: task.modelConfigRef,
            lastRunID: task.activeRunID,
            sourceTurnID: task.sourceEntityType == "conversation_turn"
                ? task.sourceEntityID : nil,
            prerequisiteTaskIDs: dependencies.filter { $0.taskID == task.taskID }
                .map(\.prerequisiteTaskID),
            inputPayloadJSON: Self.json(task.input),
            createdAt: Self.date(task.createdAtUnixMs),
            updatedAt: Self.date(task.updatedAtUnixMs)
        )
    }

    private func mapRun(_ run: LocalAgentRunRecord) -> MessageTaskRun {
        MessageTaskRun(
            id: run.runID,
            taskID: run.ownerEntityID,
            status: run.status,
            modelPhaseStatus: run.status,
            startedAt: Self.date(run.createdAtUnixMs),
            finishedAt: Self.isTerminal(run.status) ? Self.date(run.updatedAtUnixMs) : nil,
            resultSummary: run.terminalOutcome.flatMap { Self.string("text", in: $0) },
            reportContent: run.terminalOutcome.flatMap { Self.string("report", in: $0) },
            errorMessage: run.terminalOutcome.flatMap { Self.string("error", in: $0) }
        )
    }

    private func mapEvent(_ event: LocalAgentEventRecord) -> MessageTaskRunEvent {
        MessageTaskRunEvent(
            id: event.eventID,
            eventType: event.eventType,
            message: event.payload.flatMap {
                Self.string("message", in: $0) ?? Self.string("reason", in: $0)
            },
            createdAt: Self.date(event.createdAtUnixMs)
        )
    }

    private func requireOwner() throws -> String {
        guard let ownerUserID else {
            throw NativeLocalAgentMessageTaskGraphServiceError.notConfigured
        }
        return ownerUserID
    }

    private static func depths(
        tasks: [LocalAgentTaskRecord],
        dependencies: [LocalAgentTaskDependency]
    ) -> [String: Int] {
        var result = Dictionary(uniqueKeysWithValues: tasks.map { ($0.taskID, 0) })
        for _ in tasks.indices {
            var changed = false
            for dependency in dependencies {
                let depth = (result[dependency.prerequisiteTaskID] ?? 0) + 1
                if depth > (result[dependency.taskID] ?? 0) {
                    result[dependency.taskID] = depth
                    changed = true
                }
            }
            if !changed { break }
        }
        return result
    }

    private static func string(_ key: String, in value: LocalAgentJSONValue) -> String? {
        guard case let .object(object) = value,
              case let .string(result)? = object[key] else { return nil }
        return result
    }

    private static func json(_ value: LocalAgentJSONValue) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private static func date(_ milliseconds: Int64) -> Date {
        Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
    }

    private static func isTerminal(_ status: String) -> Bool {
        status == "succeeded" || status == "failed" || status == "cancelled"
    }
}

public enum NativeLocalAgentMessageTaskGraphServiceError: LocalizedError {
    case notConfigured
    case taskNotFound

    public var errorDescription: String? {
        switch self {
        case .notConfigured: "Local Agent task service is not configured."
        case .taskNotFound: "The local task does not exist."
        }
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
