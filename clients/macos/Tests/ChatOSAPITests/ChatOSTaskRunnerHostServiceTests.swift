import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSTaskRunnerHostServiceTests: XCTestCase {
    func testPreparesTopologicalProjectBatchAndReusesItIdempotently() async throws {
        let transport = TaskRunnerHostTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSTaskRunnerHostService(client: client)
        let request = PluginHostTaskBatchRequest(idempotencyKey: "intent-1:plan-1:v1", tasks: [
            .init(clientRef: "child", title: "验证", objective: "验证实现", prerequisiteRefs: ["root"]),
            .init(clientRef: "root", title: "实现", objective: "完成实现"),
        ])
        let first = try await service.prepareBatch(
            request,
            project: try project(),
            host: host(),
            defaultModelConfigID: "model-1"
        )
        XCTAssertFalse(first.reused)
        XCTAssertEqual(first.tasks.map(\.clientRef), ["child", "root"])
        let captured = await transport.snapshot()
        XCTAssertEqual(captured.titles, ["实现", "验证"])
        XCTAssertEqual(captured.prerequisites, [[], ["task-1"]])
        XCTAssertEqual(captured.projectID, "project-1")
        XCTAssertEqual(captured.defaultModelConfigIDs, ["model-1", "model-1"])
        XCTAssertFalse(captured.contextContainsOwner)
        XCTAssertFalse(captured.contextContainsRootPath)

        let second = try await service.prepareBatch(
            request,
            project: try project(),
            host: host(),
            defaultModelConfigID: "model-1"
        )
        XCTAssertTrue(second.reused)
        XCTAssertEqual(second.tasks.map(\.taskID), first.tasks.map(\.taskID))
        let final = await transport.snapshot()
        XCTAssertEqual(final.postCount, 2)
        XCTAssertTrue(final.paths.allSatisfy { $0.hasPrefix("/api/task/") })
    }

    func testRejectsCyclesBeforeCallingTaskRunner() async throws {
        let transport = TaskRunnerHostTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSTaskRunnerHostService(client: client)
        let request = PluginHostTaskBatchRequest(idempotencyKey: "batch", tasks: [
            .init(clientRef: "a", title: "A", objective: "A", prerequisiteRefs: ["b"]),
            .init(clientRef: "b", title: "B", objective: "B", prerequisiteRefs: ["a"]),
        ])
        do {
            _ = try await service.prepareBatch(
                request,
                project: try project(),
                host: host(),
                defaultModelConfigID: "model-1"
            )
            XCTFail("expected cycle rejection")
        } catch let error as ChatOSAPIError {
            XCTAssertEqual(error, .invalidRequest("任务依赖图存在循环"))
        }
        let snapshot = await transport.snapshot()
        XCTAssertEqual(snapshot.paths.count, 0)
    }

    private func project() throws -> ProjectContextSnapshot {
        try ProjectContextSnapshot(
            record: .init(
                id: "project-1", ownerUserID: "owner",
                draft: .init(name: "Project", workspaceID: "workspace-1", relativeRoot: "repo"),
                createdAtUnixMs: 1, updatedAtUnixMs: 1
            ),
            deviceID: "device-1"
        )
    }

    private func host() -> PluginHostIdentity {
        .init(pluginID: "plugin-1", componentKey: "workbench", releaseID: "release-1", version: "1.0.0", artifactSHA256: String(repeating: "a", count: 64))
    }
}

private actor TaskRunnerHostTransport: HTTPTransport {
    private(set) var paths: [String] = []
    private(set) var createdBodies: [[String: Any]] = []
    private var records: [[String: Any]] = []
    struct Snapshot: Sendable {
        let paths: [String]
        let titles: [String]
        let prerequisites: [[String]]
        let projectID: String?
        let defaultModelConfigIDs: [String]
        let contextContainsOwner: Bool
        let contextContainsRootPath: Bool
        let postCount: Int
    }

    func snapshot() -> Snapshot {
        let context = createdBodies.first?["project_context"] as? [String: Any]
        return .init(
            paths: paths,
            titles: createdBodies.compactMap { $0["title"] as? String },
            prerequisites: createdBodies.map { $0["prerequisite_task_ids"] as? [String] ?? [] },
            projectID: context?["projectId"] as? String,
            defaultModelConfigIDs: createdBodies.compactMap { $0["default_model_config_id"] as? String },
            contextContainsOwner: context?["ownerUserId"] != nil,
            contextContainsRootPath: context?["rootPath"] != nil,
            postCount: createdBodies.count
        )
    }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        paths.append(request.url.path)
        guard request.url.path == "/api/task/tasks" else { throw URLError(.unsupportedURL) }
        if request.method == "GET" {
            return response(records)
        }
        guard request.method == "POST", let body = request.body,
              let value = try JSONSerialization.jsonObject(with: body) as? [String: Any] else {
            throw URLError(.cannotParseResponse)
        }
        createdBodies.append(value)
        var record = value
        record["id"] = "task-\(createdBodies.count)"
        record["last_run_id"] = NSNull()
        record["updated_at"] = "2026-09-09T00:00:00Z"
        records.append(record)
        return response(record, status: 201)
    }

    private func response(_ value: Any, status: Int = 200) -> HTTPResponse {
        HTTPResponse(statusCode: status, headers: [:], body: try! JSONSerialization.data(withJSONObject: value))
    }
}
