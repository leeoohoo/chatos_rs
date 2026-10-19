import ChatOSCore
import Foundation

public struct LocalAgentToolInvocationRecord: Decodable, Sendable, Equatable {
    public let invocationID: String
    public let runID: String
    public let batchID: String
    public let callID: String
    public let toolName: String
    public let executionRoute: String
    public let arguments: LocalAgentJSONValue
    public let sideEffecting: Bool
    public let requiresApproval: Bool
    public let approvalStatus: String
    public let approvalDecidedBy: String?
    public let approvalReason: String?
    public let approvalDecidedAtUnixMs: Int64?
    public let status: String
    public let result: LocalAgentJSONValue?
    public let error: String?
    public let version: UInt64
    public let claimToken: String?
    public let claimUntilUnixMs: Int64?
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case invocationID = "invocation_id"
        case runID = "run_id"
        case batchID = "batch_id"
        case callID = "call_id"
        case toolName = "tool_name"
        case executionRoute = "execution_route"
        case arguments
        case sideEffecting = "side_effecting"
        case requiresApproval = "requires_approval"
        case approvalStatus = "approval_status"
        case approvalDecidedBy = "approval_decided_by"
        case approvalReason = "approval_reason"
        case approvalDecidedAtUnixMs = "approval_decided_at_unix_ms"
        case status, result, error, version
        case claimToken = "claim_token"
        case claimUntilUnixMs = "claim_until_unix_ms"
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }
}

struct LocalRunCapabilityDisclosure: Decodable, Sendable, Hashable {
    let pluginOption: String
    let toolOption: String
    let providerID: String
    let schemaSHA256: String

    private enum CodingKeys: String, CodingKey {
        case pluginOption = "plugin_option"
        case toolOption = "tool_option"
        case providerID = "provider_id"
        case schemaSHA256 = "schema_sha256"
    }
}

public struct LocalAgentToolClaim: Decodable, Sendable, Equatable {
    public let workerID: String
    public let claimToken: String
    public let invocation: LocalAgentToolInvocationRecord
    public let preflightOutcome: LocalAgentToolOutcome?

    private enum CodingKeys: String, CodingKey {
        case workerID = "worker_id"
        case claimToken = "claim_token"
        case invocation
        case preflightOutcome = "preflight_outcome"
    }
}

public enum LocalAgentToolOutcome: Codable, Sendable, Equatable {
    case succeeded(LocalAgentJSONValue)
    case failed(error: String, detail: LocalAgentJSONValue)
    case needsReview(reason: String, detail: LocalAgentJSONValue)

    private enum CodingKeys: String, CodingKey { case type, output, error, detail, reason }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "succeeded":
            self = .succeeded(try container.decode(LocalAgentJSONValue.self, forKey: .output))
        case "failed":
            self = .failed(
                error: try container.decode(String.self, forKey: .error),
                detail: try container.decode(LocalAgentJSONValue.self, forKey: .detail)
            )
        case "needs_review":
            self = .needsReview(
                reason: try container.decode(String.self, forKey: .reason),
                detail: try container.decode(LocalAgentJSONValue.self, forKey: .detail)
            )
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type,
                in: container,
                debugDescription: "Unknown local tool outcome type"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .succeeded(output):
            try container.encode("succeeded", forKey: .type)
            try container.encode(output, forKey: .output)
        case let .failed(error, detail):
            try container.encode("failed", forKey: .type)
            try container.encode(error, forKey: .error)
            try container.encode(detail, forKey: .detail)
        case let .needsReview(reason, detail):
            try container.encode("needs_review", forKey: .type)
            try container.encode(reason, forKey: .reason)
            try container.encode(detail, forKey: .detail)
        }
    }
}

public struct LocalAgentToolCommitResult: Decodable, Sendable, Equatable {
    public let invocation: LocalAgentToolInvocationRecord
    public let run: LocalAgentRunRecord
}

