import ChatOSCore
import Foundation

public struct LocalAgentTaskRecord: Decodable, Sendable, Equatable {
    public let graphID: String
    public let ownerUserID: String
    public let sourceEntityType: String
    public let sourceEntityID: String
    public let taskID: String
    public let title: String
    public let modelConfigRef: String
    public let input: LocalAgentJSONValue
    public let status: String
    public let activeRunID: String?
    public let version: UInt64
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case graphID = "graph_id"
        case ownerUserID = "owner_user_id"
        case sourceEntityType = "source_entity_type"
        case sourceEntityID = "source_entity_id"
        case taskID = "task_id"
        case title
        case modelConfigRef = "model_config_ref"
        case input, status
        case activeRunID = "active_run_id"
        case version
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

public struct LocalAgentTaskDependency: Decodable, Sendable, Equatable {
    public let taskID: String
    public let prerequisiteTaskID: String

    private enum CodingKeys: String, CodingKey {
        case taskID = "task_id"
        case prerequisiteTaskID = "prerequisite_task_id"
    }
}

public struct LocalAgentTaskGraph: Decodable, Sendable, Equatable {
    public let graphID: String
    public let ownerUserID: String
    public let sourceEntityType: String
    public let sourceEntityID: String
    public let status: String
    public let tasks: [LocalAgentTaskRecord]
    public let dependencies: [LocalAgentTaskDependency]
    public let createdAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case graphID = "graph_id"
        case ownerUserID = "owner_user_id"
        case sourceEntityType = "source_entity_type"
        case sourceEntityID = "source_entity_id"
        case status, tasks, dependencies
        case createdAtUnixMs = "created_at_unix_ms"
    }
}

public struct LocalAgentTaskGraphSummary: Decodable, Sendable, Equatable {
    public let graphID: String
    public let sourceEntityType: String
    public let sourceEntityID: String
    public let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case graphID = "graph_id"
        case sourceEntityType = "source_entity_type"
        case sourceEntityID = "source_entity_id"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

public struct LocalAgentTaskGraphPage: Decodable, Sendable, Equatable {
    public let graphs: [LocalAgentTaskGraphSummary]
    public let nextBeforeUpdatedAtUnixMs: Int64?
    public let nextBeforeGraphID: String?

    private enum CodingKeys: String, CodingKey {
        case graphs
        case nextBeforeUpdatedAtUnixMs = "next_before_updated_at_unix_ms"
        case nextBeforeGraphID = "next_before_graph_id"
    }
}

public struct NativeLocalAgentTaskClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    public func listGraphs(
        ownerUserID: String,
        beforeUpdatedAtUnixMs: Int64? = nil,
        beforeGraphID: String? = nil,
        limit: UInt32 = 100
    ) async throws -> LocalAgentTaskGraphPage {
        let result: TaskGraphsResult = try await host.request(ListGraphsCommand(
            type: "list_task_graphs",
            ownerUserID: ownerUserID,
            scope: "all",
            beforeUpdatedAtUnixMs: beforeUpdatedAtUnixMs,
            beforeGraphID: beforeGraphID,
            limit: limit
        ))
        guard result.type == "task_graphs" else { throw NativeLocalAgentHostError.invalidResponse }
        return result.page
    }

    public func graph(ownerUserID: String, graphID: String) async throws -> LocalAgentTaskGraph {
        let result: TaskGraphResult = try await host.request(GetGraphCommand(
            type: "get_task_graph",
            ownerUserID: ownerUserID,
            graphID: graphID
        ))
        guard result.type == "task_graph" else { throw NativeLocalAgentHostError.invalidResponse }
        return result.graph
    }

    public func runs(
        ownerUserID: String,
        taskID: String,
        limit: UInt32 = 100
    ) async throws -> [LocalAgentRunRecord] {
        let result: TaskRunsResult = try await host.request(GetTaskRunsCommand(
            type: "get_task_runs",
            ownerUserID: ownerUserID,
            taskID: taskID,
            limit: limit
        ))
        guard result.type == "task_runs", result.taskID == taskID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.runs
    }

    public func run(ownerUserID: String, runID: String) async throws -> LocalAgentRunRecord {
        let result: RunResult = try await host.request(GetRunCommand(
            type: "get_run",
            ownerUserID: ownerUserID,
            runID: runID
        ))
        guard result.type == "run" else { throw NativeLocalAgentHostError.invalidResponse }
        return result.run
    }

