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
        checks = try container.decode([Check].self, forKey: .checks)
    }

    /// Compatibility name for older call sites and persisted payloads. This is
    /// the internal result/dependency gate, never execution status.
    public var status: String { resultStatus }

    public static func decode(json: String?) -> Self? {
        guard let data = json?.data(using: .utf8),
              let report = try? JSONDecoder().decode(Self.self, from: data),
              ["succeeded", "failed", "blocked"].contains(report.resultStatus),
              !report.checks.isEmpty,
              Set(report.checks.map(\.criterionIndex)) == Set(0..<report.checks.count),
              report.checks.allSatisfy({ ["passed", "failed", "not_verified"].contains($0.status)
                  && !$0.evidence.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty })
        else { return nil }
        return report
    }

    public var disposition: MessageTaskResultDisposition {
        if resultStatus == "blocked" { return .blocked }
        if checks.contains(where: { $0.status == "failed" }) { return .acceptanceFailed }
        if checks.contains(where: { $0.status == "not_verified" }) { return .evidenceIncomplete }
        if resultStatus == "failed" { return .acceptanceFailed }
        return .acceptancePassed
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
