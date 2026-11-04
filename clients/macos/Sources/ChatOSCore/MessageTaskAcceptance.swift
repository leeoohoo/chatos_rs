import Foundation

/// Presentation only: never changes scheduling, dependency gates, or Task status.
public enum MessageTaskResultDisposition: String, Sendable, Equatable {
    case executionFailed, blocked, evidenceIncomplete, acceptanceFailed, acceptancePassed
}

/// One presentation truth for Task graph nodes, details, Run inspectors and
/// callback UI. Scheduling continues to use the durable Task lifecycle and the
/// internal result gate independently.
public enum MessageTaskPresentationStatus: String, Sendable, Equatable, Hashable {
    case waiting
    case running
    case completed
    case executionFailed = "execution_failed"
    case resultBlocked = "result_blocked"
    case dependencyBlocked = "dependency_blocked"
    case blocked
    case evidenceIncomplete = "evidence_incomplete"
    case acceptanceFailed = "acceptance_failed"
    case acceptancePassed = "acceptance_passed"
    case cancelled
    case reviewingFollowup = "reviewing_followup"
    case waitingConfirmation = "waiting_confirmation"
    case unknown
}

public struct MessageTaskAcceptanceReport: Decodable, Sendable, Equatable {
    public struct Check: Decodable, Sendable, Equatable {
        public let criterionIndex: Int
        public let status: AgentExecutionAcceptanceStatus
        public let evidence: String

        public init(
            criterionIndex: Int,
            status: AgentExecutionAcceptanceStatus,
            evidence: String
        ) {
            self.criterionIndex = criterionIndex
            self.status = status
            self.evidence = evidence
        }

        enum CodingKeys: String, CodingKey {
            case criterionIndex = "criterion_index", status, evidence
        }

        /// Ordinary Tasks do not yet persist a separate `requires_human` bit. A blocked
        /// Task acceptance verdict therefore keeps its existing protocol meaning: external
        /// intervention is required. The shared gate can consume it without changing storage.
        public var gateCheck: AgentExecutionAcceptanceGateCheck {
            .init(
                criterionIndex: criterionIndex,
                status: status,
                requiresHuman: status == .blocked
            )
        }
    }
    public let resultStatus: String
    public let reason: String
    public let checks: [Check]
    enum CodingKeys: String, CodingKey {
        case resultStatus = "result_status", status, reason, checks = "acceptance_checks"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        resultStatus = try container.decodeIfPresent(String.self, forKey: .resultStatus)
            ?? container.decode(String.self, forKey: .status)
        reason = try container.decode(String.self, forKey: .reason)
        checks = try container.decodeIfPresent([Check].self, forKey: .checks) ?? []
    }

    /// Compatibility name for older call sites and persisted payloads. This is
    /// the internal result/dependency gate, never execution status.
    public var status: String { resultStatus }

    public static func decode(json: String?) -> Self? {
        guard let data = json?.data(using: .utf8),
              let report = try? JSONDecoder().decode(Self.self, from: data),
              ["succeeded", "failed", "blocked"].contains(report.resultStatus),
              !report.reason.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              report.resultStatus != "succeeded" || !report.checks.isEmpty,
              Set(report.checks.map(\.criterionIndex)) == Set(0..<report.checks.count),
              report.checks.allSatisfy({ [.passed, .failed, .notVerified, .blocked].contains($0.status)
                  && !$0.evidence.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty })
        else { return nil }
        return report
    }

    public var disposition: MessageTaskResultDisposition {
        if resultStatus == "blocked" || checks.contains(where: { $0.status == .blocked }) {
            return .blocked
        }
        if checks.contains(where: { $0.status == .failed }) { return .acceptanceFailed }
        if checks.contains(where: { $0.status == .notVerified }) { return .evidenceIncomplete }
        if resultStatus == "failed" { return .acceptanceFailed }
        return .acceptancePassed
    }

    public var gateChecks: [AgentExecutionAcceptanceGateCheck] {
        checks.map(\.gateCheck)
    }

    public var acceptanceCanComplete: Bool {
        resultStatus == "succeeded" && AgentExecutionAcceptanceGate.canComplete(gateChecks)
    }

    public var acceptanceCanFinishBlocked: Bool {
        resultStatus == "blocked" && AgentExecutionAcceptanceGate.canFinishBlocked(gateChecks)
    }

    public var actionableChecks: [Check] {
        let actionableIndices = Set(
            AgentExecutionAcceptanceGate.actionableChecks(gateChecks).map(\.criterionIndex)
        )
        return checks.filter { actionableIndices.contains($0.criterionIndex) }
    }
}

public extension MessageTask {
    var resultDisposition: MessageTaskResultDisposition? {
        let runtimeStatus = lastRun?.status?.lowercased()
        if ["failed", "error"].contains(runtimeStatus ?? "") { return .executionFailed }
        guard runtimeStatus == "succeeded" else { return nil }
        return lastRun?.acceptanceReport?.disposition
    }

    var dependencyBlockingPrerequisites: [MessageTaskReference] {
        prerequisiteTasks.filter(\.blocksDependentExecution)
    }

    var wasPreventedFromStartingByPrerequisite: Bool {
        normalizedStatus == "blocked"
            && lastRunID == nil
            && !prerequisiteTaskIDs.isEmpty
    }

    var presentationStatus: MessageTaskPresentationStatus {
        if let disposition = resultDisposition {
            return switch disposition {
            case .executionFailed: .executionFailed
            case .blocked: .resultBlocked
            case .evidenceIncomplete: .evidenceIncomplete
            case .acceptanceFailed: .acceptanceFailed
            case .acceptancePassed: .acceptancePassed
            }
        }
        if wasPreventedFromStartingByPrerequisite { return .dependencyBlocked }
        return MessageTaskPresentationStatus(taskStatus: normalizedStatus)
    }
}

public extension MessageTaskReference {
    var blocksDependentExecution: Bool {
        let lifecycle = status?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if ["failed", "error", "execution_failed", "blocked", "cancelled", "canceled", "stopped"]
            .contains(lifecycle ?? "") { return true }
        guard let result = resultStatus?
            .trimmingCharacters(in: .whitespacesAndNewlines).lowercased(),
              !result.isEmpty else { return false }
        return result != "succeeded"
    }
}

public extension MessageTaskPresentationStatus {
    init(taskStatus: String?) {
        switch taskStatus?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "pending", "ready", "queued": self = .waiting
        case "running", "processing", "in_progress", "doing", "executing": self = .running
        case "completed", "succeeded", "success", "done": self = .completed
        case "failed", "error", "execution_failed": self = .executionFailed
        case "result_blocked": self = .resultBlocked
        case "dependency_blocked": self = .dependencyBlocked
        case "blocked": self = .blocked
        case "evidence_incomplete": self = .evidenceIncomplete
        case "acceptance_failed": self = .acceptanceFailed
        case "acceptance_passed": self = .acceptancePassed
        case "cancelled", "canceled", "stopped": self = .cancelled
        case "reviewing_followup": self = .reviewingFollowup
        case "waiting_confirmation": self = .waitingConfirmation
        default: self = .unknown
        }
    }
}
