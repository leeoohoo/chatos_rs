import ChatOSCore
import CryptoKit
import Foundation

actor StoryProjectStore {
    nonisolated static let maximumProjectBytes = 16 * 1_024 * 1_024
    nonisolated static let maximumRunBytes = 64 * 1_024 * 1_024
    nonisolated static let defaultProjectPageSize = 40
    nonisolated static let defaultHistoryPageSize = 20

    struct PageCursor: Sendable, Equatable {
        var modifiedAt: Date
        var filename: String
    }
    struct Snapshot: Sendable {
        var projects: [StoryProject]
        var unreadableCount: Int
        var nextCursor: PageCursor? = nil
    }
    struct FileCandidate {
        var url: URL
        var cursor: PageCursor
    }
    struct FilePage {
        var candidates: ArraySlice<FileCandidate>
        var nextCursor: PageCursor?
    }
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

    func load(owner: String, after cursor: PageCursor? = nil,
              limit requestedLimit: Int = defaultProjectPageSize) throws -> Snapshot {
        let directory = accountDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return .init(projects: [], unreadableCount: 0) }
        let limit = min(max(requestedLimit, 1), 200)
        var projects: [StoryProject] = []
        var unreadable = 0
        guard let folders = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles, .skipsSubdirectoryDescendants]
        ) else { throw CocoaError(.fileReadUnknown) }
        var candidates: [FileCandidate] = []
        candidates.reserveCapacity(limit + 1)
        for case let folder as URL in folders {
            try Task.checkCancellation()
            guard (try? folder.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true else { continue }
            guard UUID(uuidString: folder.lastPathComponent) != nil else { continue }
            let manifest = folder.appendingPathComponent("project.json")
            guard FileManager.default.fileExists(atPath: manifest.path) else { continue }
            let modifiedAt = (try? manifest.resourceValues(
                forKeys: [.contentModificationDateKey]
            ).contentModificationDate) ?? .distantPast
            let candidate = FileCandidate(
                url: manifest,
                cursor: .init(modifiedAt: modifiedAt, filename: folder.lastPathComponent)
            )
            guard cursor.map({ Self.isAfter(candidate.cursor, cursor: $0) }) ?? true else { continue }
            Self.retain(candidate, in: &candidates, capacity: limit + 1)
        }
        let hasMore = candidates.count > limit
        let page = candidates.prefix(limit)
        for candidate in page {
            try Task.checkCancellation()
            let manifest = candidate.url
            let folder = manifest.deletingLastPathComponent()
            do {
                var project = try JSONDecoder().decode(
                    StoryProject.self,
                    from: AppBoundedFileReader.read(
                        manifest,
                        maximumBytes: Self.maximumProjectBytes
                    )
                )
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
        return .init(
            projects: projects.sorted { $0.updatedAt > $1.updatedAt },
            unreadableCount: unreadable,
            nextCursor: hasMore ? page.last?.cursor : nil
        )
    }

    /// Selects a stable newest-first page from file metadata without decoding every JSON body.
    /// The caller can continue with `nextCursor`; no record is migrated, rewritten, or deleted.
    func filePage(in directory: URL, after cursor: PageCursor?, limit requestedLimit: Int,
                  accepts: (URL) -> Bool) throws -> FilePage {
        guard FileManager.default.fileExists(atPath: directory.path) else {
            return .init(candidates: [], nextCursor: nil)
        }
        let limit = min(max(requestedLimit, 1), 200)
        guard let urls = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [.isRegularFileKey, .contentModificationDateKey],
            options: [.skipsHiddenFiles, .skipsSubdirectoryDescendants]
        ) else { throw CocoaError(.fileReadUnknown) }
        var candidates: [FileCandidate] = []
        candidates.reserveCapacity(limit + 1)
        for case let url as URL in urls {
            try Task.checkCancellation()
            guard accepts(url),
                  (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true
            else { continue }
            let modifiedAt = (try? url.resourceValues(
                forKeys: [.contentModificationDateKey]
            ).contentModificationDate) ?? .distantPast
            let candidate = FileCandidate(
                url: url,
                cursor: .init(modifiedAt: modifiedAt, filename: url.lastPathComponent)
            )
            guard cursor.map({ Self.isAfter(candidate.cursor, cursor: $0) }) ?? true else { continue }
            Self.retain(candidate, in: &candidates, capacity: limit + 1)
        }
        let hasMore = candidates.count > limit
        let page = candidates.prefix(limit)
        return .init(candidates: page, nextCursor: hasMore ? page.last?.cursor : nil)
    }

    static func retain(_ candidate: FileCandidate, in values: inout [FileCandidate], capacity: Int) {
        let index = values.firstIndex { isNewer(candidate.cursor, than: $0.cursor) } ?? values.endIndex
        guard index < capacity else { return }
        values.insert(candidate, at: index)
        if values.count > capacity { values.removeLast() }
    }

    static func isNewer(_ lhs: PageCursor, than rhs: PageCursor) -> Bool {
        if lhs.modifiedAt != rhs.modifiedAt { return lhs.modifiedAt > rhs.modifiedAt }
        return lhs.filename > rhs.filename
    }

    static func isAfter(_ value: PageCursor, cursor: PageCursor) -> Bool {
        if value.modifiedAt != cursor.modifiedAt { return value.modifiedAt < cursor.modifiedAt }
        return value.filename < cursor.filename
    }

    func save(_ input: StoryProject, owner: String) throws {
        var project = input
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        try project.validate()
        let folder = directory(owner: owner, projectID: project.id)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(project)
        guard data.count <= Self.maximumProjectBytes else { throw StoryError.invalidProject }
        try data.write(to: folder.appendingPathComponent("project.json"), options: .atomic)
    }

    /// Saves an ordinary project edit while retaining provider-owned state for video jobs
    /// that are currently running. This prevents an unrelated edit from erasing a newly
    /// returned job ID or a video that completed in the opposite order.
    func save(_ input: StoryProject, owner: String,
              preservingVideoSegmentIDs: Set<String>) throws -> StoryProject {
        let manifest = directory(owner: owner, projectID: input.id).appendingPathComponent("project.json")
        guard FileManager.default.fileExists(atPath: manifest.path) else {
            guard preservingVideoSegmentIDs.isEmpty else { throw StoryError.invalidProject }
            var project = input
            StoryContinuityContext.reconcileInheritedFirstFrames(&project)
            try save(project, owner: owner)
            return project
        }
        let current = try loadProject(projectID: input.id, owner: owner)
        var project = input
        project.version = max(project.version, current.version)
        let durableJobSegmentIDs = Set(current.segments.lazy.compactMap { segment in
            segment.videoJobRecordID == nil ? nil : segment.id
        })
        for segmentID in preservingVideoSegmentIDs.union(durableJobSegmentIDs) {
            guard let currentSegment = current.segments.first(where: { $0.id == segmentID }),
                  let index = project.segments.firstIndex(where: { $0.id == segmentID }) else {
                if durableJobSegmentIDs.contains(segmentID) {
                    throw StoryError.unresolvedSubmission
                }
                continue
            }
            project.segments[index].attempt = currentSegment.attempt
            project.segments[index].videoJobRecordID = currentSegment.videoJobRecordID
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
                    && !project.segments[index].hasVideoJob {
                project.segments[index].confirmedFrameID = nil
                project.segments[index].confirmedLastFrameID = nil
            }
        }
        project.updatedAt = Date()
        try save(project, owner: owner)
        return (project, image)
    }

    func completeAssetImageGeneration(_ file: GeneratedMediaFile, mimeType: String,
                                      projectID: UUID, resourceID: String, attemptID: UUID,
                                      providerResultID: String?, providerAssetID: String?,
                                      owner: String) async throws -> (StoryProject, StoryImage) {
        let current = try loadProject(projectID: projectID, owner: owner)
        guard current.resource(id: resourceID)?.media.generationAttemptID == attemptID else {
            throw StoryError.unresolvedSubmission
        }
        let image = try await saveImage(
            file,
            mimeType: mimeType,
            projectID: projectID,
            owner: owner,
            sourceResourceID: resourceID,
            generationAttemptID: attemptID,
            providerResultID: providerResultID,
            providerAssetID: providerAssetID
        )
        do {
            var project = try loadProject(projectID: projectID, owner: owner)
            guard var resource = project.resource(id: resourceID),
                  resource.media.generationAttemptID == attemptID else {
                throw StoryError.unresolvedSubmission
            }
            resource.media.images.append(image)
            resource.media.generationAttemptID = nil
            let automaticallyConfirmed = resource.media.confirmedImageID == nil
            if automaticallyConfirmed { resource.media.confirmedImageID = image.id }
            try project.replaceResource(resource)
            if automaticallyConfirmed {
                for index in project.segments.indices
                    where project.segments[index].resourceIDs.contains(resourceID)
                        && !project.segments[index].hasVideoJob {
                    project.segments[index].confirmedFrameID = nil
                    project.segments[index].confirmedLastFrameID = nil
                }
            }
            project.updatedAt = Date()
            try save(project, owner: owner)
            return (project, image)
        } catch {
            try? FileManager.default.removeItem(at: fileURL(
                image.filename, projectID: projectID, owner: owner
            ))
            throw error
        }
    }

    func beginFrameImageGeneration(projectID: UUID, segmentID: String, role: StoryFrameRole,
                                   attemptID: UUID, owner: String) throws -> FrameImageGenerationIntent {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              !project.segments[index].hasVideoJob else { throw StoryError.invalidPlan }
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

    func completeFrameImageGeneration(_ file: GeneratedMediaFile, mimeType: String,
                                      projectID: UUID, segmentID: String, role: StoryFrameRole,
                                      attemptID: UUID, providerResultID: String?,
                                      providerAssetID: String?, owner: String) async throws
        -> (StoryProject, StoryImage) {
        let current = try loadProject(projectID: projectID, owner: owner)
        guard let currentIndex = current.segments.firstIndex(where: { $0.id == segmentID })
        else { throw StoryError.invalidPlan }
        let currentMatches = role == .first
            ? current.segments[currentIndex].firstFrames.generationAttemptID == attemptID
            : current.segments[currentIndex].lastFrames.generationAttemptID == attemptID
        guard currentMatches else { throw StoryError.unresolvedSubmission }

        let image = try await saveImage(
            file,
            mimeType: mimeType,
            projectID: projectID,
            owner: owner,
            sourceResourceID: segmentID,
            generationAttemptID: attemptID,
            providerResultID: providerResultID,
            providerAssetID: providerAssetID
        )
        do {
            var project = try loadProject(projectID: projectID, owner: owner)
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else {
                throw StoryError.invalidPlan
            }
            let matchesAttempt = role == .first
                ? project.segments[index].firstFrames.generationAttemptID == attemptID
                : project.segments[index].lastFrames.generationAttemptID == attemptID
            guard matchesAttempt else { throw StoryError.unresolvedSubmission }
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
        } catch {
            try? FileManager.default.removeItem(at: fileURL(
                image.filename, projectID: projectID, owner: owner
            ))
            throw error
        }
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

    /// Atomically changes the Story manifest from the legacy attempt writer to a stable
    /// MediaJobStore association. The caller must stage the matching job first and activate it
    /// only after this commit succeeds. An exact retry after a crash is a read-only success.
    func handoffVideoGenerationToMediaJob(
        projectID: UUID,
        segmentID: String,
        expectedAttempt: StoryVideoAttempt,
        owner: String
    ) throws -> StoryProject {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else {
            throw StoryError.invalidPlan
        }
        if project.segments[index].videoJobRecordID == expectedAttempt.id {
            guard project.version == 3, project.segments[index].attempt == nil else {
                throw StoryError.unresolvedSubmission
            }
            return project
        }
        guard project.segments[index].videoJobRecordID == nil,
              project.segments[index].video == nil,
              project.segments[index].attempt == expectedAttempt else {
            throw StoryError.unresolvedSubmission
        }
        project.version = 3
        project.segments[index].videoJobRecordID = expectedAttempt.id
        project.segments[index].attempt = nil
        project.updatedAt = Date()
        try save(project, owner: owner)
        return project
    }

    func updateVideoGenerationProgress(_ progress: VideoGenerationProgress, projectID: UUID,
                                       segmentID: String, attemptID: UUID,
                                       owner: String) throws -> StoryProject {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt?.id == attemptID else {
            throw StoryError.unresolvedSubmission
        }
        var changed = false
        if let jobID = progress.jobID {
            let existing = project.segments[index].attempt?.jobID
            guard existing == nil || existing == jobID else { throw StoryError.invalidPlan }
            if existing != jobID {
                project.segments[index].attempt?.jobID = jobID
                changed = true
            }
        }
        if project.segments[index].attempt?.status != progress.status {
            project.segments[index].attempt?.status = progress.status
            changed = true
        }
        guard changed else { return project }
        project.updatedAt = Date()
        try save(project, owner: owner)
        return project
    }

    func completeVideoGeneration(_ result: VideoGenerationResult, projectID: UUID,
                                 segmentID: String, attemptID: UUID,
                                 owner: String) async throws -> (StoryProject, StoryVideo) {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt?.id == attemptID else {
            throw StoryError.unresolvedSubmission
        }
        let video = try await saveVideo(result, projectID: projectID, owner: owner)
        project.segments[index].video = video
        project.segments[index].attempt?.status = "completed"
        project.segments[index].error = nil
        project.updatedAt = Date()
        try save(project, owner: owner)
        return (project, video)
    }

    /// Finalizes a video only while the Story manifest still points at the matching durable
    /// MediaJobStore record. Replaying the same provider result after a crash is read-only and
    /// reuses the already committed file instead of producing another local copy.
    func completeVideoGenerationFromMediaJob(
        _ result: VideoGenerationResult,
        projectID: UUID,
        segmentID: String,
        jobRecordID: UUID,
        owner: String
    ) async throws -> (StoryProject, StoryVideo) {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let initialIndex = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[initialIndex].videoJobRecordID == jobRecordID,
              project.segments[initialIndex].attempt == nil else {
            throw StoryError.unresolvedSubmission
        }
        if let existing = project.segments[initialIndex].video {
            guard existing.jobID == result.id else { throw StoryError.unresolvedSubmission }
            return (project, existing)
        }

        let candidate = try await saveVideo(result, projectID: projectID, owner: owner)
        let candidateURL = try fileURL(candidate.filename, projectID: projectID, owner: owner)
        do {
            // `saveVideo` suspends for bounded file I/O. Re-read the manifest so a concurrent
            // replay cannot overwrite a result that won the race while this actor was reentrant.
            project = try loadProject(projectID: projectID, owner: owner)
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
                  project.segments[index].videoJobRecordID == jobRecordID,
                  project.segments[index].attempt == nil else {
                throw StoryError.unresolvedSubmission
            }
            if let existing = project.segments[index].video {
                guard existing.jobID == result.id else { throw StoryError.unresolvedSubmission }
                try? FileManager.default.removeItem(at: candidateURL)
                return (project, existing)
            }
            project.segments[index].video = candidate
            project.segments[index].error = nil
            project.updatedAt = Date()
            try save(project, owner: owner)
            return (project, candidate)
        } catch {
            try? FileManager.default.removeItem(at: candidateURL)
            throw error
        }
    }

    /// Saves the frame decoded from a completed local video without replacing the
    /// pre-generated tail that was confirmed as guidance for the video provider.
    func applyActualVideoLastFrame(_ data: Data, projectID: UUID, segmentID: String,
                                   videoJobID: String, owner: String,
                                   forceNew: Bool = false) throws -> (StoryProject, StoryImage) {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].video?.jobID == videoJobID else {
            throw StoryError.unresolvedSubmission
        }
        if !forceNew, let existing = project.segments[index].lastFrames.images.first(where: {
            $0.derivedFromVideoJobID == videoJobID
        }) {
            project.segments[index].actualVideoLastFrameID = existing.id
            StoryContinuityContext.reconcileInheritedFirstFrames(&project)
            project.updatedAt = Date()
            try save(project, owner: owner)
            return (project, existing)
        }
        let image = try saveImage(
            data, mimeType: "image/png", projectID: projectID, owner: owner,
            sourceResourceID: segmentID, providerResultID: videoJobID,
            providerAssetID: "video-last-frame", derivedFromVideoJobID: videoJobID
        )
        project.segments[index].lastFrames.images.append(image)
        project.segments[index].actualVideoLastFrameID = image.id
        StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        project.updatedAt = Date()
        try save(project, owner: owner)
        return (project, image)
    }

    func applyActualVideoLastFrame(
        _ file: GeneratedMediaFile,
        projectID: UUID,
        segmentID: String,
        videoJobID: String,
        owner: String,
        forceNew: Bool = false
    ) async throws -> (StoryProject, StoryImage) {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let initialIndex = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[initialIndex].video?.jobID == videoJobID else {
            throw StoryError.unresolvedSubmission
        }
        if !forceNew, let existing = project.segments[initialIndex].lastFrames.images.first(where: {
            $0.derivedFromVideoJobID == videoJobID
        }) {
            project.segments[initialIndex].actualVideoLastFrameID = existing.id
            StoryContinuityContext.reconcileInheritedFirstFrames(&project)
            project.updatedAt = Date()
            try save(project, owner: owner)
            return (project, existing)
        }

        let image = try await saveImage(
            file,
            mimeType: "image/png",
            projectID: projectID,
            owner: owner,
            sourceResourceID: segmentID,
            providerResultID: videoJobID,
            providerAssetID: "video-last-frame",
            derivedFromVideoJobID: videoJobID
        )
        let imageURL = try fileURL(image.filename, projectID: projectID, owner: owner)
        do {
            project = try loadProject(projectID: projectID, owner: owner)
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
                  project.segments[index].video?.jobID == videoJobID else {
                throw StoryError.unresolvedSubmission
            }
            if !forceNew, let existing = project.segments[index].lastFrames.images.first(where: {
                $0.derivedFromVideoJobID == videoJobID
            }) {
                try? FileManager.default.removeItem(at: imageURL)
                project.segments[index].actualVideoLastFrameID = existing.id
                StoryContinuityContext.reconcileInheritedFirstFrames(&project)
                project.updatedAt = Date()
                try save(project, owner: owner)
                return (project, existing)
            }
            project.segments[index].lastFrames.images.append(image)
            project.segments[index].actualVideoLastFrameID = image.id
            StoryContinuityContext.reconcileInheritedFirstFrames(&project)
            project.updatedAt = Date()
            try save(project, owner: owner)
            return (project, image)
        } catch {
            try? FileManager.default.removeItem(at: imageURL)
            throw error
        }
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

    /// Persists only the Story-facing error after MediaJobStore has become the sole task
    /// owner. It never recreates a legacy attempt or changes the durable record association.
    func failVideoGenerationFromMediaJob(
        _ error: String,
        projectID: UUID,
        segmentID: String,
        jobRecordID: UUID,
        owner: String
    ) throws -> StoryProject {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].videoJobRecordID == jobRecordID,
              project.segments[index].attempt == nil,
              project.segments[index].video == nil else {
            throw StoryError.unresolvedSubmission
        }
        guard project.segments[index].error != error else { return project }
        project.segments[index].error = error
        project.updatedAt = Date()
        try save(project, owner: owner)
        return project
    }

    /// Clears a Story association only after MediaJobStore has durably terminalized the exact
    /// record. A nil association is accepted as an idempotent replay after a prior successful
    /// commit; a different record is never cleared.
    func releaseVideoJobAfterVerification(
        projectID: UUID,
        segmentID: String,
        jobRecordID: UUID,
        owner: String
    ) throws -> StoryProject {
        var project = try loadProject(projectID: projectID, owner: owner)
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt == nil,
              project.segments[index].video == nil else {
            throw StoryError.unresolvedSubmission
        }
        if project.segments[index].videoJobRecordID == nil {
            return project
        }
        guard project.segments[index].videoJobRecordID == jobRecordID else {
            throw StoryError.unresolvedSubmission
        }
        project.segments[index].videoJobRecordID = nil
        project.segments[index].error = nil
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
        guard data.count <= Self.maximumRunBytes else { throw StoryAgentError.invalidRun }
        try data.write(to: folder.appendingPathComponent("\(run.id).json"), options: .atomic)
    }

    func loadRuns(owner: String, projectID: UUID, after cursor: PageCursor? = nil,
                  limit: Int = defaultHistoryPageSize)
        throws -> (runs: [StoryAgentRun], unreadable: Int, nextCursor: PageCursor?) {
        let folder = directory(owner: owner, projectID: projectID).appendingPathComponent("runs", isDirectory: true)
        var runs: [StoryAgentRun] = []; var unreadable = 0
        let page = try filePage(in: folder, after: cursor, limit: limit) {
            $0.pathExtension == "json" && UUID(uuidString: $0.deletingPathExtension().lastPathComponent) != nil
        }
        for candidate in page.candidates {
            try Task.checkCancellation()
            let url = candidate.url
            guard let id = UUID(uuidString: url.deletingPathExtension().lastPathComponent) else { continue }
            do {
                let run = try JSONDecoder().decode(
                    StoryAgentRun.self,
                    from: AppBoundedFileReader.read(
                        url,
                        maximumBytes: Self.maximumRunBytes
                    )
                )
                guard run.id == id else { throw StoryAgentError.invalidRun }
                try run.validate(owner: owner, projectID: projectID)
                runs.append(run)
            } catch { unreadable += 1 }
        }
        return (runs.sorted { $0.updatedAt > $1.updatedAt }, unreadable, page.nextCursor)
    }

    func loadRun(owner: String, projectID: UUID, runID: UUID) throws -> StoryAgentRun {
        let url = directory(owner: owner, projectID: projectID)
            .appendingPathComponent("runs", isDirectory: true)
            .appendingPathComponent("\(runID).json")
        let run = try JSONDecoder().decode(
            StoryAgentRun.self,
            from: AppBoundedFileReader.read(url, maximumBytes: Self.maximumRunBytes)
        )
        guard run.id == runID else { throw StoryAgentError.invalidRun }
        try run.validate(owner: owner, projectID: projectID)
        return run
    }

    /// Canonical project replacement is guarded by the original digest. A crash after project save
    /// but before the applied marker is recoverable by comparing with the completed draft digest.
    func applyRun(_ input: StoryAgentRun, owner: String) throws -> (StoryAgentRun, StoryProject) {
        var run = input
        try run.validate(owner: owner, projectID: run.projectID)
        guard run.abandonedAt == nil, run.checkpoint.status == .completed else { throw StoryAgentError.incompletePlan }
        try StoryAgentTools.validateCompletion(run)
        let url = directory(owner: owner, projectID: run.projectID).appendingPathComponent("project.json")
        let current = try JSONDecoder().decode(
            StoryProject.self,
            from: AppBoundedFileReader.read(url, maximumBytes: Self.maximumProjectBytes)
        )
        let digest = try StoryAgentRun.digest(current)
        let draftDigest = try StoryAgentRun.digest(run.draft)
        guard try StoryAgentRun.matchesPersistedDigest(run.baseDigest, project: current)
                || digest == draftDigest else { throw StoryAgentError.projectChanged }
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
        let current = try JSONDecoder().decode(
            StoryProject.self,
            from: AppBoundedFileReader.read(url, maximumBytes: Self.maximumProjectBytes)
        )
        let currentDigest = try StoryAgentRun.digest(current)
        let draftDigest = try StoryAgentRun.digest(input.draft)
        guard try StoryAgentRun.matchesPersistedDigest(input.baseDigest, project: current)
                || currentDigest == draftDigest else { return nil }

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
                   providerResultID: String? = nil, providerAssetID: String? = nil,
                   derivedFromVideoJobID: String? = nil) throws -> StoryImage {
        guard !data.isEmpty, data.count <= 20 * 1024 * 1024,
              ["image/png", "image/jpeg", "image/webp"].contains(mimeType) else { throw StoryError.unsafeFile }
        let ext = mimeType == "image/jpeg" ? "jpg" : mimeType == "image/webp" ? "webp" : "png"
        let image = StoryImage(filename: "\(UUID().uuidString).\(ext)", mimeType: mimeType,
                               sourceResourceID: sourceResourceID, generationAttemptID: generationAttemptID,
                               providerResultID: providerResultID, providerAssetID: providerAssetID,
                               derivedFromVideoJobID: derivedFromVideoJobID)
        try saveFile(data, filename: image.filename, projectID: projectID, owner: owner)
        return image
    }

    func saveImage(_ file: GeneratedMediaFile, mimeType: String, projectID: UUID, owner: String,
                   sourceResourceID: String? = nil, generationAttemptID: UUID? = nil,
                   providerResultID: String? = nil, providerAssetID: String? = nil,
                   derivedFromVideoJobID: String? = nil) async throws -> StoryImage {
        let maximumBytes = 20 * 1_024 * 1_024
        guard file.byteCount > 0, file.byteCount <= maximumBytes,
              ["image/png", "image/jpeg", "image/webp"].contains(mimeType) else {
            throw StoryError.unsafeFile
        }
        let ext = mimeType == "image/jpeg" ? "jpg" : mimeType == "image/webp" ? "webp" : "png"
        let image = StoryImage(filename: "\(UUID().uuidString).\(ext)", mimeType: mimeType,
                               sourceResourceID: sourceResourceID,
                               generationAttemptID: generationAttemptID,
                               providerResultID: providerResultID,
                               providerAssetID: providerAssetID,
                               derivedFromVideoJobID: derivedFromVideoJobID)
        let folder = directory(owner: owner, projectID: projectID)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let destinationURL = try fileURL(
            image.filename,
            projectID: projectID,
            owner: owner
        )
        defer { withExtendedLifetime(file) {} }
        try await AppCancellableDetachedWork.run {
            try AppBoundedFileReader.copy(
                file.url,
                to: destinationURL,
                expectedBytes: file.byteCount,
                maximumBytes: maximumBytes
            )
        }
        return image
    }

    func saveVideo(_ result: VideoGenerationResult, projectID: UUID, owner: String) async throws -> StoryVideo {
        let maximumBytes = 512 * 1_024 * 1_024
        guard (result.videoFile != nil) != (!result.videoData.isEmpty) else {
            throw StoryError.unsafeFile
        }
        let video = StoryVideo(filename: "\(UUID().uuidString).mp4", jobID: result.id, modelName: result.modelName)
        let folder = directory(owner: owner, projectID: projectID)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let destinationURL = try fileURL(
            video.filename,
            projectID: projectID,
            owner: owner
        )
        if let videoFile = result.videoFile {
            try await AppCancellableDetachedWork.run {
                try AppBoundedFileReader.copy(
                    videoFile.url,
                    to: destinationURL,
                    expectedBytes: videoFile.byteCount,
                    maximumBytes: maximumBytes
                )
            }
        } else {
            guard result.videoData.count <= maximumBytes else { throw StoryError.unsafeFile }
            try await AppCancellableDetachedWork.run {
                try result.videoData.write(to: destinationURL, options: .atomic)
            }
        }
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
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw StoryError.invalidProject
        }
        var project = try JSONDecoder().decode(
            StoryProject.self,
            from: AppBoundedFileReader.read(url, maximumBytes: Self.maximumProjectBytes)
        )
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

extension StoryProjectStore {
    /// Resolves only the exact project/segment/batch carried by a durable workload. A damaged
    /// referenced manifest throws so maintenance preserves the job instead of guessing.
    func referencesVideoJob(
        id: UUID,
        owner: String,
        projectID: UUID,
        segmentID: String,
        batchID: UUID?
    ) throws -> Bool {
        let folder = directory(owner: owner, projectID: projectID)
        guard FileManager.default.fileExists(atPath: folder.path) else { return false }
        let folderValues = try folder.resourceValues(forKeys: [
            .isDirectoryKey,
            .isSymbolicLinkKey,
        ])
        guard folderValues.isDirectory == true, folderValues.isSymbolicLink != true else {
            throw StoryError.invalidProject
        }
        let projectManifest = folder.appendingPathComponent("project.json")
        var projectReferences = false
        if FileManager.default.fileExists(atPath: projectManifest.path) {
            let manifestValues = try projectManifest.resourceValues(forKeys: [
                .isRegularFileKey,
                .isSymbolicLinkKey,
            ])
            guard manifestValues.isRegularFile == true,
                  manifestValues.isSymbolicLink != true else {
                throw StoryError.invalidProject
            }
            let project = try loadProject(projectID: projectID, owner: owner)
            projectReferences = project.segments.first(where: { $0.id == segmentID })?
                .videoJobRecordID == id
        }
        guard let batchID else { return projectReferences }
        let batchManifest = folder.appendingPathComponent("media-batch-\(batchID).json")
        guard FileManager.default.fileExists(atPath: batchManifest.path) else {
            return projectReferences
        }
        let batchValues = try batchManifest.resourceValues(forKeys: [
            .isRegularFileKey,
            .isSymbolicLinkKey,
        ])
        guard batchValues.isRegularFile == true, batchValues.isSymbolicLink != true else {
            throw StoryError.invalidProject
        }
        let batch = try loadMediaBatch(owner: owner, projectID: projectID, batchID: batchID)
        let batchReferences = batch.jobs["videos:\(segmentID)"]?.videoJobRecordID == id
        return projectReferences || batchReferences
    }
}

extension StoryProjectStore: StoryProjectCatalogLoading {
    func loadStoryProjects(
        ownerUserID: String,
        after cursor: StoryProjectCatalogCursor?,
        limit: Int
    ) async throws -> StoryProjectCatalogPage {
        let snapshot = try load(
            owner: ownerUserID,
            after: cursor.map { .init(modifiedAt: $0.modifiedAt, filename: $0.recordID) },
            limit: limit
        )
        return StoryProjectCatalogPage(
            projects: snapshot.projects,
            unreadableCount: snapshot.unreadableCount,
            nextCursor: snapshot.nextCursor.map {
                .init(modifiedAt: $0.modifiedAt, recordID: $0.filename)
            }
        )
    }
}

extension StoryProjectStore: StoryProjectMutationPersisting {
    func persistStoryProject(
        _ project: StoryProject,
        ownerUserID: String,
        preservingVideoSegmentIDs: Set<String>
    ) async throws -> StoryProject {
        var next = project
        StoryContinuityContext.reconcileInheritedFirstFrames(&next)
        next.updatedAt = Date()
        return try save(
            next,
            owner: ownerUserID,
            preservingVideoSegmentIDs: preservingVideoSegmentIDs
        )
    }
}
