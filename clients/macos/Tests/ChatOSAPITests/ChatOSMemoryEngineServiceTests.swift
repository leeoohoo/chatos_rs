import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSMemoryEngineServiceTests: XCTestCase {
    func testUsesPublicSiblingGatewayAndUserBearerWithStableRecordIDs() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        try await service.ensureThread()
        let entries = try records(scope)
        try await service.sync(entries, reconciling: false)
        let composed = try await service.compose(try composeRequest())
        XCTAssertEqual(composed.recentRecords.map(\.id), entries.map(\.id))
        XCTAssertEqual(composed.recentRecords.map(\.message), entries.map(\.message))
        let calls = await transport.requests
        XCTAssertEqual(calls.count, 5)
        XCTAssertEqual(calls.map(\.url.path), [
            "/prefix/api/memory/threads/\(scope.threadID)",
            "/prefix/api/local/runtime-context-assertions",
            "/prefix/api/memory/threads/\(scope.threadID)/records/batch-sync",
            "/prefix/api/local/runtime-context-assertions",
            "/prefix/api/memory/context/compose",
        ])
        XCTAssertEqual(calls.map(\.method), ["PUT", "POST", "PUT", "POST", "POST"])
        XCTAssertTrue(calls.allSatisfy { $0.headers["Authorization"] == "Bearer user-token" })
        XCTAssertFalse(calls.contains { $0.url.path.contains("/api/chatos/") || $0.url.path.contains("/sdk/") || $0.url.path.contains("/api/memory-engine/") })
        XCTAssertFalse(calls.contains { $0.headers.keys.contains { $0.lowercased().hasPrefix("x-memory-") } })
        let threadBody = try object(calls[0].body)
        XCTAssertEqual(threadBody["external_thread_id"] as? String, scope.threadID)
        XCTAssertNotEqual(threadBody["external_thread_id"] as? String, scope.runID.uuidString)
        let labels = try XCTUnwrap(threadBody["labels"] as? [String])
        XCTAssertTrue(labels.contains("memory-schema:v2"))
        XCTAssertTrue(labels.contains(scope.subjectID))
        XCTAssertFalse(labels.contains(try XCTUnwrap(scope.projectSubjectID)))
        let threadMetadata = try XCTUnwrap(threadBody["metadata"] as? [String: Any])
        XCTAssertEqual(threadMetadata["agent_name"] as? String, scope.agentName)
        let body = try object(calls[2].body)
        XCTAssertEqual(body["source_id"] as? String, "chatos")
        XCTAssertEqual(body["tenant_id"] as? String, scope.tenantID)
        let records = try XCTUnwrap(body["records"] as? [[String: Any]])
        XCTAssertEqual(records.map { $0["id"] as? String }, entries.map(\.id))
        let firstPayload = try XCTUnwrap(records[0]["structured_payload"] as? [String: Any])
        XCTAssertEqual((firstPayload["tool_calls"] as? [[String: Any]])?.first?["id"] as? String, "call-a")
        XCTAssertEqual((records[1]["structured_payload"] as? [String: Any])?["tool_call_id"] as? String, "call-a")
        let firstMetadata = try XCTUnwrap(records[0]["metadata"] as? [String: Any])
        XCTAssertEqual(
            firstMetadata["memory_project_id"] as? String,
            try XCTUnwrap(scope.projectSubjectID).replacingOccurrences(of: "project:", with: "")
        )
        XCTAssertNil(firstMetadata["conversation_id"])
        XCTAssertEqual(firstMetadata["thread_kind"] as? String, scope.threadKind)
        XCTAssertEqual(firstMetadata["agent_name"] as? String, scope.agentName)
        XCTAssertEqual((firstMetadata["responses_output"] as? [[String: Any]])?.first?["type"] as? String, "compaction")
        XCTAssertEqual((firstMetadata["provider_usage"] as? [String: Any])?["cached_tokens"] as? Int, 7)
        let toolMetadata = try XCTUnwrap(records[1]["metadata"] as? [String: Any])
        XCTAssertEqual(toolMetadata["assistant_record_id"] as? String, scope.recordID(at: 0))
        XCTAssertEqual(toolMetadata["tool_exchange_id"] as? String, "agent:\(scope.runID.uuidString):tool:0")
        XCTAssertEqual(toolMetadata["tool_call_id"] as? String, "call-a")
        XCTAssertEqual(toolMetadata["tool_result_ordinal"] as? Int, 0)
        XCTAssertEqual(toolMetadata["tool_result_count"] as? Int, 1)
        XCTAssertNil(records[0]["summary_status"])
        let policy = try XCTUnwrap(try object(calls[4].body)["policy"] as? [String: Any])
        XCTAssertEqual(policy["max_memory_tokens"] as? Int, 24_000)
        XCTAssertEqual(try object(calls[4].body)["query_text"] as? String, "current query")
        XCTAssertEqual(try object(calls[4].body)["current_turn_id"] as? String, "turn-current")
        XCTAssertEqual(
            try object(calls[4].body)["model_capability_revision"] as? String,
            "test-capability-v1"
        )
        XCTAssertEqual(try object(calls[4].body)["subject_id"] as? String, scope.subjectID)
        XCTAssertEqual(
            try object(calls[4].body)["context_project_subject_id"] as? String,
            try XCTUnwrap(scope.projectSubjectID)
        )
        XCTAssertEqual(policy["max_recent_record_tokens"] as? Int, 12_000)
        XCTAssertFalse(calls.contains { String(decoding: $0.body ?? Data(), as: UTF8.self).contains("user-token") })
    }

    func testTerminalFlushStartsActiveSummary() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)

        try await service.flushSummary()

        let requests = await transport.requests
        XCTAssertEqual(requests.count, 2)
        XCTAssertTrue(requests[1].url.path.hasSuffix("/active-summary/run"))
        XCTAssertEqual(try object(requests[1].body)["trigger_reason"] as? String, "agent_run_terminal")
    }

    func testTaskThreadRegistersBothAgentAndProjectAggregationScopes() async throws {
        let scope = try AgentMemoryScope(
            tenantID: "user/a?&b", todoID: "todo-1", agentID: "agent-1", projectID: "project-1",
            runID: UUID(), runtimeScope: "task:todo-1"
        )
        let transport = MemoryTransport(
            scope: scope, refreshToken: false, foreignTenant: false, statusCode: 200,
            summaryError: nil
        )
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://app.example/prefix")!),
            accessToken: "user-token", transport: transport
        )
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)

        try await service.ensureThread()

        let requests = await transport.requests
        let request = try XCTUnwrap(requests.first)
        let labels = try XCTUnwrap(try object(request.body)["labels"] as? [String])
        XCTAssertTrue(labels.contains("agent:agent-1"))
        XCTAssertTrue(labels.contains("project:project-1"))
        XCTAssertTrue(labels.contains("thread-kind:todo_execution"))
    }

    func testReconciliationReadsExistingRecordsAndDoesNotResetSummaries() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        let entries = try records(scope)
        try await service.sync(entries, reconciling: false)
        try await service.sync(entries, reconciling: true)
        let requests = await transport.requests
        XCTAssertEqual(requests.filter { $0.method == "PUT" }.count, 1)
        XCTAssertEqual(requests.filter { $0.method == "GET" }.count, 2)
        XCTAssertTrue(requests.filter { $0.method == "GET" }.allSatisfy { $0.url.query?.contains("thread_id=") == true })
    }

    func testStableAgentThreadCanFlushRawRecordsCreatedByAnEarlierRun() async throws {
        let agentID = "manager-agent"
        let earlier = try AgentMemoryScope(
            tenantID: "user/a?&b",
            agentID: agentID,
            projectID: "project-a",
            runID: UUID(),
            runtimeScope: "manager:earlier"
        )
        let current = try AgentMemoryScope(
            tenantID: "user/a?&b",
            agentID: agentID,
            projectID: "project-b",
            runID: UUID(),
            runtimeScope: "manager:current"
        )
        XCTAssertEqual(earlier.subjectID, current.subjectID)
        XCTAssertEqual(earlier.threadID, current.threadID)
        let transport = MemoryTransport(
            scope: current,
            refreshToken: false,
            foreignTenant: false,
            statusCode: 200,
            summaryError: nil
        )
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://app.example/prefix")!),
            accessToken: "user-token",
            transport: transport
        )
        let service = try await ChatOSMemoryEngineService(client: client, scope: current)

        try await service.sync(try records(earlier), reconciling: false)

        let requests = await transport.requests
        XCTAssertEqual(requests.count, 2)
        XCTAssertTrue(requests[1].url.path.hasSuffix("/records/batch-sync"))
    }

    func testStableStoryAgentAcceptsRecordsFromAnEarlierRun() async throws {
        let (scope, transport, client) = try fixture()
        let other = try AgentMemoryScope(
            tenantID: scope.tenantID,
            profile: "story",
            projectID: UUID(),
            runID: UUID(),
            runtimeScope: scope.runtimeScope
        )
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)

        try await service.sync(try records(other), reconciling: false)
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 2)
    }

    func testIncompleteOrChangedReconciliationDoesNotWrite() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        do { try await service.sync(try records(scope), reconciling: true); XCTFail("Missing records need review") } catch {}
        let requests = await transport.requests
        XCTAssertFalse(requests.contains { $0.method == "PUT" })
        XCTAssertEqual(requests.filter { $0.method == "POST" }.map(\.url.path), [
            "/prefix/api/local/runtime-context-assertions",
        ])
        XCTAssertTrue(requests.filter { $0.method == "GET" }.allSatisfy {
            $0.url.path.contains("/records/")
        })
    }

    func testAccountChangeStopsOldServiceBeforeAnyNetworkRequest() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        try await client.setAccessToken("different-account-token")
        do { try await service.ensureThread(); XCTFail("Old session must stop") }
        catch { XCTAssertEqual(error as? ChatOSAPIError, .unauthorized) }
        let count = await transport.requests.count
        XCTAssertEqual(count, 0)
    }

    func testTokenRefreshIsAcceptedWithoutChangingAccountBinding() async throws {
        let (scope, transport, client) = try fixture(refreshToken: true)
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        try await service.ensureThread()
        _ = try await service.compose(try composeRequest())
        let calls = await transport.requests
        XCTAssertEqual(calls[1].headers["Authorization"], "Bearer refreshed-token")
    }

    func testForeignTenantResponsesAreRejected() async throws {
        let (scope, _, client) = try fixture(foreignTenant: true)
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        do { try await service.ensureThread(); XCTFail("Foreign tenant") }
        catch { XCTAssertNotNil(error as? AgentRuntimeError) }
    }

    func testUnauthorizedAndForbiddenAreNotRetried() async throws {
        for status in [401, 403] {
            let (scope, transport, client) = try fixture(statusCode: status)
            let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
            do { try await service.ensureThread(); XCTFail("Should reject HTTP \(status)") } catch {}
            let requests = await transport.requests
            XCTAssertEqual(
                requests.filter { $0.url.path.contains("/threads/") }.count,
                1,
                "The rejected Memory request must never be replayed"
            )
            XCTAssertEqual(
                requests.filter { $0.url.path.hasSuffix("/auth/refresh") }.count,
                status == 401 ? 1 : 0,
                "Only a 401 may perform one authoritative User Service refresh"
            )
        }
    }

    func testInvalidGatewayRootDoesNotLeakToken() async throws {
        let (scope, transport, _) = try fixture()
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://user:password@app.example")!), accessToken: "token", transport: transport)
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        do { try await service.ensureThread(); XCTFail("Invalid gateway root") }
        catch { XCTAssertEqual(error as? ChatOSAPIError, .invalidEndpoint) }
        let count = await transport.requests.count
        XCTAssertEqual(count, 0)
    }

    private func fixture(refreshToken: Bool = false, foreignTenant: Bool = false, statusCode: Int = 200,
                         summaryError: String? = nil) throws -> (AgentMemoryScope, MemoryTransport, ChatOSAPIClient) {
        let scope = try AgentMemoryScope(tenantID: "user/a?&b", profile: "story", projectID: UUID(), runID: UUID(), runtimeScope: "story:v1")
        let transport = MemoryTransport(scope: scope, refreshToken: refreshToken, foreignTenant: foreignTenant,
                                        statusCode: statusCode, summaryError: summaryError)
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://app.example/prefix")!), accessToken: "user-token", transport: transport)
        return (scope, transport, client)
    }
    private func records(_ scope: AgentMemoryScope) throws -> [AgentMemoryEntry] {
        let messages: [AgentMessage] = [
            .init(
                role: .assistant, content: "", toolCalls: [.init(id: "call-a", name: "work", arguments: "{}")],
                responseOutputJSON: Data(#"[{"type":"compaction"}]"#.utf8),
                usage: .init(inputTokens: 11, cachedTokens: 7, outputTokens: 3, requests: 1)
            ),
            .init(role: .tool, content: "done", toolCallID: "call-a"),
        ]
        return try messages.enumerated().map {
            .init(
                id: scope.recordID(at: $0.offset),
                index: $0.offset,
                message: $0.element,
                createdAt: Date(timeIntervalSince1970: 1_700_000_000 + Double($0.offset) / 1_000),
                conversationTurnID: "turn-0",
                toolExchange: $0.element.role == .tool
                    ? try AgentMemoryToolExchange(
                        assistantRecordID: scope.recordID(at: 0),
                        exchangeID: "agent:\(scope.runID.uuidString):tool:0",
                        callID: "call-a",
                        resultOrdinal: 0,
                        resultCount: 1
                    )
                    : nil
            )
        }
    }
    private func composeRequest() throws -> AgentMemoryComposeRequest {
        try AgentMemoryComposeRequest(
            modelCapability: AgentModelCapabilitySnapshot(
                revision: "test-capability-v1",
                contextWindowTokens: 200_000,
                outputReserveTokens: 30_000,
                protocolOverheadEstimatorID: "responses-json-v1",
                tokenizerID: nil
            ),
            queryText: "current query",
            currentTurnID: "turn-current"
        )
    }
    private func object(_ data: Data?) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(data)) as? [String: Any])
    }
}

