import ChatOSCore
import CryptoKit
import Foundation

actor StoryProjectStore {
    struct Snapshot: Sendable { var projects: [StoryProject]; var unreadableCount: Int }
    struct AssetImageGenerationIntent: Sendable {
        var project: StoryProject
        var resource: StoryResource
        var attemptID: UUID
    }
    struct FrameImageGenerationIntent: Sendable {
        var project: StoryProject
        var segment: StorySegment
        var role: StoryFrameRole
        var attemptID: UUID
    }
    struct VideoGenerationIntent: Sendable {
        var project: StoryProject
        var segment: StorySegment
        var attempt: StoryVideoAttempt
    }
    nonisolated let root: URL
    init(root: URL? = nil) {
        self.root = (root ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ChatOSSwift/StoryStudio", isDirectory: true)).resolvingSymlinksInPath()
    }

    func load(owner: String) throws -> Snapshot {
        let directory = accountDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return .init(projects: [], unreadableCount: 0) }
        var projects: [StoryProject] = []
        var unreadable = 0
        for folder in try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]) {
            guard UUID(uuidString: folder.lastPathComponent) != nil else { continue }
            let manifest = folder.appendingPathComponent("project.json")
            guard FileManager.default.fileExists(atPath: manifest.path) else { continue }
            do {
                guard (try manifest.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 16 * 1024 * 1024 else { throw StoryError.invalidProject }
                var project = try JSONDecoder().decode(StoryProject.self, from: Data(contentsOf: manifest))
                guard project.id.uuidString == folder.lastPathComponent else { throw StoryError.invalidProject }
                let repairedContinuity = StoryContinuityContext.reconcileInheritedFirstFrames(&project)
                try project.validate()
                // Older builds persisted the intent before MiniMax prompt validation. This
                // exact error is raised before the provider POST, so the apparent remote task
                // cannot exist and its false local lock is safe to release during migration.
                var repairedLegacyPreflightFailure = false
                for index in project.segments.indices
                    where project.segments[index].attempt?.jobID == nil
                        && project.segments[index].error == "MiniMax 视频提示词不能为空，且不能超过 7000 字符。" {
                    project.segments[index].attempt = nil
                    repairedLegacyPreflightFailure = true
                }
                if repairedLegacyPreflightFailure || repairedContinuity {
                    project.updatedAt = Date()
                    try save(project, owner: owner)
                }
                projects.append(project)
            } catch { unreadable += 1 } // Never replace or delete an unreadable manifest.
        }
        return .init(projects: projects.sorted { $0.updatedAt > $1.updatedAt }, unreadableCount: unreadable)
    }

    func save(_ input: StoryProject, owner: String) throws {
        var project = input
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        try project.validate()
        let folder = directory(owner: owner, projectID: project.id)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try JSONEncoder().encode(project).write(to: folder.appendingPathComponent("project.json"), options: .atomic)
    }

    /// Saves an ordinary project edit while retaining provider-owned state for video jobs
    /// that are currently running. This prevents an unrelated edit from erasing a newly
    /// returned job ID or a video that completed in the opposite order.
    func save(_ input: StoryProject, owner: String,
              preservingVideoSegmentIDs: Set<String>) throws -> StoryProject {
        guard !preservingVideoSegmentIDs.isEmpty else {
            var project = input
            StoryContinuityContext.reconcileInheritedFirstFrames(&project)
            try save(project, owner: owner)
            return project
        }
        let current = try loadProject(projectID: input.id, owner: owner)
        var project = input
        for segmentID in preservingVideoSegmentIDs {
            guard let currentSegment = current.segments.first(where: { $0.id == segmentID }),
                  let index = project.segments.firstIndex(where: { $0.id == segmentID }) else { continue }
            project.segments[index].attempt = currentSegment.attempt
            project.segments[index].video = currentSegment.video
            project.segments[index].error = currentSegment.error
        }
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        try save(project, owner: owner)
        return project
    }

    /// Creates one durable, ID-addressed intent from the latest manifest. The store actor
    /// serializes this tiny transaction while the expensive provider calls remain concurrent.
    func beginAssetImageGeneration(projectID: UUID, resourceID: String, attemptID: UUID,
                                   owner: String) throws -> AssetImageGenerationIntent {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard var resource = project.resource(id: resourceID) else { throw StoryError.invalidPlan }
        guard resource.media.generationAttemptID == nil else { throw StoryError.unresolvedSubmission }
        resource.media.generationAttemptID = attemptID
        try project.replaceResource(resource)
        project.updatedAt = Date()
        try save(project, owner: owner)
        return .init(project: project, resource: resource, attemptID: attemptID)
    }

    /// Merges a completed image into the latest manifest instead of replacing the whole
    /// project snapshot. A late or mismatched task can therefore never write into another
    /// resource, and reverse-order completions preserve every result.
    func completeAssetImageGeneration(_ data: Data, mimeType: String, projectID: UUID,
                                      resourceID: String, attemptID: UUID,
                                      providerResultID: String?, providerAssetID: String?,
                                      owner: String) throws -> (StoryProject, StoryImage) {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard var resource = project.resource(id: resourceID),
              resource.media.generationAttemptID == attemptID else {
            throw StoryError.unresolvedSubmission
        }
        let image = try saveImage(data, mimeType: mimeType, projectID: projectID, owner: owner,
                                  sourceResourceID: resourceID, generationAttemptID: attemptID,
                                  providerResultID: providerResultID, providerAssetID: providerAssetID)
        resource.media.images.append(image)
        resource.media.generationAttemptID = nil
        let automaticallyConfirmed = resource.media.confirmedImageID == nil
        if automaticallyConfirmed { resource.media.confirmedImageID = image.id }
        try project.replaceResource(resource)
        if automaticallyConfirmed {
            for index in project.segments.indices
                where project.segments[index].resourceIDs.contains(resourceID)
                    && project.segments[index].attempt == nil {
                project.segments[index].confirmedFrameID = nil
                project.segments[index].confirmedLastFrameID = nil
            }
        }
        project.updatedAt = Date()
        try save(project, owner: owner)
        return (project, image)
    }

    func beginFrameImageGeneration(projectID: UUID, segmentID: String, role: StoryFrameRole,
                                   attemptID: UUID, owner: String) throws -> FrameImageGenerationIntent {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt == nil else { throw StoryError.invalidPlan }
        switch role {
        case .first:
            guard project.segments[index].firstFrames.generationAttemptID == nil else { throw StoryError.unresolvedSubmission }
            project.segments[index].firstFrames.generationAttemptID = attemptID
        case .last:
            guard project.segments[index].lastFrames.generationAttemptID == nil else { throw StoryError.unresolvedSubmission }
            project.segments[index].lastFrames.generationAttemptID = attemptID
        }
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        project.updatedAt = Date()
        try save(project, owner: owner)
        return .init(project: project, segment: project.segments[index], role: role, attemptID: attemptID)
    }

    func completeFrameImageGeneration(_ data: Data, mimeType: String, projectID: UUID,
                                      segmentID: String, role: StoryFrameRole, attemptID: UUID,
                                      providerResultID: String?, providerAssetID: String?,
                                      owner: String) throws -> (StoryProject, StoryImage) {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else { throw StoryError.invalidPlan }
        let matchesAttempt = role == .first
            ? project.segments[index].firstFrames.generationAttemptID == attemptID
            : project.segments[index].lastFrames.generationAttemptID == attemptID
        guard matchesAttempt else { throw StoryError.unresolvedSubmission }
        let image = try saveImage(data, mimeType: mimeType, projectID: projectID, owner: owner,
                                  sourceResourceID: segmentID, generationAttemptID: attemptID,
                                  providerResultID: providerResultID, providerAssetID: providerAssetID)
        switch role {
        case .first:
            project.segments[index].firstFrames.images.append(image)
            project.segments[index].firstFrames.generationAttemptID = nil
            if project.segments[index].firstFrames.confirmedImageID == nil {
                project.segments[index].firstFrames.confirmedImageID = image.id
                project.segments[index].inheritedFirstFrameSourceSegmentID = nil
            }
        case .last:
            project.segments[index].lastFrames.images.append(image)
            project.segments[index].lastFrames.generationAttemptID = nil
            if project.segments[index].lastFrames.confirmedImageID == nil {
                project.segments[index].lastFrames.confirmedImageID = image.id
            }
        }
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        project.updatedAt = Date()
        try save(project, owner: owner)
        return (project, image)
    }

    /// Persists the per-segment video intent against the latest project manifest. Other
    /// segments may begin or finish concurrently without replacing this segment's state.
    func beginVideoGeneration(projectID: UUID, segmentID: String, attempt: StoryVideoAttempt,
                              owner: String) throws -> VideoGenerationIntent {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].isReady else { throw StoryError.invalidPlan }
        project.segments[index].attempt = attempt
        project.segments[index].error = nil
        project.updatedAt = Date()
        try save(project, owner: owner)
        return .init(project: project, segment: project.segments[index], attempt: attempt)
    }

    func updateVideoGenerationProgress(_ progress: VideoGenerationProgress, projectID: UUID,
                                       segmentID: String, attemptID: UUID,
                                       owner: String) throws -> StoryProject {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt?.id == attemptID else {
            throw StoryError.unresolvedSubmission
        }
        if let jobID = progress.jobID {
            let existing = project.segments[index].attempt?.jobID
            guard existing == nil || existing == jobID else { throw StoryError.invalidPlan }
            project.segments[index].attempt?.jobID = jobID
        }
        project.segments[index].attempt?.status = progress.status
        project.updatedAt = Date()
        try save(project, owner: owner)
        return project
    }

    func completeVideoGeneration(_ result: VideoGenerationResult, projectID: UUID,
                                 segmentID: String, attemptID: UUID,
                                 owner: String) throws -> (StoryProject, StoryVideo) {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt?.id == attemptID else {
            throw StoryError.unresolvedSubmission
        }
        let video = try saveVideo(result, projectID: projectID, owner: owner)
        project.segments[index].video = video
        project.segments[index].attempt?.status = "completed"
        project.segments[index].error = nil
        project.updatedAt = Date()
        try save(project, owner: owner)
        return (project, video)
    }

    func failVideoGeneration(_ error: String, projectID: UUID, segmentID: String,
                             attemptID: UUID, owner: String) throws -> StoryProject {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt?.id == attemptID else {
            throw StoryError.unresolvedSubmission
        }
        project.segments[index].error = error
        project.updatedAt = Date()
        try save(project, owner: owner)
        return project
    }

    /// A deterministic failure before the provider POST must release the local intent. Keeping
    /// it would falsely ask the user to recover a task ID that can never exist.
    func failVideoGenerationBeforeSubmission(_ error: String, projectID: UUID, segmentID: String,
                                             attemptID: UUID, owner: String) throws -> StoryProject {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else {
            throw StoryError.invalidPlan
        }
        if let attempt = project.segments[index].attempt {
            guard attempt.id == attemptID, attempt.jobID == nil else {
                throw StoryError.unresolvedSubmission
            }
            project.segments[index].attempt = nil
        }
        project.segments[index].error = error
        project.updatedAt = Date()
        try save(project, owner: owner)
        return project
    }

    func saveRun(_ run: StoryAgentRun, owner: String) throws {
        try run.validate(owner: owner, projectID: run.projectID)
        let folder = directory(owner: owner, projectID: run.projectID).appendingPathComponent("runs", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(run)
        guard data.count <= 64 * 1024 * 1024 else { throw StoryAgentError.invalidRun }
        try data.write(to: folder.appendingPathComponent("\(run.id).json"), options: .atomic)
    }

    func loadRuns(owner: String, projectID: UUID) throws -> (runs: [StoryAgentRun], unreadable: Int) {
        let folder = directory(owner: owner, projectID: projectID).appendingPathComponent("runs", isDirectory: true)
        guard FileManager.default.fileExists(atPath: folder.path) else { return ([], 0) }
        var runs: [StoryAgentRun] = []; var unreadable = 0
        for url in try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.fileSizeKey]) {
            guard url.pathExtension == "json", let id = UUID(uuidString: url.deletingPathExtension().lastPathComponent) else { continue }
            do {
                guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 64 * 1024 * 1024 else { throw StoryAgentError.invalidRun }
                let run = try JSONDecoder().decode(StoryAgentRun.self, from: Data(contentsOf: url))
                guard run.id == id else { throw StoryAgentError.invalidRun }
                try run.validate(owner: owner, projectID: projectID)
                runs.append(run)
            } catch { unreadable += 1 }
        }
        return (runs.sorted { $0.updatedAt > $1.updatedAt }, unreadable)
    }

    /// Canonical project replacement is guarded by the original digest. A crash after project save
    /// but before the applied marker is recoverable by comparing with the completed draft digest.
    func applyRun(_ input: StoryAgentRun, owner: String) throws -> (StoryAgentRun, StoryProject) {
        var run = input
        try run.validate(owner: owner, projectID: run.projectID)
        guard run.abandonedAt == nil, run.checkpoint.status == .completed else { throw StoryAgentError.incompletePlan }
        try StoryAgentTools.validateCompletion(run)
        let url = directory(owner: owner, projectID: run.projectID).appendingPathComponent("project.json")
        let current = try JSONDecoder().decode(StoryProject.self, from: Data(contentsOf: url))
        let digest = try StoryAgentRun.digest(current)
        let draftDigest = try StoryAgentRun.digest(run.draft)
        guard digest == run.baseDigest || digest == draftDigest else { throw StoryAgentError.projectChanged }
        var next = run.draft
        StoryContinuityContext.reconcileInheritedFirstFrames(&next)
        next.updatedAt = Date()
        if digest != (try StoryAgentRun.digest(next)) { try save(next, owner: owner) }
        else { next = current }
        run.draft = next; run.applied = true; run.updatedAt = Date()
        try saveRun(run, owner: owner)
        return (run, next)
    }

    /// Reconciles a run that reached a valid domain state but was persisted before the runtime
    /// could write its terminal checkpoint. This is deliberately local and deterministic: it
    /// neither resumes the model nor applies an incomplete or stale draft.
    func applyValidatedRunIfPossible(_ input: StoryAgentRun, owner: String) throws -> (StoryAgentRun, StoryProject)? {
        guard !input.applied, input.abandonedAt == nil, input.checkpoint.pendingCalls.isEmpty,
              input.checkpoint.inFlightCallID == nil else { return nil }
        try input.validate(owner: owner, projectID: input.projectID)
        guard (try? StoryAgentTools.validateCompletion(input)) != nil else { return nil }

        let url = directory(owner: owner, projectID: input.projectID).appendingPathComponent("project.json")
        let current = try JSONDecoder().decode(StoryProject.self, from: Data(contentsOf: url))
        let currentDigest = try StoryAgentRun.digest(current)
        let draftDigest = try StoryAgentRun.digest(input.draft)
        guard currentDigest == input.baseDigest || currentDigest == draftDigest else { return nil }

        var run = input
        let result = "本阶段规划完成，已通过客户端校验。"
        run.checkpoint.status = .completed
        run.checkpoint.completionResult = result
        run.checkpoint.result = result
        run.checkpoint.stopReason = nil
        if run.events.count < 20_000 {
            run.events.append(.init(kind: "completion_recovered", detail: "启动时检测到业务数据已完整，自动完成并应用草稿", modelCalls: run.checkpoint.modelCalls))
        }
        run.updatedAt = Date()
        return try applyRun(run, owner: owner)
    }

    func saveImage(_ data: Data, mimeType: String, projectID: UUID, owner: String,
                   sourceResourceID: String? = nil, generationAttemptID: UUID? = nil,
                   providerResultID: String? = nil, providerAssetID: String? = nil) throws -> StoryImage {
        guard !data.isEmpty, data.count <= 20 * 1024 * 1024,
              ["image/png", "image/jpeg", "image/webp"].contains(mimeType) else { throw StoryError.unsafeFile }
        let ext = mimeType == "image/jpeg" ? "jpg" : mimeType == "image/webp" ? "webp" : "png"
        let image = StoryImage(filename: "\(UUID().uuidString).\(ext)", mimeType: mimeType,
                               sourceResourceID: sourceResourceID, generationAttemptID: generationAttemptID,
                               providerResultID: providerResultID, providerAssetID: providerAssetID)
        try saveFile(data, filename: image.filename, projectID: projectID, owner: owner)
        return image
    }

    func saveVideo(_ result: VideoGenerationResult, projectID: UUID, owner: String) throws -> StoryVideo {
        guard !result.videoData.isEmpty, result.videoData.count <= 512 * 1024 * 1024 else { throw StoryError.unsafeFile }
        let video = StoryVideo(filename: "\(UUID().uuidString).mp4", jobID: result.id, modelName: result.modelName)
        try saveFile(result.videoData, filename: video.filename, projectID: projectID, owner: owner)
        return video
    }

    private func saveFile(_ data: Data, filename: String, projectID: UUID, owner: String) throws {
        let folder = directory(owner: owner, projectID: projectID)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = try fileURL(filename, projectID: projectID, owner: owner)
        try data.write(to: url, options: .atomic)
    }

    private func loadProject(projectID: UUID, owner: String) throws -> StoryProject {
        let url = directory(owner: owner, projectID: projectID).appendingPathComponent("project.json")
        guard FileManager.default.fileExists(atPath: url.path),
              (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 16 * 1024 * 1024 else {
            throw StoryError.invalidProject
        }
        var project = try JSONDecoder().decode(StoryProject.self, from: Data(contentsOf: url))
        guard project.id == projectID else { throw StoryError.invalidProject }
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        try project.validate()
        return project
    }

    nonisolated func fileURL(_ filename: String, projectID: UUID, owner: String) throws -> URL {
        guard filename == URL(fileURLWithPath: filename).lastPathComponent,
              !filename.hasPrefix("."), !filename.isEmpty, !filename.contains("/") else { throw StoryError.unsafeFile }
        let folder = directory(owner: owner, projectID: projectID).resolvingSymlinksInPath()
        let file = folder.appendingPathComponent(filename).resolvingSymlinksInPath()
        guard file.deletingLastPathComponent() == folder else { throw StoryError.unsafeFile }
        return file
    }

    private nonisolated func accountDirectory(_ owner: String) -> URL {
        let key = SHA256.hash(data: Data(owner.utf8)).map { String(format: "%02x", $0) }.joined()
        return root.appendingPathComponent(key, isDirectory: true)
    }
    private nonisolated func directory(owner: String, projectID: UUID) -> URL {
        accountDirectory(owner).appendingPathComponent(projectID.uuidString, isDirectory: true)
    }
}
