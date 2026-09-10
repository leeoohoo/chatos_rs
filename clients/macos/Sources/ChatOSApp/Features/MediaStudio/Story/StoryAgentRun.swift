import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

struct StoryAgentRun: Codable, Equatable, Identifiable, Sendable {
    enum Stage: String, Codable, Sendable { case outline, refine }
    struct Receipt: Codable, Equatable, Sendable {
        let name: String
        let arguments: String
        let outcome: AgentToolOutcome
    }
    var version = 1
    var id: UUID { checkpoint.id }
    let owner: String
    let projectID: UUID
    let stage: Stage
    let targetIDs: [String]
    let baseDigest: String
    let cloudMemory: Bool
    let consentAt: Date
    var policy: AgentRunPolicy
    var checkpoint: AgentRunCheckpoint
    var draft: StoryProject
    var readThrough = 0
    var toolReceipts: [String: Receipt] = [:]
    var events: [AgentRunEvent] = []
    var applied = false
    var updatedAt = Date()

    init(project: StoryProject, owner: String, stage: Stage, targetIDs: [String], policy: AgentRunPolicy) throws {
        try project.validate(); try policy.validate()
        guard !project.source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw StoryError.invalidProject }
        if stage == .outline {
            guard project.segments.isEmpty else { throw StoryError.invalidPlan }
        } else {
            guard !targetIDs.isEmpty, Set(targetIDs).count == targetIDs.count,
                  targetIDs.allSatisfy({ id in project.segments.contains { $0.id == id && $0.detail == nil && $0.attempt == nil && $0.video == nil } }) else { throw StoryError.invalidPlan }
        }
        self.owner = owner; self.projectID = project.id; self.stage = stage; self.targetIDs = targetIDs
        self.baseDigest = try Self.digest(project); self.cloudMemory = true; self.consentAt = Date()
        self.policy = policy; self.draft = project
        self.checkpoint = .init(scope: "story:\(owner):\(project.id):\(baseDigest):\(UUID())", messages: [
            .init(role: .system, content: StoryAgentTools.systemPrompt),
            .init(role: .user, content: "当前阶段：\(stage.rawValue)。剧情长度 \(project.source.count) 个字符。先读取状态和原文工具，按阶段逐步保存计划，最后调用 story_finish。不要生成图片或视频。"),
        ])
    }
    var canResume: Bool { !applied && checkpoint.status != .completed }
    static func digest(_ project: StoryProject) throws -> String {
        var value = project; value.updatedAt = .distantPast
        let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
        return SHA256.hash(data: try encoder.encode(value)).map { String(format: "%02x", $0) }.joined()
    }
    func validate(owner: String, projectID: UUID) throws {
        guard version == 1, self.owner == owner, self.projectID == projectID, draft.id == projectID,
              readThrough >= 0, readThrough <= draft.source.count, events.count <= 20_000 else { throw StoryAgentError.invalidRun }
        try draft.validate(); try policy.validate()
    }
}

enum StoryAgentError: LocalizedError {
    case invalidRun, projectChanged, incompletePlan, unavailable, wrongStage, forbiddenTarget
    var errorDescription: String? {
        switch self {
        case .invalidRun: "剧情运行记录无效，原文件已保留。"
        case .projectChanged: "项目已被修改，不能覆盖或恢复旧规划；请保留原草稿并启动新的规划。"
        case .incompletePlan: "计划尚未完成：必须读完原文、连续覆盖全剧，并保存本阶段所有目标。"
        case .unavailable: "当前剧情服务没有接入公共 Agent 循环。"
        case .wrongStage: "此工具不属于当前规划阶段。"
        case .forbiddenTarget: "目标不在本次授权范围内，或已经存在不可覆盖的结果。"
        }
    }
}
