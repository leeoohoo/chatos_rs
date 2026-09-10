import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSMemoryEngineServiceTests: XCTestCase {
    func testUsesPublicSiblingGatewayAndUserBearerWithStableRecordIDs() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        try await service.ensureThread()
        let entries = records(scope)
        try await service.sync(entries, reconciling: false)
        let composed = try await service.compose()
        XCTAssertEqual(composed.recentRecordIDs, entries.map(\.id))
        let started = try await service.startSummary(reason: "active_context_budget")
        let status = try await service.summaryStatus(jobID: started.jobID)
        XCTAssertTrue(status.completed)
        let calls = await transport.requests
        XCTAssertEqual(calls.count, 5)
        XCTAssertEqual(calls.map(\.url.path), [
            "/prefix/api/memory/threads/\(scope.threadID)",
            "/prefix/api/memory/threads/\(scope.threadID)/records/batch-sync",
            "/prefix/api/memory/context/compose",
            "/prefix/api/memory/threads/\(scope.threadID)/active-summary/run",
            "/prefix/api/memory/threads/\(scope.threadID)/active-summary/status",
        ])
        XCTAssertEqual(calls.map(\.method), ["PUT", "PUT", "POST", "POST", "GET"])
        XCTAssertTrue(calls.allSatisfy { $0.headers["Authorization"] == "Bearer user-token" })
        XCTAssertFalse(calls.contains { $0.url.path.contains("/api/chatos/") || $0.url.path.contains("/sdk/") || $0.url.path.contains("/api/memory-engine/") })
        XCTAssertFalse(calls.contains { $0.headers.keys.contains { $0.lowercased().hasPrefix("x-memory-") } })
        let body = try object(calls[1].body)
        XCTAssertEqual(body["source_id"] as? String, "chatos")
        XCTAssertEqual(body["tenant_id"] as? String, scope.tenantID)
        let records = try XCTUnwrap(body["records"] as? [[String: Any]])
        XCTAssertEqual(records.map { $0["id"] as? String }, entries.map(\.id))
        let firstPayload = try XCTUnwrap(records[0]["structured_payload"] as? [String: Any])
        XCTAssertEqual((firstPayload["tool_calls"] as? [[String: Any]])?.first?["id"] as? String, "call-a")
        XCTAssertEqual((records[1]["structured_payload"] as? [String: Any])?["tool_call_id"] as? String, "call-a")
        XCTAssertNil(records[0]["summary_status"])
        let policy = try XCTUnwrap(try object(calls[2].body)["policy"] as? [String: Any])
        XCTAssertEqual(policy["include_subject_memory"] as? Bool, false)
        XCTAssertNil(policy["recent_record_limit"], "Do not silently drop unsummarized records")
        XCTAssertFalse(calls.contains { String(decoding: $0.body ?? Data(), as: UTF8.self).contains("user-token") })
    }

    func testReconciliationReadsExistingRecordsAndDoesNotResetSummaries() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        let entries = records(scope)
        try await service.sync(entries, reconciling: false)
        try await service.sync(entries, reconciling: true)
        let requests = await transport.requests
        XCTAssertEqual(requests.filter { $0.method == "PUT" }.count, 1)
        XCTAssertEqual(requests.filter { $0.method == "GET" }.count, 2)
        XCTAssertTrue(requests.dropFirst().allSatisfy { $0.url.query?.contains("thread_id=") == true })
    }

    func testIncompleteOrChangedReconciliationDoesNotWrite() async throws {
        let (scope, transport, client) = try fixture()
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        do { try await service.sync(records(scope), reconciling: true); XCTFail("Missing records need review") } catch {}
        let requests = await transport.requests
        XCTAssertTrue(requests.allSatisfy { $0.method == "GET" })
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
        _ = try await service.compose()
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
            XCTAssertEqual(requests.count, 1)
        }
    }

    func testUnsupportedBasePathDoesNotGuessOrLeakToken() async throws {
        let (scope, transport, _) = try fixture()
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://app.example/unknown-api")!), accessToken: "token", transport: transport)
        let service = try await ChatOSMemoryEngineService(client: client, scope: scope)
        do { try await service.ensureThread(); XCTFail("Unknown gateway layout") }
        catch { XCTAssertEqual(error as? ChatOSAPIError, .invalidEndpoint) }
        let count = await transport.requests.count
        XCTAssertEqual(count, 0)
    }

    private func fixture(refreshToken: Bool = false, foreignTenant: Bool = false, statusCode: Int = 200) throws -> (AgentMemoryScope, MemoryTransport, ChatOSAPIClient) {
        let scope = try AgentMemoryScope(tenantID: "user/a?&b", profile: "story", projectID: UUID(), runID: UUID(), runtimeScope: "story:v1")
        let transport = MemoryTransport(scope: scope, refreshToken: refreshToken, foreignTenant: foreignTenant, statusCode: statusCode)
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://app.example/prefix/api/chatos")!), accessToken: "user-token", transport: transport)
        return (scope, transport, client)
    }
    private func records(_ scope: AgentMemoryScope) -> [AgentMemoryEntry] {
        let messages: [AgentMessage] = [.init(role: .assistant, toolCalls: [.init(id: "call-a", name: "work", arguments: "{}")]),
                                        .init(role: .tool, content: "done", toolCallID: "call-a")]
        return messages.enumerated().map { .init(id: scope.recordID(at: $0.offset), index: $0.offset, message: $0.element,
                                                createdAt: Date(timeIntervalSince1970: 1_700_000_000 + Double($0.offset) / 1_000)) }
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
    var requests: [HTTPRequest] = []
    var stored: [[String: Any]] = []
    init(scope: AgentMemoryScope, refreshToken: Bool, foreignTenant: Bool, statusCode: Int) {
        self.scope = scope; self.refreshToken = refreshToken; self.foreignTenant = foreignTenant; self.statusCode = statusCode
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
        } else if path.hasSuffix("/context/compose") {
            result = ["thread_id": scope.threadID, "blocks": [], "recent_records": stored,
                      "meta": ["summary_count": 0, "recent_record_count": stored.count]]
        } else if path.contains("/active-summary/") {
            result = ["thread_id": scope.threadID, "job_run_id": "job/a?b", "running": false,
                      "completed": true, "failed": false, "compacted": true]
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
