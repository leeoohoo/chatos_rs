import ChatOSCore
import Foundation

/// Deterministic production queue, never a model-controlled tool loop.
struct StoryMediaBatch: Codable, Equatable, Identifiable, Sendable {
    enum Kind: String, Codable, CaseIterable, Sendable { case assets, frames, lastFrames, videos, pipeline }
    enum Status: String, Codable, Sendable { case ready, running, paused, needsReview, completed, abandoned }
    struct Step: Codable, Equatable, Identifiable, Sendable {
        let kind: Kind
        let targetID: String
        var id: String { "\(kind.rawValue):\(targetID)" }
    }
    struct Job: Codable, Equatable, Sendable {
        var intentID = UUID()
        var jobID: String?
        var completed = false
    }
    struct Event: Codable, Equatable, Identifiable, Sendable {
        var id = UUID()
        var date = Date()
        let detail: String
    }
    let id: UUID
    let owner: String
    let kind: Kind
    let steps: [Step]
    let models: [MediaGenerationModel]
    let consentAt: Date
    var status: Status = .ready
    var draft: StoryProject
    var expectedProjectDigest: String
    var jobs: [String: Job] = [:]
    var events: [Event] = []
    var error: String?
    var updatedAt = Date()
    var finished: Bool { status == .completed || status == .abandoned }
    var completedCount: Int { jobs.values.filter(\.completed).count }

    init(project: StoryProject, owner: String, kind: Kind, targets: [String], models: [MediaGenerationModel]) throws {
        try project.validate()
        guard !targets.isEmpty, Set(targets).count == targets.count else { throw StoryAgentError.forbiddenTarget }
        var steps: [Step] = []
        if kind == .pipeline {
            let segments = try targets.map { id in
                guard let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil,
                      segment.video == nil, segment.attempt == nil, segment.imageGenerationAttemptID == nil,
                      segment.lastFrameGenerationAttemptID == nil else { throw StoryError.invalidPlan }
                return segment
            }
            let refs = Set(segments.flatMap(\.resourceIDs))
            for asset in project.resources where refs.contains(asset.id) {
                guard asset.imageGenerationAttemptID == nil else { throw StoryError.unresolvedSubmission }
                if asset.confirmedImage == nil {
                    guard asset.images.isEmpty else { throw StoryBatchError.unconfirmedVersions }
                    steps.append(.init(kind: .assets, targetID: asset.id))
                }
            }
            // Keep each segment's first/tail pair adjacent. Once a tail is generated and
            // confirmed, the following segment can consume it as its continuity reference.
            for segment in segments {
                if segment.firstFrame == nil {
                    guard segment.firstFrames.images.isEmpty else { throw StoryBatchError.unconfirmedVersions }
                    steps.append(.init(kind: .frames, targetID: segment.id))
                }
                if segment.lastFrame == nil {
                    guard segment.lastFrames.images.isEmpty else { throw StoryBatchError.unconfirmedVersions }
                    steps.append(.init(kind: .lastFrames, targetID: segment.id))
                }
            }
            steps += segments.map { .init(kind: .videos, targetID: $0.id) }
        } else {
            for id in targets {
                switch kind {
                case .assets:
                    guard let asset = project.resource(id: id), asset.imageGenerationAttemptID == nil else { throw StoryError.unresolvedSubmission }
                case .frames:
                    guard let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil,
                          segment.attempt == nil, segment.video == nil, segment.imageGenerationAttemptID == nil,
                          segment.resourceIDs.allSatisfy({ project.resource(id: $0)?.confirmedImage != nil }) else { throw StoryError.invalidPlan }
                case .lastFrames:
                    guard let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil,
                          segment.attempt == nil, segment.video == nil, segment.lastFrameGenerationAttemptID == nil,
                          segment.resourceIDs.allSatisfy({ project.resource(id: $0)?.confirmedImage != nil }) else { throw StoryError.invalidPlan }
                case .videos:
                    guard project.segments.first(where: { $0.id == id })?.isReady == true else { throw StoryError.invalidPlan }
                case .pipeline: break
                }
                steps.append(.init(kind: kind, targetID: id))
            }
        }
        for type in Set(steps.map(\.kind)) {
            let id = type == .videos ? project.models.videoModelID : project.models.imageModelID
            guard let model = models.first(where: { $0.id == id }), model.enabled, model.hasAPIKey else { throw StoryError.missingModel }
            if type == .videos {
                let durations = VideoGenerationProfile(modelName: model.modelName).durations
                guard steps.filter({ $0.kind == .videos }).allSatisfy({ step in
                    project.segments.first(where: { $0.id == step.targetID }).map { durations.contains($0.seconds) } == true
                }) else { throw StoryError.unsupportedDuration }
            }
        }
        self.id = UUID(); self.owner = owner; self.kind = kind; self.steps = steps; self.models = models
        self.consentAt = Date(); self.draft = project; self.expectedProjectDigest = try StoryAgentRun.digest(project)
    }
    static func candidates(_ project: StoryProject, kind: Kind) -> [String] {
        switch kind {
        case .assets: project.resources.filter { $0.images.isEmpty && $0.imageGenerationAttemptID == nil }.map(\.id)
        case .frames: project.segments.filter { s in s.detail != nil && s.attempt == nil && s.video == nil && s.firstFrames.images.isEmpty && s.imageGenerationAttemptID == nil && s.resourceIDs.allSatisfy { project.resource(id: $0)?.confirmedImage != nil } }.map(\.id)
        case .lastFrames: project.segments.filter { s in s.detail != nil && s.attempt == nil && s.video == nil && s.lastFrames.images.isEmpty && s.lastFrameGenerationAttemptID == nil && s.resourceIDs.allSatisfy { project.resource(id: $0)?.confirmedImage != nil } }.map(\.id)
        case .videos: project.segments.filter(\.isReady).map(\.id)
        case .pipeline: project.segments.filter { $0.detail != nil && $0.attempt == nil && $0.video == nil && $0.imageGenerationAttemptID == nil && $0.lastFrameGenerationAttemptID == nil }.map(\.id)
        }
    }
    func validate(owner: String, projectID: UUID) throws {
        guard self.owner == owner, draft.id == projectID, !steps.isEmpty, steps.count <= 500,
              Set(steps.map(\.id)).count == steps.count, steps.allSatisfy({ $0.kind != .pipeline }),
              jobs.keys.allSatisfy(Set(steps.map(\.id)).contains), events.count <= 20_000 else { throw StoryAgentError.invalidRun }
        try draft.validate()
    }
}

