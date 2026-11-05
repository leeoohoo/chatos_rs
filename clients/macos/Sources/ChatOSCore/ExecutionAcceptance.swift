import Foundation

/// Canonical acceptance vocabulary shared by ordinary conversation Tasks and Agent-team Todos.
/// Persisted wire values intentionally match Local Agent Host's task acceptance protocol.
public enum AgentExecutionAcceptanceStatus: String, Codable, Sendable, CaseIterable {
    case pending
    case inProgress = "in_progress"
    case passed
    case failed
    case notVerified = "not_verified"
    case blocked
}

/// Minimal state consumed by the shared terminal gate. Business objects retain their own IDs,
/// criteria, evidence and storage; only completion semantics are shared here.
public struct AgentExecutionAcceptanceGateCheck: Sendable, Equatable {
    public let criterionIndex: Int
    public let status: AgentExecutionAcceptanceStatus
    public let requiresHuman: Bool

    public init(
        criterionIndex: Int,
        status: AgentExecutionAcceptanceStatus,
        requiresHuman: Bool = false
    ) {
        self.criterionIndex = criterionIndex
        self.status = status
        self.requiresHuman = requiresHuman
    }
}

/// One deterministic terminal gate for both execution surfaces.
public enum AgentExecutionAcceptanceGate {
    public static func canComplete(_ checks: [AgentExecutionAcceptanceGateCheck]) -> Bool {
        !checks.isEmpty && checks.allSatisfy { $0.status == .passed }
    }

    public static func canFinishBlocked(_ checks: [AgentExecutionAcceptanceGateCheck]) -> Bool {
        !checks.isEmpty
            && checks.contains { $0.status == .blocked && $0.requiresHuman }
            && actionableChecks(checks).isEmpty
    }

    public static func actionableChecks(
        _ checks: [AgentExecutionAcceptanceGateCheck]
    ) -> [AgentExecutionAcceptanceGateCheck] {
        checks.filter {
            $0.status != .passed && !($0.status == .blocked && $0.requiresHuman)
        }
    }
}
