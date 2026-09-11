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
            "subject_id": .string(scope.subjectID),
            "policy": .object(["include_thread_summary": .bool(true), "include_recent_records": .bool(true),
                               "include_subject_memory": .bool(true), "summary_limit": .number(2)]),
        ]
        let result: MemoryComposeDTO = try await request("/context/compose", method: "POST", body: body)
        guard result.thread_id == scope.threadID, result.meta.recent_record_count == result.recent_records.count else {
            throw AgentRuntimeError.scopeMismatch
        }
        for record in result.recent_records {
            try validate(tenant: record.tenant_id, source: record.source_id, thread: record.thread_id)
        }
        return try .init(
            blocks: result.blocks.map { .init(blockType: $0.block_type, text: $0.text) },
            recentRecords: result.recent_records.map(contextRecord)
        )
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
        return .init(jobID: value.job_run_id, accepted: value.accepted, running: value.running,
                     completed: value.completed, failed: value.failed, generated: value.generated,
                     compacted: value.compacted, errorMessage: value.error_message)
    }
    private func contextRecord(_ value: MemoryRecordDTO) throws -> AgentMemoryContextRecord {
        guard let role = AgentMessage.Role(rawValue: value.role) else { throw ChatOSAPIError.invalidResponse }
        let calls = role == .assistant ? toolCalls(value.structured_payload, metadata: value.metadata) : []
        let toolCallID = role == .tool
            ? jsonString(value.metadata, keys: ["tool_call_id", "toolCallId", "tool_callId"])
                ?? jsonString(value.structured_payload, keys: ["tool_call_id", "toolCallId", "tool_callId"])
            : nil
        return .init(id: value.id, message: .init(role: role, content: value.content,
                                                  toolCalls: calls, toolCallID: toolCallID))
    }
    private func toolCalls(_ payload: JSONValue?, metadata: JSONValue?) -> [AgentToolCall] {
        let raw = jsonValue(payload, keys: ["tool_calls", "toolCalls"])
            ?? jsonValue(metadata, keys: ["tool_calls", "toolCalls"])
        let values: [JSONValue]
        switch raw {
        case let .array(items): values = items
        case let .object(object): values = [.object(object)]
        case let .string(text):
            values = (try? JSONDecoder().decode(JSONValue.self, from: Data(text.utf8))).map {
                if case let .array(items) = $0 { return items }
                return [$0]
            } ?? []
        default: values = []
        }
        return values.compactMap { value in
            guard case let .object(object) = value,
                  let id = string(object["id"]) ?? string(object["call_id"]),
                  !id.isEmpty else { return nil }
            let function: [String: JSONValue]
            if case let .object(nested)? = object["function"] { function = nested } else { function = object }
            guard let name = string(function["name"]), !name.isEmpty else { return nil }
            let arguments = string(function["arguments"])
                ?? function["arguments"].flatMap(compactJSON)
                ?? "{}"
            return .init(id: id, name: name, arguments: arguments)
        }
    }
    private func jsonValue(_ value: JSONValue?, keys: [String]) -> JSONValue? {
        guard case let .object(object)? = value else { return nil }
        return keys.lazy.compactMap { object[$0] }.first
    }
    private func jsonString(_ value: JSONValue?, keys: [String]) -> String? {
        keys.lazy.compactMap { key in
            guard case let .object(object)? = value else { return nil }
            return string(object[key])
        }.first
    }
    private func string(_ value: JSONValue?) -> String? {
        guard case let .string(text)? = value else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
    private func compactJSON(_ value: JSONValue) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
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
