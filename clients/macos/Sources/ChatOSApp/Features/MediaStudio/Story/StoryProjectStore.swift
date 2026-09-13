import ChatOSCore
import ChatOSConnector
import CryptoKit
import Foundation

protocol StoryProjectIPCClient: Sendable {
    func storyRecord(id: String) async throws -> LocalAgentStorySnapshot
    func storyRecords() async throws -> [LocalAgentStorySnapshot]
    func putStoryRecord(
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentStoryDraft
    ) async throws -> LocalAgentStorySnapshot
    func deleteStoryRecord(id: String, expectedRevision: UInt64) async throws
}

extension NativeLocalAgentIPCClient: StoryProjectIPCClient {}

struct StoryProjectStorageContext: Sendable {
    let ownerUserID: String
    let client: any StoryProjectIPCClient
}

private actor StoryProjectMutationGate {
    private var locked = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func acquire() async {
        guard locked else {
            locked = true
            return
        }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
    }

    func release() {
        if waiters.isEmpty {
            locked = false
        } else {
            waiters.removeFirst().resume()
        }
    }
}

actor StoryProjectStore {
    typealias ContextProvider = @MainActor @Sendable (String) async throws
        -> StoryProjectStorageContext
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
    private let contextProvider: ContextProvider
    private let mutationGate = StoryProjectMutationGate()

    init(root: URL? = nil, contextProvider: @escaping ContextProvider) {
        self.root = (root ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ChatOSSwift/StoryStudioV2/Payloads", isDirectory: true))
            .resolvingSymlinksInPath()
        self.contextProvider = contextProvider
    }

    func load(owner: String) async throws -> Snapshot {
        let context = try await storageContext(owner: owner)
        var projects: [StoryProject] = []
        var unreadable = 0
        for record in try await context.client.storyRecords() where record.draft.kind == .project {
            do {
                guard let projectID = UUID(uuidString: record.draft.projectID),
                      record.ownerUserID == context.ownerUserID,
                      record.recordID == Self.projectRecordID(projectID) else {
                    throw StoryError.invalidProject
                }
                var project: StoryProject = try Self.decodeState(record.draft.state)
                guard project.id.uuidString.lowercased() == record.draft.projectID.lowercased() else {
                    throw StoryError.invalidProject
                }
                StoryContinuityContext.reconcileInheritedFirstFrames(&project)
                try project.validate()
                projects.append(project)
            } catch { unreadable += 1 }
        }
        return .init(projects: projects.sorted { $0.updatedAt > $1.updatedAt }, unreadableCount: unreadable)
    }

    func save(_ input: StoryProject, owner: String) async throws {
        try await withMutation {
            try await self.saveUnlocked(input, owner: owner)
        }
    }

    func saveUnlocked(_ input: StoryProject, owner: String) async throws {
        var project = input
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        try project.validate()
        _ = try await put(
            project,
            recordID: Self.projectRecordID(project.id),
            projectID: project.id,
            kind: .project,
            status: "draft",
            owner: owner
        )
    }

    /// Saves an ordinary project edit while retaining provider-owned state for video jobs
    /// that are currently running. This prevents an unrelated edit from erasing a newly
    /// returned job ID or a video that completed in the opposite order.
    func save(_ input: StoryProject, owner: String,
              preservingVideoSegmentIDs: Set<String>) async throws -> StoryProject {
        try await withMutation {
            guard !preservingVideoSegmentIDs.isEmpty else {
                var project = input
                StoryContinuityContext.reconcileInheritedFirstFrames(&project)
                try await self.saveUnlocked(project, owner: owner)
                return project
            }
            let current = try await self.loadProject(projectID: input.id, owner: owner)
            var project = input
            for segmentID in preservingVideoSegmentIDs {
                guard let currentSegment = current.segments.first(where: { $0.id == segmentID }),
                      let index = project.segments.firstIndex(where: { $0.id == segmentID }) else { continue }
                project.segments[index].attempt = currentSegment.attempt
                project.segments[index].video = currentSegment.video
                project.segments[index].error = currentSegment.error
            }
            StoryContinuityContext.reconcileInheritedFirstFrames(&project)
            try await self.saveUnlocked(project, owner: owner)
            return project
        }
    }

    /// Creates one durable, ID-addressed intent from the latest manifest. The store actor
    /// serializes this tiny transaction while the expensive provider calls remain concurrent.
    func beginAssetImageGeneration(projectID: UUID, resourceID: String, attemptID: UUID,
                                   owner: String) async throws -> AssetImageGenerationIntent {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
            guard var resource = project.resource(id: resourceID) else { throw StoryError.invalidPlan }
            guard resource.media.generationAttemptID == nil else { throw StoryError.unresolvedSubmission }
            resource.media.generationAttemptID = attemptID
            try project.replaceResource(resource)
            project.updatedAt = Date()
            try await self.saveUnlocked(project, owner: owner)
            return .init(project: project, resource: resource, attemptID: attemptID)
        }
    }

    /// Merges a completed image into the latest manifest instead of replacing the whole
    /// project snapshot. A late or mismatched task can therefore never write into another
    /// resource, and reverse-order completions preserve every result.
    func completeAssetImageGeneration(_ data: Data, mimeType: String, projectID: UUID,
                                      resourceID: String, attemptID: UUID,
                                      providerResultID: String?, providerAssetID: String?,
                                      owner: String) async throws -> (StoryProject, StoryImage) {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
            guard var resource = project.resource(id: resourceID),
                  resource.media.generationAttemptID == attemptID else {
                throw StoryError.unresolvedSubmission
            }
            let image = try await self.saveImage(data, mimeType: mimeType, projectID: projectID, owner: owner,
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
            try await self.saveUnlocked(project, owner: owner)
            return (project, image)
        }
    }

    func beginFrameImageGeneration(projectID: UUID, segmentID: String, role: StoryFrameRole,
                                   attemptID: UUID, owner: String) async throws -> FrameImageGenerationIntent {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
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
            try await self.saveUnlocked(project, owner: owner)
            return .init(project: project, segment: project.segments[index], role: role, attemptID: attemptID)
        }
    }

    func completeFrameImageGeneration(_ data: Data, mimeType: String, projectID: UUID,
                                      segmentID: String, role: StoryFrameRole, attemptID: UUID,
                                      providerResultID: String?, providerAssetID: String?,
                                      owner: String) async throws -> (StoryProject, StoryImage) {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else { throw StoryError.invalidPlan }
            let matchesAttempt = role == .first
                ? project.segments[index].firstFrames.generationAttemptID == attemptID
                : project.segments[index].lastFrames.generationAttemptID == attemptID
            guard matchesAttempt else { throw StoryError.unresolvedSubmission }
            let image = try await self.saveImage(data, mimeType: mimeType, projectID: projectID, owner: owner,
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
            try await self.saveUnlocked(project, owner: owner)
            return (project, image)
        }
    }

    /// Persists the per-segment video intent against the latest project manifest. Other
    /// segments may begin or finish concurrently without replacing this segment's state.
    func beginVideoGeneration(projectID: UUID, segmentID: String, attempt: StoryVideoAttempt,
                              owner: String) async throws -> VideoGenerationIntent {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
                  project.segments[index].isReady else { throw StoryError.invalidPlan }
            project.segments[index].attempt = attempt
            project.segments[index].error = nil
            project.updatedAt = Date()
            try await self.saveUnlocked(project, owner: owner)
            return .init(project: project, segment: project.segments[index], attempt: attempt)
        }
    }

    func updateVideoGenerationProgress(_ progress: VideoGenerationProgress, projectID: UUID,
                                       segmentID: String, attemptID: UUID,
                                       owner: String) async throws -> StoryProject {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
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
            try await self.saveUnlocked(project, owner: owner)
            return project
        }
    }

    func completeVideoGeneration(_ result: VideoGenerationResult, projectID: UUID,
                                 segmentID: String, attemptID: UUID,
                                 owner: String) async throws -> (StoryProject, StoryVideo) {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
                  project.segments[index].attempt?.id == attemptID else {
                throw StoryError.unresolvedSubmission
            }
            let video = try await self.saveVideo(result, projectID: projectID, owner: owner)
            project.segments[index].video = video
            project.segments[index].attempt?.status = "completed"
            project.segments[index].error = nil
            project.updatedAt = Date()
            try await self.saveUnlocked(project, owner: owner)
            return (project, video)
        }
    }

    func failVideoGeneration(_ error: String, projectID: UUID, segmentID: String,
                             attemptID: UUID, owner: String) async throws -> StoryProject {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
                  project.segments[index].attempt?.id == attemptID else {
                throw StoryError.unresolvedSubmission
            }
            project.segments[index].error = error
            project.updatedAt = Date()
            try await self.saveUnlocked(project, owner: owner)
            return project
        }
    }

    /// A deterministic failure before the provider POST must release the local intent. Keeping
    /// it would falsely ask the user to recover a task ID that can never exist.
    func failVideoGenerationBeforeSubmission(_ error: String, projectID: UUID, segmentID: String,
                                             attemptID: UUID, owner: String) async throws -> StoryProject {
        try await withMutation {
            var project = try await self.loadProject(projectID: projectID, owner: owner)
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
            try await self.saveUnlocked(project, owner: owner)
            return project
        }
    }

    func saveRun(_ run: StoryAgentRun, owner: String) async throws {
        try await withMutation {
            try await self.saveRunUnlocked(run, owner: owner)
        }
    }

    private func saveRunUnlocked(_ run: StoryAgentRun, owner: String) async throws {
        try run.validate(owner: owner, projectID: run.projectID)
        _ = try await put(
            run,
            recordID: Self.agentRunRecordID(run.id),
            projectID: run.projectID,
            kind: .agentRun,
            status: run.checkpoint.status.rawValue,
            owner: owner
        )
    }

    func loadRuns(owner: String, projectID: UUID) async throws -> (runs: [StoryAgentRun], unreadable: Int) {
        let context = try await storageContext(owner: owner)
        var runs: [StoryAgentRun] = []; var unreadable = 0
        for record in try await context.client.storyRecords()
            where record.draft.kind == .agentRun
                && record.draft.projectID.lowercased() == projectID.uuidString.lowercased() {
            do {
                let run: StoryAgentRun = try Self.decodeState(record.draft.state)
                guard record.ownerUserID == context.ownerUserID,
                      record.recordID == Self.agentRunRecordID(run.id) else {
                    throw StoryAgentError.invalidRun
                }
                try run.validate(owner: owner, projectID: projectID)
                runs.append(run)
            } catch { unreadable += 1 }
        }
        return (runs.sorted { $0.updatedAt > $1.updatedAt }, unreadable)
    }

    /// Canonical project replacement is guarded by the original digest. A crash after project save
    /// but before the applied marker is recoverable by comparing with the completed draft digest.
    func applyRun(_ input: StoryAgentRun, owner: String) async throws -> (StoryAgentRun, StoryProject) {
        try await withMutation {
            try await self.applyRunUnlocked(input, owner: owner)
        }
    }

    private func applyRunUnlocked(
        _ input: StoryAgentRun,
        owner: String
    ) async throws -> (StoryAgentRun, StoryProject) {
        var run = input
        try run.validate(owner: owner, projectID: run.projectID)
        guard run.abandonedAt == nil, run.checkpoint.status == .completed else { throw StoryAgentError.incompletePlan }
        try StoryAgentTools.validateCompletion(run)
        let current = try await loadProject(projectID: run.projectID, owner: owner)
        let digest = try StoryAgentRun.digest(current)
        let draftDigest = try StoryAgentRun.digest(run.draft)
        guard digest == run.baseDigest || digest == draftDigest else { throw StoryAgentError.projectChanged }
        var next = run.draft
        StoryContinuityContext.reconcileInheritedFirstFrames(&next)
        next.updatedAt = Date()
        if digest != (try StoryAgentRun.digest(next)) { try await saveUnlocked(next, owner: owner) }
        else { next = current }
        run.draft = next; run.applied = true; run.updatedAt = Date()
        try await saveRunUnlocked(run, owner: owner)
        return (run, next)
    }

    /// Reconciles a run that reached a valid domain state but was persisted before the runtime
    /// could write its terminal checkpoint. This is deliberately local and deterministic: it
    /// neither resumes the model nor applies an incomplete or stale draft.
    func applyValidatedRunIfPossible(_ input: StoryAgentRun, owner: String) async throws -> (StoryAgentRun, StoryProject)? {
        try await withMutation {
            guard !input.applied, input.abandonedAt == nil, input.checkpoint.pendingCalls.isEmpty,
                  input.checkpoint.inFlightCallID == nil else { return nil }
            try input.validate(owner: owner, projectID: input.projectID)
            guard (try? StoryAgentTools.validateCompletion(input)) != nil else { return nil }

            let current = try await self.loadProject(projectID: input.projectID, owner: owner)
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
                run.events.append(.init(
                    kind: "completion_recovered",
                    detail: "启动时检测到业务数据已完整，自动完成并应用草稿",
                    modelCalls: run.checkpoint.modelCalls
                ))
            }
            run.updatedAt = Date()
            return try await self.applyRunUnlocked(run, owner: owner)
        }
    }

    func saveImage(_ data: Data, mimeType: String, projectID: UUID, owner: String,
                   sourceResourceID: String? = nil, generationAttemptID: UUID? = nil,
                   providerResultID: String? = nil, providerAssetID: String? = nil) async throws -> StoryImage {
        guard !data.isEmpty, data.count <= 20 * 1024 * 1024,
              ["image/png", "image/jpeg", "image/webp"].contains(mimeType) else { throw StoryError.unsafeFile }
        let ext = mimeType == "image/jpeg" ? "jpg" : mimeType == "image/webp" ? "webp" : "png"
        let image = StoryImage(filename: "\(UUID().uuidString).\(ext)", mimeType: mimeType,
                               sourceResourceID: sourceResourceID, generationAttemptID: generationAttemptID,
                               providerResultID: providerResultID, providerAssetID: providerAssetID)
        try saveFile(data, filename: image.filename, projectID: projectID, owner: owner)
        return image
    }

    func saveVideo(_ result: VideoGenerationResult, projectID: UUID, owner: String) async throws -> StoryVideo {
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

    func loadProject(projectID: UUID, owner: String) async throws -> StoryProject {
        let context = try await storageContext(owner: owner)
        let record = try await context.client.storyRecord(id: Self.projectRecordID(projectID))
        guard record.ownerUserID == context.ownerUserID,
              record.draft.kind == .project,
              record.draft.projectID.lowercased() == projectID.uuidString.lowercased() else {
            throw StoryError.invalidProject
        }
        var project: StoryProject = try Self.decodeState(record.draft.state)
        guard project.id == projectID else { throw StoryError.invalidProject }
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        try project.validate()
        return project
    }

    func putRecord<T: Encodable>(
        _ value: T,
        recordID: String,
        projectID: UUID,
        kind: LocalAgentStoryKind,
        status: String?,
        owner: String
    ) async throws -> LocalAgentStorySnapshot {
        try await put(
            value,
            recordID: recordID,
            projectID: projectID,
            kind: kind,
            status: status,
            owner: owner
        )
    }

    private func put<T: Encodable>(
        _ value: T,
        recordID: String,
        projectID: UUID,
        kind: LocalAgentStoryKind,
        status: String?,
        owner: String
    ) async throws -> LocalAgentStorySnapshot {
        let context = try await storageContext(owner: owner)
        let expectedRevision: UInt64?
        do {
            let current = try await context.client.storyRecord(id: recordID)
            guard current.ownerUserID == context.ownerUserID,
                  current.recordID == recordID,
                  current.draft.projectID.lowercased() == projectID.uuidString.lowercased(),
                  current.draft.kind == kind else {
                throw StoryError.invalidProject
            }
            expectedRevision = current.revision
        } catch NativeLocalAgentIPCError.rejected(let error) where error.code == "story_not_found" {
            expectedRevision = nil
        }
        let stored = try await context.client.putStoryRecord(
            id: recordID,
            expectedRevision: expectedRevision,
            draft: .init(
                projectID: projectID.uuidString.lowercased(),
                kind: kind,
                status: status,
                state: try Self.encodeState(value)
            )
        )
        guard stored.ownerUserID == context.ownerUserID,
              stored.recordID == recordID,
              stored.draft.projectID.lowercased() == projectID.uuidString.lowercased(),
              stored.draft.kind == kind else {
            throw StoryError.invalidProject
        }
        return stored
    }

    func storageContext(owner: String) async throws -> StoryProjectStorageContext {
        let context = try await contextProvider(owner)
        guard !owner.isEmpty, context.ownerUserID == owner else {
            throw StoryError.invalidProject
        }
        return context
    }

    func withMutation<T>(
        _ operation: () async throws -> T
    ) async throws -> T {
        await mutationGate.acquire()
        do {
            let result = try await operation()
            await mutationGate.release()
            return result
        } catch {
            await mutationGate.release()
            throw error
        }
    }

    private static func encodeState<T: Encodable>(_ value: T) throws -> LocalAgentJSONValue {
        try JSONDecoder().decode(LocalAgentJSONValue.self, from: JSONEncoder().encode(value))
    }

    static func decodeState<T: Decodable>(
        _ state: LocalAgentJSONValue,
        as type: T.Type = T.self
    ) throws -> T {
        try JSONDecoder().decode(type, from: JSONEncoder().encode(state))
    }

    nonisolated static func projectRecordID(_ projectID: UUID) -> String {
        "project:\(projectID.uuidString.lowercased())"
    }

    nonisolated static func agentRunRecordID(_ runID: UUID) -> String {
        "agent-run:\(runID.uuidString.lowercased())"
    }

    nonisolated static func mediaBatchRecordID(_ batchID: UUID) -> String {
        "media-batch:\(batchID.uuidString.lowercased())"
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