private actor MemoryTransport: HTTPTransport {
    let scope: AgentMemoryScope
    let refreshToken: Bool
    let foreignTenant: Bool
    let statusCode: Int
    let summaryError: String?
    var requests: [HTTPRequest] = []
    var stored: [[String: Any]] = []
    init(scope: AgentMemoryScope, refreshToken: Bool, foreignTenant: Bool, statusCode: Int,
         summaryError: String?) {
        self.scope = scope; self.refreshToken = refreshToken; self.foreignTenant = foreignTenant
        self.statusCode = statusCode; self.summaryError = summaryError
    }
    func send(_ request: HTTPRequest) throws -> HTTPResponse {
        requests.append(request)
        let body = try request.body.map { try JSONSerialization.jsonObject(with: $0) as? [String: Any] } ?? nil
        let result: [String: Any]
        let path = request.url.path
        if statusCode != 200 { result = ["error": "test error"] }
        else if path.hasSuffix("/records/batch-sync") {
            let batch = body?["records"] as? [[String: Any]] ?? []
            for var record in batch {
                record["tenant_id"] = scope.tenantID; record["source_id"] = scope.sourceID; record["thread_id"] = scope.threadID
                record["summary_status"] = "summarized"
                stored.append(record)
            }
            result = ["thread_id": scope.threadID, "received_count": batch.count, "upserted_count": batch.count]
        } else if path.hasSuffix("/runtime-context-assertions") {
            result = [
                "runtime_context_assertion": "assertion-\(UUID().uuidString)",
                "source_id": scope.sourceID,
                "canonical_agent_subject_id": scope.subjectID,
                "thread_id": scope.threadID,
                "project_partition_key": scope.projectSubjectID ?? "__none__",
                "runtime_identity_revision": "test-runtime-v1",
            ]
        } else if path.hasSuffix("/context/compose") {
            result = ["thread_id": scope.threadID, "blocks": [], "recent_records": stored,
                      "meta": ["summary_count": 0, "recent_record_count": stored.count]]
        } else if path.contains("/active-summary/") {
            result = ["thread_id": scope.threadID, "job_run_id": "job/a?b", "accepted": true, "running": false,
                      "completed": summaryError == nil, "failed": summaryError != nil,
                      "generated": summaryError == nil, "compacted": summaryError == nil,
                      "error_message": summaryError as Any? ?? NSNull()]
        } else if path.contains("/records/") {
            result = ["item": stored.first { $0["id"] as? String == request.url.lastPathComponent } as Any? ?? NSNull()]
        } else {
            result = ["id": scope.threadID, "tenant_id": foreignTenant ? "foreign" : scope.tenantID,
                      "source_id": scope.sourceID, "subject_id": scope.subjectID]
        }
        return .init(statusCode: statusCode, headers: refreshToken ? ["x-access-token": "refreshed-token"] : [:],
                     body: try JSONSerialization.data(withJSONObject: result))
    }
}