public struct LocalAgentToolApprovalResult: Decodable, Sendable, Equatable {
    public let invocation: LocalAgentToolInvocationRecord
    public let run: LocalAgentRunRecord
}

public struct NativeLocalAgentToolClient: Sendable {
    public static let reservedRustToolNames = [
        "list_tasks",
        "get_task",
        "create_task",
        "create_tasks_with_prerequisites",
        "cancel_task",
        "wait_for_task_completion",
        "get_task_dependency_graph",
        "resolve_task_followup",
    ]

    private let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    public func claimNext(
        ownerUserID: String,
        workerID: String,
        leaseDurationMilliseconds: UInt64 = 30_000,
        includeToolNames: [String]? = nil,
        excludeToolNames: [String] = []
    ) async throws -> LocalAgentToolClaim? {
        let excluded = Array(Set(
            excludeToolNames + Self.reservedRustToolNames
        )).sorted()
        let result: ClaimResult = try await host.request(ClaimCommand(
            type: "claim_next_tool",
            ownerUserID: ownerUserID,
            workerID: workerID,
            leaseDurationMilliseconds: leaseDurationMilliseconds,
            includeToolNames: includeToolNames,
            excludeToolNames: excluded
        ))
        guard result.type == "tool_claim" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.claim
    }

    public func commit(
        ownerUserID: String,
        claim: LocalAgentToolClaim,
        outcome: LocalAgentToolOutcome
    ) async throws -> LocalAgentToolCommitResult {
        let result: CommitResult = try await host.request(CommitCommand(
            type: "commit_tool",
            ownerUserID: ownerUserID,
            invocationID: claim.invocation.invocationID,
            claimToken: claim.claimToken,
            expectedVersion: claim.invocation.version,
            outcome: outcome
        ))
        guard result.type == "tool_commit" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.result
    }

    public func renew(
        ownerUserID: String,
        claim: LocalAgentToolClaim,
        leaseDurationMilliseconds: UInt64 = 30_000
    ) async throws -> Bool {
        let result: RenewResult = try await host.request(RenewCommand(
            type: "renew_tool_claim",
            ownerUserID: ownerUserID,
            invocationID: claim.invocation.invocationID,
            claimToken: claim.claimToken,
            expectedVersion: claim.invocation.version,
            leaseDurationMilliseconds: leaseDurationMilliseconds
        ))
        guard result.type == "tool_claim_renewed" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.renewed
    }

    public func pendingApprovals(
        ownerUserID: String,
        limit: UInt32 = 20
    ) async throws -> [LocalAgentToolInvocationRecord] {
        let result: PendingApprovalsResult = try await host.request(
            ListPendingApprovalsCommand(
                type: "list_pending_tool_approvals",
                ownerUserID: ownerUserID,
                limit: limit
            )
        )
        guard result.type == "pending_tool_approvals" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.invocations
    }

    public func activatedSkillRefs(
        ownerUserID: String,
        runID: String
    ) async throws -> Set<String> {
        let result: RunSkillActivationsResult = try await host.request(
            GetRunSkillActivationsCommand(
                type: "get_run_skill_activations",
                ownerUserID: ownerUserID,
                runID: runID
            )
        )
        guard result.type == "run_skill_activations", result.runID == runID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return Set(result.skillRefs)
    }

    func capabilityDisclosures(
        ownerUserID: String,
        runID: String
    ) async throws -> Set<LocalRunCapabilityDisclosure> {
        let result: RunCapabilityDisclosuresResult = try await host.request(
            GetRunCapabilityDisclosuresCommand(
                type: "get_run_capability_disclosures",
                ownerUserID: ownerUserID,
                runID: runID
            )
        )
        guard result.type == "run_capability_disclosures", result.runID == runID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return Set(result.disclosures)
    }

    public func skillBundle(
        ownerUserID: String,
        runID: String
    ) async throws -> LocalAgentSkillBundleSnapshot? {
        let result: RunSkillBundleResult = try await host.request(
            GetRunSkillActivationsCommand(
                type: "get_run_skill_bundle",
                ownerUserID: ownerUserID,
                runID: runID
            )
        )
        guard result.type == "run_skill_bundle", result.runID == runID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.bundle
    }

