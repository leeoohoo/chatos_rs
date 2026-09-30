import ChatOSCore
import Foundation

public struct LocalAgentToolInvocationRecord: Decodable, Sendable, Equatable {
    public let invocationID: String
    public let runID: String
    public let batchID: String
    public let callID: String
    public let toolName: String
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

public struct LocalAgentToolClaim: Decodable, Sendable, Equatable {
    public let workerID: String
    public let claimToken: String
    public let invocation: LocalAgentToolInvocationRecord

    private enum CodingKeys: String, CodingKey {
        case workerID = "worker_id"
        case claimToken = "claim_token"
        case invocation
    }
}

public enum LocalAgentToolOutcome: Encodable, Sendable, Equatable {
    case succeeded(LocalAgentJSONValue)
    case failed(error: String, detail: LocalAgentJSONValue)
    case needsReview(reason: String, detail: LocalAgentJSONValue)

    private enum CodingKeys: String, CodingKey { case type, output, error, detail, reason }

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

public struct NativeLocalAgentToolClient: Sendable {
    public static let reservedRustToolNames = [
        "create_task",
        "create_tasks_with_prerequisites",
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

private struct ClaimResult: Decodable, Sendable {
    let type: String
    let claim: LocalAgentToolClaim?
}

private struct CommitResult: Decodable, Sendable {
    let type: String
    let result: LocalAgentToolCommitResult
}
