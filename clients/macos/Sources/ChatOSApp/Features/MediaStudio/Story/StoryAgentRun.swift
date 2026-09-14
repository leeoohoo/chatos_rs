import ChatOSCore
import CryptoKit
import Foundation

/// Native presentation of one durable Rust `story_design` Run.
/// The model contains no checkpoint, tool loop, model client, or retry policy.
struct StoryAgentRun: Equatable, Identifiable, Sendable {
    enum Stage: String, Codable, Sendable { case outline, refine }

    let id: UUID
    let runID: String
    let storyRecordID: String
    let owner: String
    let projectID: UUID
    let stage: Stage
    let targetIDs: [String]
    let baseProjectRevision: UInt64
    let baseDigest: String
    var draft: StoryProject
    var status: LocalAgentRunStatus
    var runVersion: UInt64
    var modelCalls: UInt32
    var retryCount: UInt32
    var storyRevision: UInt64
    var applied: Bool
    var updatedAt: Date

    var isTerminal: Bool {
        status == .succeeded || status == .failed || status == .cancelled
    }

    var canResume: Bool {
        !applied && (status == .paused || status == .needsReview)
    }

    var canApply: Bool { !applied && status == .succeeded }
    var isExecuting: Bool { !isTerminal && status != .paused && status != .needsReview }

    init(run: LocalAgentRunSnapshot, record: LocalAgentStorySnapshot) throws {
        let durable: DurableStoryDesignRecord = try StoryProjectStore.decodeState(record.draft.state)
        guard let id = UUID(uuidString: run.runID),
              let projectID = UUID(uuidString: durable.design.projectID),
              run.profileKey == "story_design",
              run.ownerEntityType == "story_design",
              run.ownerEntityID == record.recordID,
              run.ownerUserID == record.ownerUserID,
              run.projectID?.lowercased() == durable.design.projectID.lowercased(),
              record.draft.kind == .agentRun,
              record.draft.projectID.lowercased() == durable.design.projectID.lowercased(),
              record.recordID == durable.design.storyRecordID,
              durable.schemaVersion == 1,
              durable.design.schemaVersion == 1,
              durable.design.draft.id == projectID
        else { throw StoryAgentError.invalidRun }

        try durable.design.draft.validate()
        self.id = id
        self.runID = run.runID
        self.storyRecordID = record.recordID
        self.owner = run.ownerUserID
        self.projectID = projectID
        self.stage = durable.design.stage
        self.targetIDs = durable.design.targetIDs
        self.baseProjectRevision = durable.design.baseProjectRevision
        self.baseDigest = durable.design.baseProjectDigest
        self.draft = durable.design.draft
        self.status = run.status
        self.runVersion = run.version
        self.modelCalls = run.iteration
        self.retryCount = run.retryCount
        self.storyRevision = record.revision
        self.applied = durable.appliedProjectRevision != nil && durable.appliedAt != nil
        self.updatedAt = max(Self.date(run.updatedAt), Self.date(record.updatedAt))
    }

    /// Used only for comparing two in-memory editor copies. The authoritative
    /// storage CAS uses the Rust canonical digest frozen in `baseDigest`.
    static func digest(_ project: StoryProject) throws -> String {
        var value = project
        value.updatedAt = .distantPast
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return SHA256.hash(data: try encoder.encode(value))
            .map { String(format: "%02x", $0) }
            .joined()
    }

    private static func date(_ value: String) -> Date {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value) ?? .distantPast
    }
}

private struct DurableStoryDesignRecord: Codable {
    let schemaVersion: UInt32
    let design: DurableStoryDesignState
    let appliedProjectRevision: UInt64?
    let appliedAt: String?

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case design
        case appliedProjectRevision = "applied_project_revision"
        case appliedAt = "applied_at"
    }
}

private struct DurableStoryDesignState: Codable {
    let schemaVersion: UInt32
    let storyRecordID: String
    let projectID: String
    let baseProjectRevision: UInt64
    let baseProjectDigest: String
    let stage: StoryAgentRun.Stage
    let targetIDs: [String]
    let draft: StoryProject

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case storyRecordID = "story_record_id"
        case projectID = "project_id"
        case baseProjectRevision = "base_project_revision"
        case baseProjectDigest = "base_project_digest"
        case stage
        case targetIDs = "target_ids"
        case draft
    }
}

enum StoryAgentError: LocalizedError {
    case invalidRun
    case projectChanged
    case incompletePlan
    case unavailable
    case wrongStage
    case forbiddenTarget

    var errorDescription: String? {
        switch self {
        case .invalidRun: "剧情运行记录无效。"
        case .projectChanged: "项目已被修改，不能覆盖这份规划草稿；请启动新的规划。"
        case .incompletePlan: "规划尚未完成。"
        case .unavailable: "本地 Agent Host 当前不可用。"
        case .wrongStage: "此操作不属于当前规划阶段。"
        case .forbiddenTarget: "目标不在本次授权范围内。"
        }
    }
}