enum StoryBatchError: LocalizedError {
    case unconfirmedVersions, activeBatch
    var errorDescription: String? {
        switch self {
        case .unconfirmedVersions: "已有图片尚未确认版本。请先确认已有素材或首帧；一键执行仅自动采用本次新生成的图片，不替你选择已有版本。"
        case .activeBatch: "还有未结束的制作批次。请先恢复，或核对原任务后结束旧批次，避免重复生成。"
        }
    }
}

extension StoryProjectStore {
    func loadMediaBatches(owner: String, projectID: UUID) throws -> (batches: [StoryMediaBatch], unreadable: Int) {
        let folder = try fileURL("project.json", projectID: projectID, owner: owner).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: folder.path) else { return ([], 0) }
        var batches: [StoryMediaBatch] = []; var unreadable = 0
        for url in try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.fileSizeKey]) where url.lastPathComponent.hasPrefix("media-batch-") && url.pathExtension == "json" {
            do {
                guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 32 * 1024 * 1024 else { throw StoryAgentError.invalidRun }
                let batch = try JSONDecoder().decode(StoryMediaBatch.self, from: Data(contentsOf: url))
                guard url.lastPathComponent == "media-batch-\(batch.id).json" else { throw StoryAgentError.invalidRun }
                try batch.validate(owner: owner, projectID: projectID); batches.append(batch)
            } catch { unreadable += 1 }
        }
        return (batches.sorted { $0.updatedAt > $1.updatedAt }, unreadable)
    }
    /// Write-ahead batch first, canonical project second. Repeating this on restart completes
    /// an interrupted second write, but never overwrites a different manual project edit.
    func commitMediaBatch(_ batch: StoryMediaBatch) throws {
        try batch.validate(owner: batch.owner, projectID: batch.draft.id)
        let manifest = try fileURL("project.json", projectID: batch.draft.id, owner: batch.owner)
        let current = try JSONDecoder().decode(StoryProject.self, from: Data(contentsOf: manifest))
        let digest = try StoryAgentRun.digest(current)
        let draftDigest = try StoryAgentRun.digest(batch.draft)
        guard digest == batch.expectedProjectDigest || digest == draftDigest else { throw StoryAgentError.projectChanged }
        let data = try JSONEncoder().encode(batch)
        guard data.count <= 32 * 1024 * 1024 else { throw StoryAgentError.invalidRun }
        try data.write(to: try fileURL("media-batch-\(batch.id).json", projectID: batch.draft.id, owner: batch.owner), options: .atomic)
        if digest != draftDigest { try save(batch.draft, owner: batch.owner) }
    }
}
