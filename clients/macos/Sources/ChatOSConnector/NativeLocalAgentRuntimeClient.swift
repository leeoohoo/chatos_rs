import ChatOSCore
import Foundation

public struct LocalAgentRunRecord: Decodable, Sendable, Equatable {
    public let runID: String
    public let ownerUserID: String
    public let ownerEntityType: String
    public let ownerEntityID: String
    public let profileKey: String
    public let input: LocalAgentJSONValue
    public let status: String
    public let version: UInt64
    public let terminalOutcome: LocalAgentJSONValue?
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case runID = "run_id"
        case ownerUserID = "owner_user_id"
        case ownerEntityType = "owner_entity_type"
        case ownerEntityID = "owner_entity_id"
        case profileKey = "profile_key"
        case input, status, version
        case terminalOutcome = "terminal_outcome"
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

public struct LocalAgentRunPage: Decodable, Sendable, Equatable {
    public let runs: [LocalAgentRunRecord]
    public let nextBeforeUpdatedAtUnixMs: Int64?
    public let nextBeforeRunID: String?

    private enum CodingKeys: String, CodingKey {
        case runs
        case nextBeforeUpdatedAtUnixMs = "next_before_updated_at_unix_ms"
        case nextBeforeRunID = "next_before_run_id"
    }
}

public struct LocalAgentEventRecord: Decodable, Sendable, Equatable {
    public let cursor: Int64
    public let eventID: String
    public let runID: String
    public let eventType: String
    public let payload: LocalAgentJSONValue?
    public let createdAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case cursor
        case eventID = "event_id"
        case runID = "run_id"
        case eventType = "event_type"
        case payload
        case createdAtUnixMs = "created_at_unix_ms"
    }
}

public struct LocalAgentEventPage: Sendable, Equatable {
    public let events: [LocalAgentEventRecord]
    public let nextCursor: Int64
}

public struct NativeLocalAgentRuntimeClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    public func listRuns(
        ownerUserID: String,
        scope: String,
        limit: UInt32 = 100
    ) async throws -> LocalAgentRunPage {
        let result: RunsResult = try await host.request(ListRunsCommand(
            type: "list_runs",
            ownerUserID: ownerUserID,
            scope: scope,
            limit: limit
        ))
        guard result.type == "runs" else { throw NativeLocalAgentHostError.invalidResponse }
        return result.page
    }

    public func waitEvents(
        ownerUserID: String,
        afterCursor: Int64,
        limit: UInt32 = 100,
        timeoutMilliseconds: UInt64 = 20_000
    ) async throws -> LocalAgentEventPage {
        let result: EventsResult = try await host.request(WaitEventsCommand(
            type: "wait_events",
            ownerUserID: ownerUserID,
            afterCursor: afterCursor,
            limit: limit,
            timeoutMilliseconds: timeoutMilliseconds
        ))
        guard result.type == "events" else { throw NativeLocalAgentHostError.invalidResponse }
        return .init(events: result.events, nextCursor: result.nextCursor)
    }

    public func resumeWaitingRun(
        ownerUserID: String,
        runID: String,
        expectedVersion: UInt64,
        input: LocalAgentJSONValue,
        reason: String
    ) async throws -> LocalAgentRunRecord {
        let result: RunResult = try await host.request(ResumeRunCommand(
            type: "resume_run",
            ownerUserID: ownerUserID,
            runID: runID,
            expectedVersion: expectedVersion,
            expectedStatus: "waiting_user",
            reason: reason,
            input: input
        ))
        guard result.type == "run" else { throw NativeLocalAgentHostError.invalidResponse }
        return result.run
    }
}

private struct ListRunsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let scope: String
    let limit: UInt32

    private enum CodingKeys: String, CodingKey {
        case type, scope, limit
        case ownerUserID = "owner_user_id"
    }
}

private struct WaitEventsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let afterCursor: Int64
    let limit: UInt32
    let timeoutMilliseconds: UInt64

    private enum CodingKeys: String, CodingKey {
        case type, limit
        case ownerUserID = "owner_user_id"
        case afterCursor = "after_cursor"
        case timeoutMilliseconds = "timeout_ms"
    }
}

private struct ResumeRunCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let runID: String
    let expectedVersion: UInt64
    let expectedStatus: String
    let reason: String
    let input: LocalAgentJSONValue

    private enum CodingKeys: String, CodingKey {
        case type, reason, input
        case ownerUserID = "owner_user_id"
        case runID = "run_id"
        case expectedVersion = "expected_version"
        case expectedStatus = "expected_status"
    }
}

private struct RunsResult: Decodable, Sendable {
    let type: String
    let page: LocalAgentRunPage
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
