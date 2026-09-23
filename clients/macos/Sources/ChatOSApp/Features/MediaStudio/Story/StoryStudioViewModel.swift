import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Combine
import Foundation

@MainActor
final class StoryStudioViewModel: ObservableObject {
    static let imageGenerationLockNotice = "还有图片版本正在后台生成。已确认的素材仍可继续生成视频；项目结构与模型设置将在图片完成后恢复修改。"
    struct SourceDraft { var source: String; var style: String; var ratio: String }
    struct CreationHistoryImage: Identifiable, Sendable {
        enum Kind: String, Sendable { case character, scene, prop, firstFrame, lastFrame, videoLastFrame }
        var id: String
        var title: String
        var prompt: String
        var kind: Kind
        var asset: GeneratedMediaAsset
    }
    struct CreationHistoryVideo: Identifiable, Sendable {
        var id: String
        var segmentID: String
        var segmentNumber: Int
        var title: String
        var prompt: String
        var modelName: String
        var createdAt: Date
        var seconds: Int
        var fileURL: URL
        var isCurrentVersion = true
    }
    struct CreationHistoryGroup: Identifiable, Sendable {
        var id: UUID { projectID }
        var projectID: UUID
        var projectTitle: String
        var updatedAt: Date
        var totalSegmentCount: Int
        var images: [CreationHistoryImage]
        var videos: [CreationHistoryVideo]

        var currentVideos: [CreationHistoryVideo] { videos.filter(\.isCurrentVersion) }
        var isComplete: Bool {
            totalSegmentCount > 0 && Set(currentVideos.map(\.segmentID)).count == totalSegmentCount
        }
    }
    struct ReusableStoryImage: Identifiable, Sendable {
        var id: String
        var projectID: UUID
        var projectTitle: String
        var resourceID: String
        var resourceName: String
        var kind: StoryResource.Kind
        var image: StoryImage
        var asset: GeneratedMediaAsset
    }
    struct AssetGenerationKey: Hashable, Sendable {
        var projectID: UUID
        var resourceID: String
    }
    struct FrameGenerationKey: Hashable, Sendable {
        var projectID: UUID
        var segmentID: String
        var role: String

        init(projectID: UUID, segmentID: String, role: StoryFrameRole) {
            self.projectID = projectID; self.segmentID = segmentID; self.role = role.rawValue
        }
    }
    struct VideoGenerationKey: Hashable, Sendable {
        var projectID: UUID
        var segmentID: String
    }
    @Published var projects: [StoryProject] = []
    @Published var selectedProjectID: UUID?
    @Published var selectedSegmentID: String?
    @Published var selectedSegments: Set<String> = []
    @Published var isLoading = false
    @Published var isBusy = false
    @Published var operation = ""
    @Published var errorMessage: String?
    @Published var progress: VideoGenerationProgress?
    @Published var activeSegmentID: String?
    @Published var activeProjectID: UUID?
    @Published var activeAgentRunID: UUID?
    @Published var pauseRequested = false
    @Published var agentRuns: [StoryAgentRun] = []
    @Published var isLoadingAgentRuns = false
    @Published var mediaBatches: [StoryMediaBatch] = []
    @Published var optimizationSuggestion: StoryPlanningTools.OptimizationSuggestion?
    @Published var optimizationTarget: StoryPlanningTools.OptimizationTarget?
    @Published var streamingModelText = ""
    @Published var streamingToolName: String?
    @Published var activeAssetGenerations: Set<AssetGenerationKey> = []
    @Published var assetGenerationErrors: [AssetGenerationKey: String] = [:]
    @Published var activeFrameGenerations: Set<FrameGenerationKey> = []
    @Published var frameGenerationErrors: [FrameGenerationKey: String] = [:]
    @Published var activeVideoGenerations: Set<VideoGenerationKey> = []
    @Published var videoGenerationProgress: [VideoGenerationKey: VideoGenerationProgress] = [:]
    @Published var activeVideoFrameExtractions: Set<VideoGenerationKey> = []
    let store: StoryProjectStore
    let media: any MediaGenerationServicing
    let planner: (any StoryPlanningServicing)?
    let agentServices: (any AgentServiceProviding)?
    let agentSettings: AgentSettingsStore
    var owner: String?
    var session = UUID()
    var task: Task<Void, Never>?
    var loadTask: Task<Void, Never>?
    var historyTask: Task<Void, Never>?
    var assetGenerationTasks: [AssetGenerationKey: Task<Void, Never>] = [:]
    var frameGenerationTasks: [FrameGenerationKey: Task<Void, Never>] = [:]
    var videoGenerationTasks: [VideoGenerationKey: Task<Void, Never>] = [:]
    var videoFrameExtractionTasks: [VideoGenerationKey: Task<Void, Never>] = [:]
    var videoBatchTask: Task<Void, Never>?
    var sourceDrafts: [UUID: SourceDraft] = [:]

