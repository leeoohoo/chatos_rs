import ChatOSAgentRuntime
import Foundation

/// Uses the existing authenticated gateway, not a model provider URL or internal service secret.
public struct ChatOSMemoryEngineService: AgentMemoryServicing {
    private let client: ChatOSAPIClient
    private let scope: AgentMemoryScope
    private let sessionID: UUID

    public init(client: ChatOSAPIClient, scope: AgentMemoryScope) async throws {
        self.client = client; self.scope = scope
        self.sessionID = try await client.currentAuthenticationSessionID()
    }

    public func ensureThread() async throws {
        let body: [String: JSONValue] = [
            "tenant_id": .string(scope.tenantID), "source_id": .string(scope.sourceID),
            "subject_id": .string(scope.subjectID), "thread_type": .string("client_agent"),
            "external_thread_id": .string(scope.runID.uuidString),
            "labels": .array([.string("client_agent"), .string("memory_mapping:client_agent.v1")]),
        ]
        let result: MemoryThreadDTO = try await request(threadPath, method: "PUT", body: body)
        try validate(tenant: result.tenant_id, source: result.source_id, thread: result.id)
        guard result.subject_id == scope.subjectID else { throw AgentRuntimeError.scopeMismatch }
    }

    public func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        guard !entries.isEmpty, entries.count <= 32, Set(entries.map(\.id)).count == entries.count,
              entries.allSatisfy({ $0.index >= 0 && $0.id == scope.recordID(at: $0.index) }) else { throw AgentContextError.invalidHistory }
        let records = entries.map(record)
        if reconciling {
            // Existing batch-sync overwrites summary_status, even for identical IDs. Never blindly
            // resend an unacknowledged batch: it could turn summarized records back into pending.
            for expected in records {
                let result: MemoryRecordEnvelope = try await request("/records/\(encoded(expected.id))" + query(includeThread: true))
                guard let stored = result.item else { throw AgentContextError.syncUncertain }
                try validate(tenant: stored.tenant_id, source: stored.source_id, thread: stored.thread_id)
                guard stored.id == expected.id, stored.role == expected.role, stored.record_type == expected.record_type,
                      stored.content == expected.content, stored.structured_payload == expected.structured_payload,
                      stored.metadata == expected.metadata, stored.created_at == expected.created_at else {
                    throw AgentContextError.invalidHistory
                }
            }
            return
        }
        let body = MemorySyncRequest(tenant_id: scope.tenantID, source_id: scope.sourceID, records: records)
        let result: MemorySyncResponse = try await request(threadPath + "/records/batch-sync", method: "PUT", body: body)
        guard result.thread_id == scope.threadID, result.received_count == entries.count,
              result.upserted_count == entries.count else { throw ChatOSAPIError.invalidResponse }
    }

    public func compose() async throws -> AgentMemoryContext {
        let body: [String: JSONValue] = [
            "tenant_id": .string(scope.tenantID), "source_id": .string(scope.sourceID), "thread_id": .string(scope.threadID),
            "policy": .object(["include_thread_summary": .bool(true), "include_recent_records": .bool(true),
                               "include_subject_memory": .bool(false), "summary_limit": .number(2)]),
        ]
        let result: MemoryComposeDTO = try await request("/context/compose", method: "POST", body: body)
        guard result.thread_id == scope.threadID, result.meta.recent_record_count == result.recent_records.count else {
            throw AgentRuntimeError.scopeMismatch
        }
        for record in result.recent_records {
            try validate(tenant: record.tenant_id, source: record.source_id, thread: record.thread_id)
        }
        return .init(summaries: result.blocks.map(\.text), recentRecordIDs: result.recent_records.map(\.id))
    }

    public func startSummary(reason: String) async throws -> AgentSummaryStatus {
        guard ["active_context_budget", "context_overflow"].contains(reason) else { throw ChatOSAPIError.invalidEndpoint }
        let body: [String: JSONValue] = ["tenant_id": .string(scope.tenantID), "source_id": .string(scope.sourceID),
                                         "trigger_reason": .string(reason)]
        let result: MemorySummaryDTO = try await request(threadPath + "/active-summary/run", method: "POST", body: body)
        return try status(result)
    }

    public func summaryStatus(jobID: String?) async throws -> AgentSummaryStatus {
        let result: MemorySummaryDTO = try await request(threadPath + "/active-summary/status" + query(jobID: jobID))
        if let jobID, result.job_run_id != jobID { throw AgentRuntimeError.scopeMismatch }
        return try status(result)
    }

    private var threadPath: String { "/threads/" + encoded(scope.threadID) }
    private func encoded(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .alphanumerics.union(CharacterSet(charactersIn: "-._~")))!
    }
    private func query(includeThread: Bool = false, jobID: String? = nil) -> String {
        var pairs = [("tenant_id", scope.tenantID), ("source_id", scope.sourceID)]
        if includeThread { pairs.append(("thread_id", scope.threadID)) }
        if let jobID { pairs.append(("job_run_id", jobID)) }
        return "?" + pairs.map { encoded($0.0) + "=" + encoded($0.1) }.joined(separator: "&")
    }
    private func validate(tenant: String, source: String, thread: String) throws {
        guard tenant == scope.tenantID, source == scope.sourceID, thread == scope.threadID else { throw AgentRuntimeError.scopeMismatch }
    }
    private func status(_ value: MemorySummaryDTO) throws -> AgentSummaryStatus {
        guard value.thread_id == scope.threadID, !(value.running && value.completed) else { throw AgentRuntimeError.scopeMismatch }
        return .init(jobID: value.job_run_id, running: value.running, completed: value.completed,
                     failed: value.failed, compacted: value.compacted)
    }
    private func request<T: Decodable & Sendable>(_ path: String) async throws -> T {
        try await client.request(path, timeoutInterval: 30, service: .memoryEngine, expectedAuthenticationSessionID: sessionID)
    }
    private func request<T: Decodable & Sendable, B: Encodable & Sendable>(_ path: String, method: String, body: B) async throws -> T {
        try await client.request(path, method: method, body: JSONEncoder().encode(body), timeoutInterval: 30,
                                 service: .memoryEngine, expectedAuthenticationSessionID: sessionID)
    }
    private func record(_ entry: AgentMemoryEntry) -> MemoryRecordInput {
        let calls = entry.message.toolCalls.map { call in
            JSONValue.object(["id": .string(call.id), "type": .string("function"),
                              "function": .object(["name": .string(call.name), "arguments": .string(call.arguments)])])
        }
        var payload: [String: JSONValue] = [:]
        if !calls.isEmpty { payload["tool_calls"] = .array(calls) }
        if let id = entry.message.toolCallID { payload["tool_call_id"] = .string(id) }
        var metadata: [String: JSONValue] = ["client_agent_run_id": .string(scope.runID.uuidString),
                                             "client_agent_message_index": .number(Double(entry.index))]
        if let id = entry.message.toolCallID { metadata["tool_call_id"] = .string(id) }
        let formatter = ISO8601DateFormatter(); formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return .init(id: entry.id, role: entry.message.role.rawValue, record_type: "message", content: entry.message.content,
                     structured_payload: payload.isEmpty ? nil : .object(payload), metadata: .object(metadata),
                     created_at: formatter.string(from: entry.createdAt))
    }
}
