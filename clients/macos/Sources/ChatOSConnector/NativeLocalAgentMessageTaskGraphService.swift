import ChatOSCore
import Foundation

public actor NativeLocalAgentMessageTaskGraphService: MessageTaskGraphServicing {
    private static let taskProcessRecordTool = "task_run_process_record_process"
    private static let eventPageLimit: UInt32 = 500
    private static let maximumProcessEventPages = 10

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
        if let graph = try await messageGraph(lookup: lookup) {
            return mapMessageGraph(graph, messageID: messageID, lookup: lookup)
        }
        let graphs = try await matchingGraphs(lookup: lookup)
        return mapGraphs(graphs, messageID: messageID, lookup: lookup)
    }

    public func fetchTask(
        messageID: String,
        taskID: String,
        lookup: MessageTaskLookup?
    ) async throws -> MessageTask {
        if let messageGraph = try await messageGraph(lookup: lookup),
           let node = messageGraph.nodes.first(where: { $0.task.taskID == taskID }) {
            let dependencies = messageGraph.edges.compactMap { edge in
                edge.kind == "prerequisite"
                    ? LocalAgentTaskDependency(
                        taskID: edge.targetTaskID,
                        prerequisiteTaskID: edge.sourceTaskID
                    )
                    : nil
            }
            return try await mapTask(node.task, dependencies: dependencies)
        }
        let graphs = try await matchingGraphs(lookup: lookup, requiredTaskID: taskID)
        guard let graph = graphs.first(where: { graph in
            graph.tasks.contains(where: { $0.taskID == taskID })
        }), let task = graph.tasks.first(where: { $0.taskID == taskID }) else {
            throw NativeLocalAgentMessageTaskGraphServiceError.taskNotFound
        }
        return try await mapTask(task, dependencies: graph.dependencies)
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
        let current: LocalAgentTaskRecord?
        if let graph = try await messageGraph(lookup: lookup) {
            current = graph.nodes.lazy.map(\.task).first(where: {
                $0.taskID == run.ownerEntityID
            })
        } else {
            let graphs = try await matchingGraphs(
                lookup: lookup,
                requiredTaskID: run.ownerEntityID
            )
            current = graphs.lazy.flatMap(\.tasks).first(where: {
                $0.taskID == run.ownerEntityID
            })
        }
        guard let current else {
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
        let task: LocalAgentTaskRecord?
        if let graph = try await messageGraph(lookup: lookup) {
            task = graph.nodes.lazy.map(\.task).first(where: { $0.taskID == taskID })
        } else {
            let graphs = try await matchingGraphs(lookup: lookup, requiredTaskID: taskID)
            task = graphs.lazy.flatMap(\.tasks).first(where: { $0.taskID == taskID })
        }
        guard let task else {
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

    private func messageGraph(
        lookup: MessageTaskLookup?
    ) async throws -> LocalAgentMessageTaskGraph? {
        guard let conversationID = lookup?.sessionID?.nilIfEmpty,
              let turnID = lookup?.turnID?.nilIfEmpty else { return nil }
        return try await client.messageGraph(
            ownerUserID: requireOwner(),
            sourceConversationID: conversationID,
            sourceTurnID: turnID,
            sourceUserMessageID: lookup?.sourceUserMessageID
        )
    }

    private func mapMessageGraph(
        _ graph: LocalAgentMessageTaskGraph,
        messageID: String,
        lookup: MessageTaskLookup?
    ) -> MessageTaskGraphSnapshot {
        let prerequisiteIDsByTask = Dictionary(grouping: graph.edges.filter {
            $0.kind == "prerequisite"
        }, by: \.targetTaskID)
        let nodes = graph.nodes.map { node in
            MessageTaskGraphNode(
                task: mapTaskWithoutRun(
                    node.task,
                    dependencies: (prerequisiteIDsByTask[node.task.taskID] ?? []).map {
                        LocalAgentTaskDependency(
                            taskID: $0.targetTaskID,
                            prerequisiteTaskID: $0.sourceTaskID
                        )
                    }
                ),
                depth: Int(node.depth),
                isRoot: node.isRoot,
                isCurrentMessage: node.isCurrentMessage
            )
        }
        return MessageTaskGraphSnapshot(
            rootTaskIDs: graph.rootTaskIDs,
            nodes: nodes,
            edges: graph.edges.map { edge in
                MessageTaskGraphEdge(
                    id: "\(edge.sourceTaskID)->\(edge.targetTaskID)",
                    sourceID: edge.sourceTaskID,
                    targetID: edge.targetTaskID,
                    kind: edge.kind
                )
            },
            sourceSessionID: graph.sourceConversationID,
            sourceTurnID: graph.sourceTurnID,
            sourceUserMessageID: graph.sourceUserMessageID
                ?? lookup?.sourceUserMessageID
                ?? messageID
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
        let mappedTasks = tasks.map { mapTaskWithoutRun($0) }
        let nodes = mappedTasks.map { task in
            MessageTaskGraphNode(
                task: task,
                depth: depths[task.id] ?? 0,
                isRoot: roots.contains(task.id),
                isCurrentMessage: true
            )
        }
        var edges = dependencies.map { dependency in
            MessageTaskGraphEdge(
                id: "\(dependency.prerequisiteTaskID)->\(dependency.taskID)",
                sourceID: dependency.prerequisiteTaskID,
                targetID: dependency.taskID
            )
        }
        var edgeIDs = Set(edges.map(\.id))
        for graph in graphs {
            let graphTasks = graph.tasks.map { mapTaskWithoutRun($0) }
            var taskIDByClientRef: [String: String] = [:]
            for task in graphTasks {
                if let clientRef = task.executionClientRef {
                    taskIDByClientRef[clientRef] = task.id
                }
            }
            for task in graphTasks {
                for contextRef in task.dependencyContextRefs {
                    guard let sourceID = taskIDByClientRef[contextRef], sourceID != task.id else {
                        continue
                    }
                    let edgeID = "\(sourceID)->\(task.id)"
                    guard edgeIDs.insert(edgeID).inserted else { continue }
                    edges.append(MessageTaskGraphEdge(
                        id: edgeID,
                        sourceID: sourceID,
                        targetID: task.id,
                        kind: "context"
                    ))
                }
            }
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
        dependencies: [LocalAgentTaskDependency]
    ) async throws -> MessageTask {
        var mapped = mapTaskWithoutRun(task, dependencies: dependencies)
        let owner = try requireOwner()
        if let run = try await client.runs(ownerUserID: owner, taskID: task.taskID).first {
            mapped = mapped.merging(run: mapRun(run))
            mapped.processLog = try? await processLog(ownerUserID: owner, runID: run.runID)
        }
        return mapped
    }

    private func mapTaskWithoutRun(
        _ task: LocalAgentTaskRecord,
        dependencies: [LocalAgentTaskDependency] = []
    ) -> MessageTask {
        let inputPayload = Self.value("input_payload", in: task.input)
        return MessageTask(
            id: task.taskID,
            title: task.title,
            description: Self.string("description", in: task.input),
            objective: Self.string("objective", in: task.input),
            status: task.status == "ready" && task.activeRunID != nil ? "queued" : task.status,
            defaultModelConfigID: task.modelConfigRef,
            lastRunID: task.activeRunID,
            sourceTurnID: task.sourceEntityType == "conversation_turn"
                ? task.sourceEntityID : nil,
            prerequisiteTaskIDs: dependencies.filter { $0.taskID == task.taskID }
                .map(\.prerequisiteTaskID),
            executionClientRef: inputPayload.flatMap {
                Self.string("execution_client_ref", in: $0)
            } ?? Self.string("client_ref", in: task.input),
            dependencyContextRefs: inputPayload.map {
                Self.strings("dependency_context_refs", in: $0)
            } ?? [],
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
            resultSummary: run.terminalOutcome.flatMap {
                Self.string("content", in: $0)
                    ?? Self.string("answer", in: $0)
                    ?? Self.string("text", in: $0)
            },
            reportContent: run.terminalOutcome.flatMap { Self.string("report", in: $0) },
            errorMessage: run.terminalOutcome.flatMap { Self.string("error", in: $0) }
        )
    }

    private func processLog(ownerUserID: String, runID: String) async throws -> String? {
        var events: [LocalAgentEventRecord] = []
        var cursor: Int64 = 0
        for _ in 0..<Self.maximumProcessEventPages {
            let page = try await client.events(
                ownerUserID: ownerUserID,
                runID: runID,
                afterCursor: cursor,
                limit: Self.eventPageLimit,
                eventType: "tool_batch_completed"
            )
            events.append(contentsOf: page.events)
            guard page.events.count == Int(Self.eventPageLimit),
                  page.nextCursor > cursor else { break }
            cursor = page.nextCursor
        }
        return Self.processLog(from: events)
    }

    static func processLog(from events: [LocalAgentEventRecord]) -> String? {
        let formatter = ISO8601DateFormatter()
        var log: String?
        for event in events.sorted(by: { $0.cursor < $1.cursor }) {
            guard event.eventType == "tool_batch_completed",
                  case let .object(payload)? = event.payload,
                  case let .array(invocations)? = payload["invocations"] else { continue }
            for invocation in invocations {
                guard case let .object(record) = invocation,
                      case let .string(toolName)? = record["tool_name"],
                      toolName == taskProcessRecordTool,
                      case let .string(status)? = record["status"],
                      status == "succeeded",
                      case let .object(arguments)? = record["arguments"] else { continue }
                let argumentValue = LocalAgentJSONValue.object(arguments)
                let operation = string("operation", in: argumentValue) ?? "append"
                let heading = string("heading", in: argumentValue)
                let content = string("content", in: argumentValue)?
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                switch operation {
                case "clear":
                    log = nil
                case "replace":
                    log = content?.nilIfEmpty
                case "append":
                    guard let content = content?.nilIfEmpty else { continue }
                    let timestamp = formatter.string(from: date(event.createdAtUnixMs))
                    let title = heading?.trimmingCharacters(in: .whitespacesAndNewlines)
                        .nilIfEmpty
                    let entry = title.map { "[\(timestamp)] \($0)\n\(content)" }
                        ?? "[\(timestamp)]\n\(content)"
                    log = log.map { "\($0)\n\n\(entry)" } ?? entry
                default:
                    continue
                }
            }
        }
        return log
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

    private static func value(_ key: String, in value: LocalAgentJSONValue) -> LocalAgentJSONValue? {
        guard case let .object(object) = value else { return nil }
        return object[key]
    }

    private static func strings(_ key: String, in value: LocalAgentJSONValue) -> [String] {
        guard case let .object(object) = value,
              case let .array(values)? = object[key] else { return [] }
        return values.compactMap { value in
            guard case let .string(result) = value else { return nil }
            return result
        }
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