    public func events(
        ownerUserID: String,
        runID: String,
        afterCursor: Int64 = 0,
        limit: UInt32 = 100
    ) async throws -> LocalAgentEventPage {
        let result: EventsResult = try await host.request(ListEventsCommand(
            type: "list_events",
            ownerUserID: ownerUserID,
            afterCursor: afterCursor,
            limit: limit,
            runID: runID
        ))
        guard result.type == "events" else { throw NativeLocalAgentHostError.invalidResponse }
        return .init(events: result.events, nextCursor: result.nextCursor)
    }

    public func cancel(
        ownerUserID: String,
        taskID: String,
        expectedVersion: UInt64,
        reason: String
    ) async throws -> LocalAgentTaskGraph {
        try await mutate(CancelTaskCommand(
            type: "cancel_task",
            ownerUserID: ownerUserID,
            taskID: taskID,
            expectedVersion: expectedVersion,
            reason: reason
        ))
    }

    public func retry(
        ownerUserID: String,
        taskID: String,
        expectedVersion: UInt64,
        retryInstruction: String?
    ) async throws -> LocalAgentTaskGraph {
        try await mutate(RetryTaskCommand(
            type: "retry_task",
            ownerUserID: ownerUserID,
            taskID: taskID,
            expectedVersion: expectedVersion,
            retryInstruction: retryInstruction
        ))
    }

    private func mutate<Command: Encodable & Sendable>(
        _ command: Command
    ) async throws -> LocalAgentTaskGraph {
        let result: TaskGraphResult = try await host.request(command)
        guard result.type == "task_graph" else { throw NativeLocalAgentHostError.invalidResponse }
        return result.graph
    }
}

private struct ListGraphsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let scope: String
    let beforeUpdatedAtUnixMs: Int64?
    let beforeGraphID: String?
    let limit: UInt32

    private enum CodingKeys: String, CodingKey {
        case type, scope, limit
        case ownerUserID = "owner_user_id"
        case beforeUpdatedAtUnixMs = "before_updated_at_unix_ms"
        case beforeGraphID = "before_graph_id"
    }
}

private struct GetGraphCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let graphID: String
    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case graphID = "graph_id"
    }
}

private struct GetTaskRunsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let taskID: String
    let limit: UInt32
    private enum CodingKeys: String, CodingKey {
        case type, limit
        case ownerUserID = "owner_user_id"
        case taskID = "task_id"
    }
}

private struct GetRunCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let runID: String
    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case runID = "run_id"
    }
}

private struct ListEventsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let afterCursor: Int64
    let limit: UInt32
    let runID: String
    private enum CodingKeys: String, CodingKey {
        case type, limit
        case ownerUserID = "owner_user_id"
        case afterCursor = "after_cursor"
        case runID = "run_id"
    }
}

private struct CancelTaskCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let taskID: String
    let expectedVersion: UInt64
    let reason: String
    private enum CodingKeys: String, CodingKey {
        case type, reason
        case ownerUserID = "owner_user_id"
        case taskID = "task_id"
        case expectedVersion = "expected_version"
    }
}

private struct RetryTaskCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let taskID: String
    let expectedVersion: UInt64
    let retryInstruction: String?
    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case taskID = "task_id"
        case expectedVersion = "expected_version"
        case retryInstruction = "retry_instruction"
    }
}

private struct TaskGraphResult: Decodable, Sendable {
    let type: String
    let graph: LocalAgentTaskGraph
}

private struct TaskGraphsResult: Decodable, Sendable {
    let type: String
    let page: LocalAgentTaskGraphPage
}

private struct TaskRunsResult: Decodable, Sendable {
    let type: String
    let taskID: String
    let runs: [LocalAgentRunRecord]
    private enum CodingKeys: String, CodingKey {
        case type, runs
        case taskID = "task_id"
    }
}

private struct RunResult: Decodable, Sendable {
    let type: String
    let run: LocalAgentRunRecord
}

private struct EventsResult: Decodable, Sendable {
    let type: String
    let events: [LocalAgentEventRecord]
    let nextCursor: Int64
    private enum CodingKeys: String, CodingKey {
        case type, events
        case nextCursor = "next_cursor"
    }
}
