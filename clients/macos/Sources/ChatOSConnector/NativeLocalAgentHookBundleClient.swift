import ChatOSCore
import Foundation

struct NativeLocalAgentHookBundleClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    func publish(_ trust: NativePluginHookTrustRecord) async throws {
        let response: PutResult = try await host.request(PutCommand(
            type: "put_plugin_hook_bundle",
            bundle: WireBundle(trust)
        ))
        guard response.type == "plugin_hook_bundle",
              response.bundle.bundleID == trust.bundleID else {
            throw NativeLocalAgentHookBundleClientError.invalidResponse
        }
    }

    func list(ownerUserID: String) async throws -> [String] {
        let response: ListResult = try await host.request(ListCommand(
            type: "list_plugin_hook_bundles",
            ownerUserID: ownerUserID
        ))
        guard response.type == "plugin_hook_bundles" else {
            throw NativeLocalAgentHookBundleClientError.invalidResponse
        }
        return response.bundles.map(\.bundleID)
    }

    func setActive(ownerUserID: String, bundleIDs: [String]) async throws {
        let response: ActiveResult = try await host.request(ActiveCommand(
            type: "set_active_plugin_hook_bundles",
            ownerUserID: ownerUserID,
            bundleIDs: bundleIDs
        ))
        guard response.type == "active_plugin_hook_bundles_updated",
              response.activeBundleIDs == bundleIDs.sorted() else {
            throw NativeLocalAgentHookBundleClientError.invalidResponse
        }
    }

    func listInvocations(
        ownerUserID: String,
        pluginID: String,
        beforeUpdatedAtUnixMs: Int64?,
        beforeInvocationID: String?,
        limit: Int
    ) async throws -> LocalConnectorPluginHookHistoryPage {
        let response: InvocationListResult = try await host.request(InvocationListCommand(
            type: "list_plugin_hook_invocations",
            ownerUserID: ownerUserID,
            pluginID: pluginID,
            beforeUpdatedAtUnixMs: beforeUpdatedAtUnixMs,
            beforeInvocationID: beforeInvocationID,
            limit: limit
        ))
        guard response.type == "plugin_hook_invocations" else {
            throw NativeLocalAgentHookBundleClientError.invalidResponse
        }
        return LocalConnectorPluginHookHistoryPage(
            invocations: response.page.invocations.map(\.model),
            nextBeforeUpdatedAtUnixMs: response.page.nextBeforeUpdatedAtUnixMs,
            nextBeforeInvocationID: response.page.nextBeforeInvocationID
        )
    }
}

private enum NativeLocalAgentHookBundleClientError: LocalizedError {
    case invalidResponse

    var errorDescription: String? { "Local Agent Host 返回了无效的 Hook Bundle 响应" }
}

private struct PutCommand: Encodable, Sendable {
    var type: String
    var bundle: WireBundle
}

private struct ListCommand: Encodable, Sendable {
    var type: String
    var ownerUserID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
    }
}

private struct ActiveCommand: Encodable, Sendable {
    var type: String
    var ownerUserID: String
    var bundleIDs: [String]

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case bundleIDs = "bundle_ids"
    }
}

private struct InvocationListCommand: Encodable, Sendable {
    var type: String
    var ownerUserID: String
    var pluginID: String
    var beforeUpdatedAtUnixMs: Int64?
    var beforeInvocationID: String?
    var limit: Int

    private enum CodingKeys: String, CodingKey {
        case type, limit
        case ownerUserID = "owner_user_id"
        case pluginID = "plugin_id"
        case beforeUpdatedAtUnixMs = "before_updated_at_unix_ms"
        case beforeInvocationID = "before_invocation_id"
    }
}

private struct WireBundle: Encodable, Sendable {
    var bundleID: String
    var ownerUserID: String
    var pluginID: String
    var releaseID: String
    var componentKey: String
    var artifactSHA256: String
    var contentSHA256: String
    var normalizedHookSHA256: String
    var commandSHA256: String
    var permissionSnapshotSHA256: String
    var runnerPolicyVersion: UInt32
    var installationRoot: String
    var permissions: [String]
    var hooks: [WireHook]
    var enabled = true
    var trustedAtUnixMs: Int64

    init(_ trust: NativePluginHookTrustRecord) {
        bundleID = trust.bundleID
        ownerUserID = trust.ownerUserID
        pluginID = trust.pluginID
        releaseID = trust.releaseID
        componentKey = trust.componentKey
        artifactSHA256 = trust.artifactSHA256
        contentSHA256 = trust.contentSHA256
        normalizedHookSHA256 = trust.normalizedHookSHA256
        commandSHA256 = trust.commandSHA256
        permissionSnapshotSHA256 = trust.permissionSnapshotSHA256
        runnerPolicyVersion = trust.runnerPolicyVersion
        installationRoot = trust.installationRoot
        permissions = trust.permissions
        hooks = trust.hooks.map(WireHook.init)
        trustedAtUnixMs = trust.trustedAtUnixMs
    }

