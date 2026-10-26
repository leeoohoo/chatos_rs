@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native Local Agent Hook history client")
struct NativeLocalAgentHookBundleClientTests {
    @Test("requests a scoped page and decodes metadata-only diagnostics")
    func listsHookDiagnostics() async throws {
        let host = HookHistoryHostStub()
        let page = try await NativeLocalAgentHookBundleClient(host: host).listInvocations(
            ownerUserID: "user-1",
            pluginID: "plugin-1",
            beforeUpdatedAtUnixMs: 200,
            beforeInvocationID: "invocation-cursor",
            limit: 20
        )

        let command = try #require(await host.lastCommand())
        #expect(command.type == "list_plugin_hook_invocations")
        #expect(command.ownerUserID == "user-1")
        #expect(command.pluginID == "plugin-1")
        #expect(command.beforeUpdatedAtUnixMs == 200)
        #expect(command.beforeInvocationID == "invocation-cursor")
        #expect(command.limit == 20)

        let invocation = try #require(page.invocations.first)
        #expect(invocation.invocationID == "invocation-1")
        #expect(invocation.eventType == "SessionStart")
        #expect(invocation.isAsync)
        #expect(invocation.status == "failed")
        #expect(invocation.exitCode == 7)
        #expect(invocation.stderrOverflow)
        #expect(page.nextBeforeUpdatedAtUnixMs == 100)
        #expect(page.nextBeforeInvocationID == "invocation-1")
    }
}

private actor HookHistoryHostStub: LocalAgentHostClientServicing {
    private var command: HookHistoryCommandSnapshot?

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command data: Data) async throws -> Data {
        let object = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        command = HookHistoryCommandSnapshot(
            type: object["type"] as? String ?? "",
            ownerUserID: object["owner_user_id"] as? String ?? "",
            pluginID: object["plugin_id"] as? String ?? "",
            beforeUpdatedAtUnixMs: object["before_updated_at_unix_ms"] as? Int ?? -1,
            beforeInvocationID: object["before_invocation_id"] as? String ?? "",
            limit: object["limit"] as? Int ?? -1
        )
        return try JSONSerialization.data(withJSONObject: [
            "type": "plugin_hook_invocations",
            "page": [
                "invocations": [[
                    "invocation_id": "invocation-1",
                    "bundle_id": "bundle-1",
                    "hook_id": "hook-1",
                    "event_type": "SessionStart",
                    "is_async": true,
                    "attempt": 2,
                    "status": "failed",
                    "exit_code": 7,
                    "timed_out": false,
                    "duration_ms": 55,
                    "stdout_bytes": 12,
                    "stdout_overflow": false,
                    "stderr_bytes": 34,
                    "stderr_overflow": true,
                    "created_at_unix_ms": 90,
                    "updated_at_unix_ms": 100,
                ]],
                "next_before_updated_at_unix_ms": 100,
                "next_before_invocation_id": "invocation-1",
            ],
        ])
    }

    func lastCommand() -> HookHistoryCommandSnapshot? { command }
}

private struct HookHistoryCommandSnapshot: Sendable {
    var type: String
    var ownerUserID: String
    var pluginID: String
    var beforeUpdatedAtUnixMs: Int
    var beforeInvocationID: String
    var limit: Int
}
