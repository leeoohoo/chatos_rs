import Foundation

/// Presentation only: never changes scheduling, dependency gates, or Task status.
public enum MessageTaskResultDisposition: String, Sendable, Equatable {
    case executionFailed, blocked, evidenceIncomplete, acceptanceFailed, acceptancePassed
}

public struct MessageTaskAcceptanceReport: Decodable, Sendable, Equatable {
    public struct Check: Decodable, Sendable, Equatable {
        public let criterionIndex: Int
        public let status: String
        public let evidence: String
        enum CodingKeys: String, CodingKey {
            case criterionIndex = "criterion_index", status, evidence
        }
    }
    public let status: String
    public let reason: String
    public let checks: [Check]
    enum CodingKeys: String, CodingKey {
        case status, reason, checks = "acceptance_checks"
    }

    public static func decode(json: String?) -> Self? {
        guard let data = json?.data(using: .utf8),
              let report = try? JSONDecoder().decode(Self.self, from: data),
              ["succeeded", "failed", "blocked"].contains(report.status),
              !report.checks.isEmpty,
              Set(report.checks.map(\.criterionIndex)) == Set(0..<report.checks.count),
              report.checks.allSatisfy({ ["passed", "failed", "not_verified"].contains($0.status)
                  && !$0.evidence.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }),
              report.status != "succeeded" || report.checks.allSatisfy({ $0.status == "passed" })
        else { return nil }
        return report
    }

    public var disposition: MessageTaskResultDisposition {
        if status == "blocked" { return .blocked }
        if status == "succeeded" { return .acceptancePassed }
        if checks.contains(where: { $0.status == "failed" }) { return .acceptanceFailed }
        if checks.contains(where: { $0.status == "not_verified" }) { return .evidenceIncomplete }
        return .acceptanceFailed
    }
}

public extension MessageTask {
    var resultDisposition: MessageTaskResultDisposition? {
        let runtimeStatus = lastRun?.status?.lowercased()
        if ["failed", "error"].contains(runtimeStatus ?? "") { return .executionFailed }
        guard runtimeStatus == "succeeded" else { return nil }
        return lastRun?.acceptanceReport?.disposition
    }
}