    init(media: any MediaGenerationServicing, planner: (any StoryPlanningServicing)? = nil,
         store: StoryProjectStore = StoryProjectStore(), agentServices: (any AgentServiceProviding)? = nil,
         agentSettings: AgentSettingsStore = .init()) {
        self.media = media; self.planner = planner; self.store = store
        self.agentServices = agentServices ?? (planner as? any AgentServiceProviding)
        self.agentSettings = agentSettings
    }

    var project: StoryProject? { projects.first { $0.id == selectedProjectID } }
    var segment: StorySegment? { project?.segments.first { $0.id == selectedSegmentID } }
    var canCreate: Bool {
        owner != nil && !isLoading && !isBusy
            && activeAssetGenerations.isEmpty && activeFrameGenerations.isEmpty
    }
    var supportsAgentPlanning: Bool { agentServices != nil }
    var projectAgentRuns: [StoryAgentRun] { agentRuns.filter { $0.projectID == selectedProjectID } }
    var latestAgentRun: StoryAgentRun? { projectAgentRuns.first }
    var projectMediaBatches: [StoryMediaBatch] { mediaBatches.filter { $0.draft.id == selectedProjectID } }
    var creationHistoryGroups: [CreationHistoryGroup] {
        projects.compactMap { project in
            var images: [CreationHistoryImage] = []
            for resource in project.resources {
                for image in resource.images where isGenerated(image) {
                    guard let asset = mediaAsset(image, projectID: project.id) else { continue }
                    let kind: CreationHistoryImage.Kind = switch resource.kind {
                    case .character: .character
                    case .scene: .scene
                    case .prop: .prop
                    }
                    images.append(.init(
                        id: "\(project.id):resource:\(resource.id):\(image.id)",
                        title: resource.name, prompt: resource.prompt, kind: kind, asset: asset
                    ))
                }
            }
            for segment in project.segments {
                for image in segment.firstFrames.images where isGenerated(image) {
                    if image.derivedFromVideoJobID != nil { continue }
                    if segment.inheritedFirstFrameSourceSegmentID != nil,
                       image.id == segment.confirmedFrameID { continue }
                    guard let asset = mediaAsset(image, projectID: project.id) else { continue }
                    images.append(.init(
                        id: "\(project.id):first:\(segment.id):\(image.id)",
                        title: segment.title, prompt: segment.detail?.firstFramePrompt ?? segment.synopsis,
                        kind: .firstFrame, asset: asset
                    ))
                }
                for image in segment.lastFrames.images where isGenerated(image) {
                    guard let asset = mediaAsset(image, projectID: project.id) else { continue }
                    images.append(.init(
                        id: "\(project.id):last:\(segment.id):\(image.id)",
                        title: segment.title,
                        prompt: image.derivedFromVideoJobID == nil
                            ? segment.detail?.effectiveLastFramePrompt ?? segment.synopsis
                            : segment.synopsis,
                        kind: image.derivedFromVideoJobID == nil ? .lastFrame : .videoLastFrame,
                        asset: asset
                    ))
                }
            }
            var videos: [CreationHistoryVideo] = []
            for (index, segment) in project.segments.enumerated() {
                for video in segment.archivedVideos {
                    guard let fileURL = videoURL(video, projectID: project.id) else { continue }
                    let attempt = segment.previousAttempts.last { $0.jobID == video.jobID }
                    videos.append(.init(
                        id: "\(project.id):archived-video:\(segment.id):\(video.jobID)",
                        segmentID: segment.id, segmentNumber: index + 1, title: segment.title,
                        prompt: attempt?.prompt ?? segment.detail?.videoPrompt ?? segment.synopsis,
                        modelName: video.modelName, createdAt: attempt?.createdAt ?? project.updatedAt,
                        seconds: attempt?.seconds ?? segment.seconds, fileURL: fileURL,
                        isCurrentVersion: false
                    ))
                }
                if let video = segment.video, let fileURL = videoURL(video, projectID: project.id) {
                    videos.append(.init(
                        id: "\(project.id):video:\(segment.id):\(video.jobID)",
                        segmentID: segment.id, segmentNumber: index + 1, title: segment.title,
                        prompt: segment.attempt?.prompt ?? segment.detail?.videoPrompt ?? segment.synopsis,
                        modelName: video.modelName, createdAt: segment.attempt?.createdAt ?? project.updatedAt,
                        seconds: segment.seconds, fileURL: fileURL
                    ))
                }
            }
            guard !images.isEmpty || !videos.isEmpty else { return nil }
            return .init(projectID: project.id, projectTitle: project.title, updatedAt: project.updatedAt,
                         totalSegmentCount: project.segments.count,
                         images: images, videos: videos)
        }
        .sorted { $0.updatedAt > $1.updatedAt }
    }
    func reusableStoryImages(kind: StoryResource.Kind, excluding projectID: UUID) -> [ReusableStoryImage] {
        projects
            .filter { $0.id != projectID }
            .sorted { $0.updatedAt > $1.updatedAt }
            .flatMap { sourceProject in
                sourceProject.resources.compactMap { resource in
                    guard resource.kind == kind,
                          let image = resource.confirmedImage ?? resource.images.last,
                          let asset = mediaAsset(image, projectID: sourceProject.id) else { return nil }
                    return .init(
                        id: "\(sourceProject.id):\(resource.id):\(image.id)",
                        projectID: sourceProject.id, projectTitle: sourceProject.title,
                        resourceID: resource.id, resourceName: resource.name,
                        kind: resource.kind, image: image, asset: asset
                    )
                }
            }
    }
    var hasActiveAssetGenerations: Bool { !activeAssetGenerations.isEmpty }
    var hasActiveFrameGenerations: Bool { !activeFrameGenerations.isEmpty }
    var hasActiveVideoGenerations: Bool { !activeVideoGenerations.isEmpty || videoBatchTask != nil }
    func hasActiveAssetGenerations(projectID: UUID) -> Bool {
        activeAssetGenerations.contains { $0.projectID == projectID }
    }
    func isGeneratingAsset(_ resourceID: String, projectID: UUID) -> Bool {
        activeAssetGenerations.contains(.init(projectID: projectID, resourceID: resourceID))
    }
    func assetGenerationError(_ resourceID: String, projectID: UUID) -> String? {
        assetGenerationErrors[.init(projectID: projectID, resourceID: resourceID)]
    }
    func hasActiveFrameGenerations(projectID: UUID) -> Bool {
        activeFrameGenerations.contains { $0.projectID == projectID }
    }
    func isGeneratingFrame(_ segmentID: String, role: StoryFrameRole, projectID: UUID) -> Bool {
        activeFrameGenerations.contains(.init(projectID: projectID, segmentID: segmentID, role: role))
    }
    func frameGenerationError(_ segmentID: String, role: StoryFrameRole, projectID: UUID) -> String? {
        frameGenerationErrors[.init(projectID: projectID, segmentID: segmentID, role: role)]
    }
    func isGeneratingVideo(_ segmentID: String, projectID: UUID) -> Bool {
        activeVideoGenerations.contains(.init(projectID: projectID, segmentID: segmentID))
    }
    func isExtractingVideoLastFrame(_ segmentID: String, projectID: UUID) -> Bool {
        activeVideoFrameExtractions.contains(.init(projectID: projectID, segmentID: segmentID))
    }
    func videoProgress(_ segmentID: String, projectID: UUID) -> VideoGenerationProgress? {
        videoGenerationProgress[.init(projectID: projectID, segmentID: segmentID)]
    }
    func activeVideoGenerationCount(projectID: UUID) -> Int {
        activeVideoGenerations.filter { $0.projectID == projectID }.count
    }
    private func isGenerated(_ image: StoryImage) -> Bool {
        image.generationAttemptID != nil || image.providerResultID != nil || image.providerAssetID != nil
    }
    func effectiveAgentPolicy() throws -> AgentRunPolicy { try agentSettings.load().effective(.story) }