    private enum CodingKeys: String, CodingKey {
        case permissions, hooks, enabled
        case bundleID = "bundle_id"
        case ownerUserID = "owner_user_id"
        case pluginID = "plugin_id"
        case releaseID = "release_id"
        case componentKey = "component_key"
        case artifactSHA256 = "artifact_sha256"
        case contentSHA256 = "content_sha256"
        case normalizedHookSHA256 = "normalized_hook_sha256"
        case commandSHA256 = "command_sha256"
        case permissionSnapshotSHA256 = "permission_snapshot_sha256"
        case runnerPolicyVersion = "runner_policy_version"
        case installationRoot = "installation_root"
        case trustedAtUnixMs = "trusted_at_unix_ms"
    }
}

private struct WireHook: Encodable, Sendable {
    var hookID: String
    var event: String
    var matcher: String?
    var command: String
    var isAsync: Bool
    var timeoutMs: UInt64
    var additionalContextLimit: UInt64

    init(_ hook: NativeCodexHookDefinition) {
        hookID = hook.id
        event = hook.event
        matcher = hook.matcher
        command = hook.command
        isAsync = hook.isAsync
        timeoutMs = UInt64((hook.timeoutSeconds * 1_000).rounded())
        additionalContextLimit = UInt64(hook.additionalContextLimit)
    }

    private enum CodingKeys: String, CodingKey {
        case event, matcher, command
        case hookID = "hook_id"
        case isAsync = "is_async"
        case timeoutMs = "timeout_ms"
        case additionalContextLimit = "additional_context_limit"
    }
}

private struct PutResult: Decodable, Sendable {
    var type: String
    var bundle: BundleReference
}

private struct ListResult: Decodable, Sendable {
    var type: String
    var bundles: [BundleReference]
}

private struct ActiveResult: Decodable, Sendable {
    var type: String
    var activeBundleIDs: [String]

    private enum CodingKeys: String, CodingKey {
        case type
        case activeBundleIDs = "active_bundle_ids"
    }
}

private struct InvocationListResult: Decodable, Sendable {
    var type: String
    var page: InvocationPage
}

private struct InvocationPage: Decodable, Sendable {
    var invocations: [InvocationDiagnostic]
    var nextBeforeUpdatedAtUnixMs: Int64?
    var nextBeforeInvocationID: String?

    private enum CodingKeys: String, CodingKey {
        case invocations
        case nextBeforeUpdatedAtUnixMs = "next_before_updated_at_unix_ms"
        case nextBeforeInvocationID = "next_before_invocation_id"
    }
}

private struct InvocationDiagnostic: Decodable, Sendable {
    var invocationID: String
    var bundleID: String
    var hookID: String
    var eventType: String
    var isAsync: Bool
    var attempt: UInt32
    var status: String
    var exitCode: Int32?
    var timedOut: Bool
    var durationMs: UInt64
    var stdoutBytes: UInt64
    var stdoutOverflow: Bool
    var stderrBytes: UInt64
    var stderrOverflow: Bool
    var createdAtUnixMs: Int64
    var updatedAtUnixMs: Int64

    var model: LocalConnectorPluginHookInvocation {
        .init(
            invocationID: invocationID,
            bundleID: bundleID,
            hookID: hookID,
            eventType: eventType,
            isAsync: isAsync,
            attempt: attempt,
            status: status,
            exitCode: exitCode,
            timedOut: timedOut,
            durationMs: durationMs,
            stdoutBytes: stdoutBytes,
            stdoutOverflow: stdoutOverflow,
            stderrBytes: stderrBytes,
            stderrOverflow: stderrOverflow,
            createdAtUnixMs: createdAtUnixMs,
            updatedAtUnixMs: updatedAtUnixMs
        )
    }

    private enum CodingKeys: String, CodingKey {
        case attempt, status
        case invocationID = "invocation_id"
        case bundleID = "bundle_id"
        case hookID = "hook_id"
        case eventType = "event_type"
        case isAsync = "is_async"
        case exitCode = "exit_code"
        case timedOut = "timed_out"
        case durationMs = "duration_ms"
        case stdoutBytes = "stdout_bytes"
        case stdoutOverflow = "stdout_overflow"
        case stderrBytes = "stderr_bytes"
        case stderrOverflow = "stderr_overflow"
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

private struct BundleReference: Decodable, Sendable {
    var bundleID: String

    private enum CodingKeys: String, CodingKey {
        case bundleID = "bundle_id"
    }
}
