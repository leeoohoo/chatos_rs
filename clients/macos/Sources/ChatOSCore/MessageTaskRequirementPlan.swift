import Foundation

/// Presentation of the Task's own frozen input; never substitute the project's
/// current proposal when displaying an older Task or a retry.
public struct MessageTaskRequirementPlan: Decodable, Sendable, Equatable {
    public struct Snapshot: Decodable, Sendable, Equatable {
        public let planID: String
        public let revision: UInt64
        public let title: String
        public let markdown: String
        public let confirmedAtUnixMs: Int64
        private enum CodingKeys: String, CodingKey {
            case revision, title, markdown
            case planID = "plan_id", confirmedAtUnixMs = "confirmed_at_unix_ms"
        }
    }
    public let snapshot: Snapshot
    public let markdownSHA256: String
    private enum CodingKeys: String, CodingKey {
        case snapshot
        case markdownSHA256 = "markdown_sha256"
    }

    public static func decode(inputPayloadJSON: String?) -> Self? {
        struct Input: Decodable { let requirement_plan: MessageTaskRequirementPlan? }
        guard let text = inputPayloadJSON, let input = try? JSONDecoder().decode(Input.self, from: Data(text.utf8)),
              let plan = input.requirement_plan, plan.snapshot.revision > 0, plan.markdownSHA256.count == 64 else { return nil }
        return plan
    }
}