    /// The canonical project remains unchanged until the agent finishes and the store can apply
    /// the whole run atomically. Only an actively executing run replaces it for live preview;
    /// persisted interrupted drafts remain recoverable without taking over the workbench.
    func presentationProject(for canonical: StoryProject) -> StoryProject {
        presentationAgentRun(for: canonical)?.draft ?? canonical
    }

    func isPresentingAgentDraft(projectID: UUID) -> Bool {
        guard let canonical = projects.first(where: { $0.id == projectID }) else { return false }
        return presentationAgentRun(for: canonical) != nil
    }

    /// A persisted planning draft is recoverable history, not a process-wide lock. After a
    /// logout, crash, or server restart the canonical project remains usable while this run can
    /// be resumed explicitly from the banner.
    func recoverableAgentRun(projectID: UUID) -> StoryAgentRun? {
        guard let canonical = projects.first(where: { $0.id == projectID }),
              let canonicalDigest = try? StoryAgentRun.digest(canonical) else { return nil }
        return agentRuns.first { run in
            guard run.projectID == projectID, !run.applied, run.abandonedAt == nil,
                  (try? StoryAgentRun.matchesPersistedDigest(run.baseDigest, project: canonical)) == true,
                  let draftDigest = try? StoryAgentRun.digest(run.draft) else { return false }
            return draftDigest != canonicalDigest
        }
    }