    public func decideApproval(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        approve: Bool,
        decidedBy: String,
        reason: String
    ) async throws -> LocalAgentToolApprovalResult {
        let result: ToolApprovalResult = try await host.request(DecideApprovalCommand(
            type: "decide_tool_approval",
            ownerUserID: ownerUserID,
            invocationID: invocation.invocationID,
            expectedVersion: invocation.version,
            decision: approve ? "approve" : "reject",
            decidedBy: decidedBy,
            reason: reason
        ))
        guard result.type == "tool_approval" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.result
    }
}

private struct ClaimCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let workerID: String
    let leaseDurationMilliseconds: UInt64
    let includeToolNames: [String]?
    let excludeToolNames: [String]

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case workerID = "worker_id"
        case leaseDurationMilliseconds = "lease_duration_ms"
        case includeToolNames = "include_tool_names"
        case excludeToolNames = "exclude_tool_names"
    }
}

private struct CommitCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let invocationID: String
    let claimToken: String
    let expectedVersion: UInt64
    let outcome: LocalAgentToolOutcome

    private enum CodingKeys: String, CodingKey {
        case type, outcome
        case ownerUserID = "owner_user_id"
        case invocationID = "invocation_id"
        case claimToken = "claim_token"
        case expectedVersion = "expected_version"
    }
}

private struct RenewCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let invocationID: String
    let claimToken: String
    let expectedVersion: UInt64
    let leaseDurationMilliseconds: UInt64

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case invocationID = "invocation_id"
        case claimToken = "claim_token"
        case expectedVersion = "expected_version"
        case leaseDurationMilliseconds = "lease_duration_ms"
    }
}

private struct ListPendingApprovalsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let limit: UInt32

    private enum CodingKeys: String, CodingKey {
        case type, limit
        case ownerUserID = "owner_user_id"
    }
}

private struct GetRunSkillActivationsCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let runID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case runID = "run_id"
    }
}

private struct GetRunCapabilityDisclosuresCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let runID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case runID = "run_id"
    }
}

private struct DecideApprovalCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let invocationID: String
    let expectedVersion: UInt64
    let decision: String
    let decidedBy: String
    let reason: String

    private enum CodingKeys: String, CodingKey {
        case type, decision, reason
        case ownerUserID = "owner_user_id"
        case invocationID = "invocation_id"
        case expectedVersion = "expected_version"
        case decidedBy = "decided_by"
    }
}

private struct ClaimResult: Decodable, Sendable {
    let type: String
    let claim: LocalAgentToolClaim?
}

private struct CommitResult: Decodable, Sendable {
    let type: String
    let result: LocalAgentToolCommitResult
}

private struct RenewResult: Decodable, Sendable {
    let type: String
    let renewed: Bool
}

private struct PendingApprovalsResult: Decodable, Sendable {
    let type: String
    let invocations: [LocalAgentToolInvocationRecord]
}

private struct RunSkillActivationsResult: Decodable, Sendable {
    let type: String
    let runID: String
    let skillRefs: [String]

    private enum CodingKeys: String, CodingKey {
        case type
        case runID = "run_id"
        case skillRefs = "skill_refs"
    }
}

private struct RunCapabilityDisclosuresResult: Decodable, Sendable {
    let type: String
    let runID: String
    let disclosures: [LocalRunCapabilityDisclosure]

    private enum CodingKeys: String, CodingKey {
        case type, disclosures
        case runID = "run_id"
    }
}

private struct RunSkillBundleResult: Decodable, Sendable {
    let type: String
    let runID: String
    let bundle: LocalAgentSkillBundleSnapshot?

    private enum CodingKeys: String, CodingKey {
        case type, bundle
        case runID = "run_id"
    }
}

private struct ToolApprovalResult: Decodable, Sendable {
    let type: String
    let result: LocalAgentToolApprovalResult
}
