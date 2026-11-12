import ChatOSAgentRuntime
import CryptoKit
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
        var labels: [JSONValue] = [
            .string("memory-schema:v2"),
            .string(scope.subjectID),
            .string("thread-kind:\(scope.threadKind ?? "client_agent")"),
        ]
        if let projectSubjectID = scope.projectAggregationSubjectID {
            labels.append(.string(projectSubjectID))
        }
        var metadata: [String: JSONValue] = [
            "agent_id": .string(String(scope.subjectID.dropFirst("agent:".count))),
            "thread_kind": .string(scope.threadKind ?? "client_agent"),
            "run_id": .string(scope.runID.uuidString),
        ]
        if let agentName = scope.agentName {
            metadata["agent_name"] = .string(agentName)
        }
        if let projectSubjectID = scope.projectSubjectID {
            metadata["project_id"] = .string(String(projectSubjectID.dropFirst("project:".count)))
        }
        let body: [String: JSONValue] = [
            "tenant_id": .string(scope.tenantID), "source_id": .string(scope.sourceID),
            "subject_id": .string(scope.subjectID), "thread_type": .string("client_agent"),
            "external_thread_id": .string(scope.threadID),
            "labels": .array(labels),
            "metadata": .object(metadata),
        ]
        let result: MemoryThreadDTO = try await request(threadPath, method: "PUT", body: body)
        try validate(tenant: result.tenant_id, source: result.source_id, thread: result.id)
        guard result.subject_id == scope.subjectID else { throw AgentRuntimeError.scopeMismatch }
    }

    public func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        guard !entries.isEmpty, entries.count <= 32, Set(entries.map(\.id)).count == entries.count,
              entries.allSatisfy({ $0.index >= 0 && scope.acceptsRecord(id: $0.id, index: $0.index) }) else {
            throw AgentContextError.invalidHistory
        }
        let assertion = try await runtimeAssertion(operation: "record_ingest")
        let records = try entries.map {
            try record($0, projectContextAssertionID: assertion.runtime_identity_revision)
        }
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
        let body = MemorySyncRequest(
            tenant_id: scope.tenantID,
            source_id: scope.sourceID,
            project_partition_key: assertion.project_partition_key,
            runtime_context_assertion: assertion.runtime_context_assertion,
            records: records
        )
        let result: MemorySyncResponse = try await request(threadPath + "/records/batch-sync", method: "PUT", body: body)
        guard result.thread_id == scope.threadID, result.received_count == entries.count,
              result.upserted_count == entries.count else { throw ChatOSAPIError.invalidResponse }
    }

    public func compose(_ composeRequest: AgentMemoryComposeRequest) async throws -> AgentMemoryContext {
        let assertion = try await runtimeAssertion(operation: "compose")
        var body: [String: JSONValue] = [
            "tenant_id": .string(scope.tenantID), "source_id": .string(scope.sourceID), "thread_id": .string(scope.threadID),
            "subject_id": .string(scope.subjectID),
            "query_text": .string(composeRequest.queryText),
            "current_turn_id": .string(composeRequest.currentTurnID),
            "model_capability_revision": .string(composeRequest.modelCapability.revision),
            "runtime_context_assertion": .string(assertion.runtime_context_assertion),
            "policy": .object([
                "max_memory_tokens": .number(Double(composeRequest.availableMemoryTokens)),
                "max_recent_record_tokens": .number(Double(composeRequest.availableMemoryTokens)),
                "max_thread_summary_tokens": .number(Double(composeRequest.availableMemoryTokens)),
                "max_project_memory_tokens": .number(Double(composeRequest.availableMemoryTokens)),
                "max_agent_memory_tokens": .number(Double(composeRequest.availableMemoryTokens)),
            ]),
        ]
        if let projectSubjectID = scope.projectSubjectID {
            body["context_project_subject_id"] = .string(projectSubjectID)
        }
        let result: MemoryComposeDTO = try await request("/context/compose", method: "POST", body: body)
        guard result.thread_id == scope.threadID, result.meta.recent_record_count == result.recent_records.count else {
            throw AgentRuntimeError.scopeMismatch
        }
        for record in result.recent_records {
            try validate(tenant: record.tenant_id, source: record.source_id, thread: record.thread_id)
        }
        return try .init(
            blocks: result.blocks.map { .init(blockType: $0.block_type, text: $0.text) },
            recentRecords: result.recent_records.map(contextRecord),
            needsCompaction: result.meta.needs_compaction ?? false,
            budgetUsedTokens: result.meta.budget_used?.total,
            pendingRecordCount: result.meta.candidate_count_by_pool?["raw_tail"]
        )
    }

    public func compactSummary() async throws {
        var result = try await runSummary(triggerReason: "compose_budget_compaction")
        while result.running {
            try Task.checkCancellation()
            try await Task.sleep(for: .seconds(1))
            result = try await activeSummaryStatus(jobRunID: result.job_run_id)
        }
        guard result.completed, !result.failed, result.generated || result.compacted else {
            throw AgentContextError.budgetExceeded
        }
    }

    public func flushSummary() async throws {
        _ = try await runSummary(triggerReason: "agent_run_terminal")
    }

    private func runSummary(triggerReason: String) async throws -> MemorySummaryDTO {
        let assertion = try await runtimeAssertion(operation: "summarize")
        let body: [String: JSONValue] = [
            "tenant_id": .string(scope.tenantID),
            "source_id": .string(scope.sourceID),
            "project_partition_key": .string(assertion.project_partition_key),
            "runtime_context_assertion": .string(assertion.runtime_context_assertion),
            "trigger_reason": .string(triggerReason),
        ]
        let result: MemorySummaryDTO = try await request(
            threadPath + "/active-summary/run", method: "POST", body: body
        )
        guard result.thread_id == scope.threadID, result.accepted else {
            throw AgentRuntimeError.scopeMismatch
        }
        return result
    }

    private func activeSummaryStatus(jobRunID: String?) async throws -> MemorySummaryDTO {
        let assertion = try await runtimeAssertion(operation: "summarize")
        var body: [String: JSONValue] = [
            "tenant_id": .string(scope.tenantID),
            "source_id": .string(scope.sourceID),
            "project_partition_key": .string(assertion.project_partition_key),
            "runtime_context_assertion": .string(assertion.runtime_context_assertion),
        ]
        if let jobRunID { body["job_run_id"] = .string(jobRunID) }
        let result: MemorySummaryDTO = try await request(
            threadPath + "/active-summary/status", method: "POST", body: body
        )
        guard result.thread_id == scope.threadID,
              jobRunID == nil || result.job_run_id == jobRunID else {
            throw AgentRuntimeError.scopeMismatch
        }
        return result
    }

    private var threadPath: String { "/threads/" + encoded(scope.threadID) }
    private func runtimeAssertion(operation: String) async throws -> MemoryRuntimeAssertionResponse {
        let requestBody = MemoryRuntimeAssertionRequest(
            runtime_kind: "local_agent",
            project_id: scope.projectSubjectID.map { String($0.dropFirst("project:".count)) },
            runtime_id: scope.threadID,
            operation: operation,
            source_id: scope.sourceID,
            canonical_agent_subject_id: scope.subjectID,
            thread_id: scope.threadID
        )
        let assertion: MemoryRuntimeAssertionResponse = try await client.request(
            "/runtime-context-assertions",
            method: "POST",
            body: JSONEncoder().encode(requestBody),
            timeoutInterval: 30,
            service: .localConnector,
            expectedAuthenticationSessionID: sessionID
        )
        let expectedPartition = scope.projectSubjectID ?? "__none__"
        guard !assertion.runtime_context_assertion.isEmpty,
              assertion.source_id == scope.sourceID,
              assertion.canonical_agent_subject_id == scope.subjectID,
              assertion.thread_id == scope.threadID,
              assertion.project_partition_key == expectedPartition,
              !assertion.runtime_identity_revision.isEmpty else {
            throw AgentRuntimeError.scopeMismatch
        }
        return assertion
    }
    private func encoded(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .alphanumerics.union(CharacterSet(charactersIn: "-._~")))!
    }
    private func query(includeThread: Bool = false) -> String {
        var pairs = [("tenant_id", scope.tenantID), ("source_id", scope.sourceID)]
        if includeThread { pairs.append(("thread_id", scope.threadID)) }
        return "?" + pairs.map { encoded($0.0) + "=" + encoded($0.1) }.joined(separator: "&")
    }
    private func validate(tenant: String, source: String, thread: String) throws {
        guard tenant == scope.tenantID, source == scope.sourceID, thread == scope.threadID else { throw AgentRuntimeError.scopeMismatch }
    }
    private func contextRecord(_ value: MemoryRecordDTO) throws -> AgentMemoryContextRecord {
        guard let role = AgentMessage.Role(rawValue: value.role) else { throw ChatOSAPIError.invalidResponse }
        let calls = role == .assistant ? toolCalls(value.structured_payload, metadata: value.metadata) : []
        let toolCallID = role == .tool
            ? jsonString(value.metadata, keys: ["tool_call_id", "toolCallId", "tool_callId"])
                ?? jsonString(value.structured_payload, keys: ["tool_call_id", "toolCallId", "tool_callId"])
            : nil
        let responseOutputJSON = try jsonValue(value.metadata, keys: ["responses_output", "response_output"])
            .map { try JSONEncoder().encode($0) }
        let usage = providerUsage(value.metadata)
        return .init(id: value.id, message: .init(
            role: role, content: value.content, toolCalls: calls, toolCallID: toolCallID,
            responseOutputJSON: responseOutputJSON, usage: usage
        ))
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
    private func providerUsage(_ metadata: JSONValue?) -> AgentUsage? {
        guard case let .object(object)? = jsonValue(metadata, keys: ["provider_usage", "providerUsage"]) else {
            return nil
        }
        return .init(
            inputTokens: object["input_tokens"]?.intValue ?? 0,
            cachedTokens: object["cached_tokens"]?.intValue ?? 0,
            outputTokens: object["output_tokens"]?.intValue ?? 0,
            requests: object["requests"]?.intValue ?? 0
        )
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
    private func record(
        _ entry: AgentMemoryEntry,
        projectContextAssertionID: String
    ) throws -> MemoryRecordInput {
        if entry.message.role == .tool {
            guard let exchange = entry.toolExchange,
                  entry.message.toolCallID == exchange.callID else {
                throw AgentContextError.invalidHistory
            }
        } else if entry.toolExchange != nil {
            throw AgentContextError.invalidHistory
        }
        let calls = entry.message.toolCalls.map { call in
            JSONValue.object(["id": .string(call.id), "type": .string("function"),
                              "function": .object(["name": .string(call.name), "arguments": .string(call.arguments)])])
        }
        var payload: [String: JSONValue] = [:]
        if !calls.isEmpty { payload["tool_calls"] = .array(calls) }
        if let id = entry.message.toolCallID { payload["tool_call_id"] = .string(id) }
        var metadata: [String: JSONValue] = ["client_agent_run_id": .string(scope.runID.uuidString),
                                             "client_agent_message_index": .number(Double(entry.index)),
                                             "conversation_turn_id": .string(entry.conversationTurnID),
                                             "project_context_assertion_id": .string(projectContextAssertionID),
                                             "agent_id": .string(String(scope.subjectID.dropFirst("agent:".count))),
                                             "thread_kind": .string(scope.threadKind ?? "client_agent")]
        let identityEncoder = JSONEncoder()
        identityEncoder.outputFormatting = [.sortedKeys]
        let identity = try identityEncoder.encode(JSONValue.object([
            "id": .string(entry.id),
            "role": .string(entry.message.role.rawValue),
            "record_type": .string("message"),
            "content": .string(entry.message.content),
            "structured_payload": payload.isEmpty ? .null : .object(payload),
        ]))
        metadata["content_identity_digest"] = .string(
            "sha256:" + SHA256.hash(data: identity).map { String(format: "%02x", $0) }.joined()
        )
        if let agentName = scope.agentName {
            metadata["agent_name"] = .string(agentName)
        }
        if let projectSubjectID = scope.projectSubjectID {
            metadata["project_id"] = .string(
                String(projectSubjectID.dropFirst("project:".count))
            )
            metadata["memory_project_id"] = .string(
                String(projectSubjectID.dropFirst("project:".count))
            )
        }
        if let id = entry.message.toolCallID { metadata["tool_call_id"] = .string(id) }
        if let exchange = entry.toolExchange {
            metadata["assistant_record_id"] = .string(exchange.assistantRecordID)
            metadata["tool_exchange_id"] = .string(exchange.exchangeID)
            metadata["tool_call_id"] = .string(exchange.callID)
            metadata["tool_result_ordinal"] = .number(Double(exchange.resultOrdinal))
            metadata["tool_result_count"] = .number(Double(exchange.resultCount))
        }
        if let output = entry.message.responseOutputJSON {
            metadata["responses_output"] = try JSONDecoder().decode(JSONValue.self, from: output)
        }
        if let usage = entry.message.usage {
            metadata["provider_usage"] = .object([
                "input_tokens": .number(Double(usage.inputTokens)),
                "cached_tokens": .number(Double(usage.cachedTokens)),
                "output_tokens": .number(Double(usage.outputTokens)),
                "requests": .number(Double(usage.requests)),
            ])
        }
        let formatter = ISO8601DateFormatter(); formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return .init(id: entry.id, role: entry.message.role.rawValue, record_type: "message", content: entry.message.content,
                     structured_payload: payload.isEmpty ? nil : .object(payload), metadata: .object(metadata),
                     created_at: formatter.string(from: entry.createdAt))
    }
}