    private func presentationAgentRun(for canonical: StoryProject) -> StoryAgentRun? {
        // Only show the mutable draft while its in-memory task is actually running. A saved
        // interrupted draft must never replace the normal workbench after a fresh login.
        guard isBusy, activeProjectID == canonical.id, let activeAgentRunID else { return nil }
        guard let canonicalDigest = try? StoryAgentRun.digest(canonical) else { return nil }
        return agentRuns.first { run in
            guard run.id == activeAgentRunID, run.projectID == canonical.id,
                  !run.applied, run.abandonedAt == nil,
                  (try? StoryAgentRun.matchesPersistedDigest(run.baseDigest, project: canonical)) == true,
                  let draftDigest = try? StoryAgentRun.digest(run.draft) else { return false }
            return draftDigest != canonicalDigest
        }
    }

    func activate(userID: String) {
        guard owner != userID else { return }
        reset()
        owner = userID
        let token = session
        isLoading = true
        loadTask = Task {
            do {
                let snapshot = try await store.load(owner: userID)
                guard session == token else { return }
                projects = snapshot.projects
                if snapshot.unreadableCount > 0 { errorMessage = "有 \(snapshot.unreadableCount) 个剧情无法读取，原文件已保留。" }
            } catch {
                guard session == token else { return }
                errorMessage = error.localizedDescription
            }
            isLoading = false
        }
    }

    func reset() {
        session = UUID(); owner = nil
        task?.cancel(); loadTask?.cancel(); task = nil; loadTask = nil
        for generationTask in assetGenerationTasks.values { generationTask.cancel() }
        assetGenerationTasks = [:]; activeAssetGenerations = []; assetGenerationErrors = [:]
        for generationTask in frameGenerationTasks.values { generationTask.cancel() }
        frameGenerationTasks = [:]; activeFrameGenerations = []; frameGenerationErrors = [:]
        for generationTask in videoGenerationTasks.values { generationTask.cancel() }
        videoBatchTask?.cancel(); videoBatchTask = nil
        videoGenerationTasks = [:]; activeVideoGenerations = []; videoGenerationProgress = [:]
        for extractionTask in videoFrameExtractionTasks.values { extractionTask.cancel() }
        videoFrameExtractionTasks = [:]; activeVideoFrameExtractions = []
        historyTask?.cancel(); historyTask = nil; agentRuns = []; mediaBatches = []; isLoadingAgentRuns = false
        projects = []; selectedProjectID = nil; selectedSegmentID = nil; selectedSegments = []
        sourceDrafts = [:]; optimizationSuggestion = nil; optimizationTarget = nil
        isBusy = false; isLoading = false; operation = ""; errorMessage = nil; progress = nil; activeSegmentID = nil; activeProjectID = nil; activeAgentRunID = nil; pauseRequested = false
        streamingModelText = ""; streamingToolName = nil
    }

    func open(_ id: UUID) {
        guard !isBusy || activeProjectID == id else { return }
        selectedProjectID = id; selectedSegmentID = activeSegmentID ?? project?.segments.first?.id; selectedSegments = []
        errorMessage = nil
        loadAgentHistory(id)
    }
    func backToList() { selectedProjectID = nil; selectedSegmentID = nil; selectedSegments = [] }
    func dismissError() { errorMessage = nil }
    func requestPause() { pauseRequested = true }

    func sourceDraft(for project: StoryProject) -> SourceDraft {
        sourceDrafts[project.id] ?? .init(source: project.source, style: project.style, ratio: project.ratio)
    }
    func rememberSourceDraft(projectID: UUID, source: String, style: String, ratio: String) {
        guard projects.contains(where: { $0.id == projectID }) else { return }
        sourceDrafts[projectID] = .init(source: source, style: style, ratio: ratio)
    }

    func optimizeSourceDraft(_ source: String, style: String, target: StoryPlanningTools.OptimizationTarget) {
        guard let project, project.segments.isEmpty else { return }
        optimizationSuggestion = nil; optimizationTarget = nil
        run(target == .source ? "正在优化完整剧情" : "正在优化画面风格") { _, token in
            guard let planner = self.planner else { throw StoryError.unavailable }
            let request = try StoryPlanningTools.optimizationRequest(project, source: source, style: style, target: target)
            let data = try await planner.plan(request)
            let suggestion = try StoryPlanningTools.decodeOptimization(data, target: target)
            try self.check(token)
            self.optimizationTarget = target
            self.optimizationSuggestion = suggestion
        }
    }

    func clearOptimizationSuggestion() {
        optimizationSuggestion = nil; optimizationTarget = nil
    }

    func create(_ draft: StoryProject, availableModels: [MediaGenerationModel]) async -> Bool {
        guard canCreate, let owner else { return false }
        let token = session
        isBusy = true; errorMessage = nil
        defer { if session == token { isBusy = false } }
        do {
            try validateModels(draft.models, available: availableModels)
            var next = draft
            next.models = modelSelectionWithCapabilities(draft.models, available: availableModels)
            try await commit(next, owner: owner, token: token)
            guard session == token else { return false }
            selectedProjectID = next.id; selectedSegmentID = nil; selectedSegments = []
            return true
        } catch {
            if session == token { errorMessage = error.localizedDescription }
            return false
        }
    }

    func updateSettings(_ draft: StoryProject, availableModels: [MediaGenerationModel]) async -> Bool {
        guard !isBusy, activeAssetGenerations.isEmpty, activeFrameGenerations.isEmpty,
              let current = project, let owner, draft.id == current.id else { return false }
        let token = session
        isBusy = true; errorMessage = nil
        defer { if session == token { isBusy = false } }
        do {
            try validateModels(draft.models, available: availableModels)
            // A remote job must remain attached to the model configuration that submitted it.
            guard !current.hasUnresolvedVideoJobs || draft.models.videoModelID == current.models.videoModelID else { throw StoryError.unresolvedSubmission }
            guard !current.hasUnresolvedImageJobs || draft.models.imageModelID == current.models.imageModelID else { throw StoryError.unresolvedSubmission }
            var next = current
            next.title = draft.title.trimmingCharacters(in: .whitespacesAndNewlines)
            next.description = draft.description
            next.models = modelSelectionWithCapabilities(draft.models, available: availableModels)
            try await commit(next, owner: owner, token: token)
            return session == token
        } catch {
            if session == token { errorMessage = error.localizedDescription }
            return false
        }
    }

    func saveSource(_ source: String, style: String, ratio: String) {
        guard var next = project, next.segments.isEmpty else { return }
        next.source = source; next.style = style; next.ratio = ratio
        run("保存剧情") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    func planOutline(userIdeas: String = "") {
        guard let project, project.segments.isEmpty, !project.source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        if supportsAgentPlanning { startAgent(stage: .outline, targets: [], userIdeas: userIdeas); return }
        run("正在分析完整剧情并规划分段") { owner, token in
            guard let planner = self.planner else { throw StoryError.unavailable }
            let request = try StoryPlanningTools.outlineRequest(project, userIdeas: userIdeas)
            let data = try await planner.plan(request)
            let next = try StoryPlanningTools.applyOutline(data, to: project)
            try await self.commit(next, owner: owner, token: token)
            self.selectedSegmentID = next.segments.first?.id
        }
    }

    func refineSegments(_ ids: [String], userIdeas: String = "") {
        guard let project else { return }
        let targets = project.segments.filter {
            ids.contains($0.id) && $0.detail == nil && $0.attempt == nil && $0.video == nil
        }.map(\.id)
        guard !targets.isEmpty else { return }
        if supportsAgentPlanning {
            startAgent(stage: .refine, targets: targets, userIdeas: userIdeas); return
        }
        run("逐段细化镜头计划") { owner, token in
            guard let planner = self.planner else { throw StoryError.unavailable }
            for id in targets {
                try self.check(token)
                if self.pauseRequested { break }
                guard var next = self.projects.first(where: { $0.id == project.id }),
                      let index = next.segments.firstIndex(where: { $0.id == id }), next.segments[index].detail == nil,
                      next.segments[index].attempt == nil, next.segments[index].video == nil else { continue }
                self.operation = "细化第 \(index + 1) / \(next.segments.count) 段"
                let request = try StoryPlanningTools.detailRequest(next, segmentID: id, userIdeas: userIdeas)
                next.segments[index].detail = try StoryPlanningTools.decodeDetail(
                    await planner.plan(request), duration: next.segments[index].seconds
                )
                next.segments[index].confirmedFrameID = nil
                next.segments[index].confirmedLastFrameID = nil
                next.segments[index].useLastFrameForVideo = false
                try await self.commit(next, owner: owner, token: token)
            }
        }
    }

    func regenerateSegmentPlans(_ ids: [String], userIdeas: String = "") {
        guard let project else { return }
        let targets = project.segments.filter {
            ids.contains($0.id) && $0.detail != nil
                && ($0.video != nil || ($0.attempt == nil && $0.video == nil))
        }.map(\.id)
        guard !targets.isEmpty else { return }
        if supportsAgentPlanning {
            startAgent(stage: .refine, targets: targets, userIdeas: userIdeas)
            return
        }
        run("重新生成分段镜头计划") { owner, token in
            guard let planner = self.planner else { throw StoryError.unavailable }
            for id in targets {
                try self.check(token)
                if self.pauseRequested { break }
                guard var next = self.projects.first(where: { $0.id == project.id }),
                      let index = next.segments.firstIndex(where: { $0.id == id }),
                      next.segments[index].detail != nil,
                      next.segments[index].video != nil
                        || (next.segments[index].attempt == nil && next.segments[index].video == nil) else { continue }
                self.operation = "重新生成第 \(index + 1) / \(next.segments.count) 段"
                // This mutation remains local until the new plan succeeds and commits.
                next.segments[index].archiveCompletedVideoForRegeneration()
                let request = try StoryPlanningTools.detailRequest(next, segmentID: id, userIdeas: userIdeas)
                next.segments[index].detail = try StoryPlanningTools.decodeDetail(
                    await planner.plan(request), duration: next.segments[index].seconds
                )
                next.segments[index].confirmedFrameID = nil
                next.segments[index].confirmedLastFrameID = nil
                next.segments[index].useLastFrameForVideo = false
                try await self.commit(next, owner: owner, token: token)
            }
        }
    }

    func saveSegment(_ edited: StorySegment, relations: [StorySegmentRelation]) {
        guard var next = project, let index = next.segments.firstIndex(where: { $0.id == edited.id }),
              next.segments[index].attempt == nil, next.segments[index].video == nil else { return }
        run("保存分段") { owner, token in
            if let detail = edited.detail { try detail.validate(duration: edited.seconds) }
            let original = next.segments[index]
            let originalRelations = next.relations(for: edited.id)
            if original.synopsis != edited.synopsis || original.kind != edited.kind || original.seconds != edited.seconds
                || original.detail?.continuityIn != edited.detail?.continuityIn
                || original.detail?.continuityOut != edited.detail?.continuityOut
                || original.characterIDs != edited.characterIDs || original.sceneIDs != edited.sceneIDs
                || original.propIDs != edited.propIDs || originalRelations != relations {
                for adjacent in [index - 1, index + 1] where next.segments.indices.contains(adjacent) && next.segments[adjacent].attempt == nil {
                    next.segments[adjacent].detail = nil
                    next.segments[adjacent].confirmedFrameID = nil
                    next.segments[adjacent].confirmedLastFrameID = nil
                }
            }
            next.segments[index].title = edited.title
            next.segments[index].synopsis = edited.synopsis
            next.segments[index].kind = edited.kind
            next.segments[index].seconds = edited.seconds
            next.segments[index].detail = edited.detail
            next.segments[index].characterIDs = edited.characterIDs
            next.segments[index].sceneIDs = edited.sceneIDs
            next.segments[index].propIDs = edited.propIDs
            next.segments[index].videoGuidanceMode = edited.videoGuidanceMode
            next.relations.removeAll { $0.segmentID == edited.id }
            next.relations.append(contentsOf: relations)
            next.segments[index].confirmedFrameID = nil
            next.segments[index].confirmedLastFrameID = nil
            try next.validate()
            try await self.commit(next, owner: owner, token: token)
        }
    }

    func setUseLastFrameForVideo(_ enabled: Bool, segmentID: String) {
        setVideoGuidanceMode(enabled ? .firstAndLastFrames : .firstFrame, segmentID: segmentID)
    }

    func setVideoGuidanceMode(_ mode: StoryVideoGuidanceMode, segmentID: String) {
        guard var next = project, let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].attempt == nil, next.segments[index].video == nil,
              next.segments[index].videoGuidanceMode != mode else { return }
        switch mode {
        case .firstFrame:
            break
        case .firstAndLastFrames:
            guard next.segments[index].lastFrame != nil else { return }
        case .previousVideo:
            guard index > 0, next.segments[index - 1].video != nil else { return }
        case .sourceVideo:
            guard next.segments[index].archivedVideos.last != nil else { return }
        }
        next.segments[index].videoGuidanceMode = mode
        run("保存视频衔接方式") { owner, token in
            try await self.commit(next, owner: owner, token: token)
        }
    }

    func adjustSegmentDurationsForVideoModel(_ segmentIDs: Set<String>, model: MediaGenerationModel) {
        guard var next = project, model.id == next.models.videoModelID,
              !segmentIDs.isEmpty else { return }
        let supported = VideoGenerationProfile(modelName: model.modelName).durations.sorted()
        var changed = false
        for index in next.segments.indices where segmentIDs.contains(next.segments[index].id) {
            guard next.segments[index].attempt == nil, next.segments[index].video == nil,
                  next.segments[index].imageGenerationAttemptID == nil,
                  next.segments[index].lastFrameGenerationAttemptID == nil,
                  !supported.contains(next.segments[index].seconds),
                  let adjusted = supported.first(where: { $0 >= next.segments[index].seconds }) else { continue }
            let original = next.segments[index].seconds
            next.segments[index].seconds = adjusted
            if var detail = next.segments[index].detail,
               let lastIndex = detail.shots.indices.last,
               detail.shots[lastIndex].end == original {
                detail.shots[lastIndex].end = adjusted
                next.segments[index].detail = detail
            }
            for relationIndex in next.relations.indices
                where next.relations[relationIndex].segmentID == next.segments[index].id
                    && next.relations[relationIndex].endSecond == original {
                next.relations[relationIndex].endSecond = adjusted
            }
            changed = true
        }
        guard changed else { return }
        next.models.supportedVideoDurations = supported
        run("调整为视频模型支持的时长") { owner, token in
            try await self.commit(next, owner: owner, token: token)
        }
    }

    func addSegment() {
        guard var next = project, !next.hasUnresolvedJobs, next.completedCount == 0 else { return }
        let end = next.source.count
        guard end > 0 else { return }
        next.segments.append(.init(id: UUID().uuidString, title: "新分段", synopsis: "请编辑本段剧情与镜头计划",
                                   sourceRange: .init(start: max(0, end - 1), end: end)))
        invalidatePlans(&next)
        run("添加分段") { owner, token in
            try await self.commit(next, owner: owner, token: token)
            self.selectedSegmentID = next.segments.last?.id
        }
    }

    func moveSegment(_ id: String, offset: Int) {
        guard var next = project, !next.hasUnresolvedJobs, next.completedCount == 0,
              let index = next.segments.firstIndex(where: { $0.id == id }), next.segments.indices.contains(index + offset) else { return }
        next.segments.swapAt(index, index + offset)
        invalidatePlans(&next)
        run("调整剧情顺序") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    func removeSegment(_ id: String) {
        guard var next = project, !next.hasUnresolvedJobs, next.completedCount == 0 else { return }
        next.segments.removeAll { $0.id == id }
        next.relations.removeAll { $0.segmentID == id }
        invalidatePlans(&next)
        run("删除分段") { owner, token in
            try await self.commit(next, owner: owner, token: token)
            self.selectedSegments.remove(id)
            self.selectedSegmentID = next.segments.first?.id
        }
    }

    private func invalidatePlans(_ project: inout StoryProject) {
        for index in project.segments.indices {
            project.segments[index].detail = nil
            project.segments[index].confirmedFrameID = nil
            project.segments[index].confirmedLastFrameID = nil
        }
    }

}
