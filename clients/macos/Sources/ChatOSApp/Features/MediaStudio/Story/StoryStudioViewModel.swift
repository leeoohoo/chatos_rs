import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Combine
import Foundation

@MainActor
final class StoryStudioViewModel: ObservableObject {
    struct SourceDraft { var source: String; var style: String; var ratio: String }
    struct AssetGenerationKey: Hashable, Sendable {
        var projectID: UUID
        var resourceID: String
    }
    struct VideoGenerationKey: Hashable, Sendable {
        var projectID: UUID
        var segmentID: String
    }
    @Published private(set) var projects: [StoryProject] = []
    @Published var selectedProjectID: UUID?
    @Published var selectedSegmentID: String?
    @Published var selectedSegments: Set<String> = []
    @Published private(set) var isLoading = false
    @Published private(set) var isBusy = false
    @Published private(set) var operation = ""
    @Published private(set) var errorMessage: String?
    @Published private(set) var progress: VideoGenerationProgress?
    @Published private(set) var activeSegmentID: String?
    @Published private(set) var activeProjectID: UUID?
    @Published private(set) var activeAgentRunID: UUID?
    @Published private(set) var pauseRequested = false
    @Published private(set) var agentRuns: [StoryAgentRun] = []
    @Published private(set) var isLoadingAgentRuns = false
    @Published private(set) var mediaBatches: [StoryMediaBatch] = []
    @Published private(set) var optimizationSuggestion: StoryPlanningTools.OptimizationSuggestion?
    @Published private(set) var optimizationTarget: StoryPlanningTools.OptimizationTarget?
    @Published private(set) var streamingModelText = ""
    @Published private(set) var streamingToolName: String?
    @Published private(set) var activeAssetGenerations: Set<AssetGenerationKey> = []
    @Published private(set) var assetGenerationErrors: [AssetGenerationKey: String] = [:]
    @Published private(set) var activeVideoGenerations: Set<VideoGenerationKey> = []
    @Published private(set) var videoGenerationProgress: [VideoGenerationKey: VideoGenerationProgress] = [:]
    private let store: StoryProjectStore
    private let media: any MediaGenerationServicing
    private let planner: (any StoryPlanningServicing)?
    private let agentServices: (any AgentServiceProviding)?
    private let agentSettings: AgentSettingsStore
    private var owner: String?
    private var session = UUID()
    private var task: Task<Void, Never>?
    private var loadTask: Task<Void, Never>?
    private var historyTask: Task<Void, Never>?
    private var assetGenerationTasks: [AssetGenerationKey: Task<Void, Never>] = [:]
    private var videoGenerationTasks: [VideoGenerationKey: Task<Void, Never>] = [:]
    private var sourceDrafts: [UUID: SourceDraft] = [:]

    init(media: any MediaGenerationServicing, planner: (any StoryPlanningServicing)? = nil,
         store: StoryProjectStore = StoryProjectStore(), agentServices: (any AgentServiceProviding)? = nil,
         agentSettings: AgentSettingsStore = .init()) {
        self.media = media; self.planner = planner; self.store = store
        self.agentServices = agentServices ?? (planner as? any AgentServiceProviding)
        self.agentSettings = agentSettings
    }

    var project: StoryProject? { projects.first { $0.id == selectedProjectID } }
    var segment: StorySegment? { project?.segments.first { $0.id == selectedSegmentID } }
    var canCreate: Bool { owner != nil && !isLoading && !isBusy && activeAssetGenerations.isEmpty }
    var supportsAgentPlanning: Bool { agentServices != nil }
    var projectAgentRuns: [StoryAgentRun] { agentRuns.filter { $0.projectID == selectedProjectID } }
    var latestAgentRun: StoryAgentRun? { projectAgentRuns.first }
    var projectMediaBatches: [StoryMediaBatch] { mediaBatches.filter { $0.draft.id == selectedProjectID } }
    var hasActiveAssetGenerations: Bool { !activeAssetGenerations.isEmpty }
    var hasActiveVideoGenerations: Bool { !activeVideoGenerations.isEmpty }
    func hasActiveAssetGenerations(projectID: UUID) -> Bool {
        activeAssetGenerations.contains { $0.projectID == projectID }
    }
    func isGeneratingAsset(_ resourceID: String, projectID: UUID) -> Bool {
        activeAssetGenerations.contains(.init(projectID: projectID, resourceID: resourceID))
    }
    func assetGenerationError(_ resourceID: String, projectID: UUID) -> String? {
        assetGenerationErrors[.init(projectID: projectID, resourceID: resourceID)]
    }
    func isGeneratingVideo(_ segmentID: String, projectID: UUID) -> Bool {
        activeVideoGenerations.contains(.init(projectID: projectID, segmentID: segmentID))
    }
    func videoProgress(_ segmentID: String, projectID: UUID) -> VideoGenerationProgress? {
        videoGenerationProgress[.init(projectID: projectID, segmentID: segmentID)]
    }
    func activeVideoGenerationCount(projectID: UUID) -> Int {
        activeVideoGenerations.filter { $0.projectID == projectID }.count
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
                  run.baseDigest == canonicalDigest,
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
                  run.baseDigest == canonicalDigest,
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
        for generationTask in videoGenerationTasks.values { generationTask.cancel() }
        videoGenerationTasks = [:]; activeVideoGenerations = []; videoGenerationProgress = [:]
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
            try await commit(draft, owner: owner, token: token)
            guard session == token else { return false }
            selectedProjectID = draft.id; selectedSegmentID = nil; selectedSegments = []
            return true
        } catch {
            if session == token { errorMessage = error.localizedDescription }
            return false
        }
    }

    func updateSettings(_ draft: StoryProject, availableModels: [MediaGenerationModel]) async -> Bool {
        guard !isBusy, activeAssetGenerations.isEmpty, let current = project, let owner, draft.id == current.id else { return false }
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
            next.description = draft.description; next.models = draft.models
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

    func planOutline() {
        guard let project, project.segments.isEmpty, !project.source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        if supportsAgentPlanning { startAgent(stage: .outline, targets: []); return }
        run("正在分析完整剧情并规划分段") { owner, token in
            guard let planner = self.planner else { throw StoryError.unavailable }
            let request = try StoryPlanningTools.outlineRequest(project)
            let data = try await planner.plan(request)
            let next = try StoryPlanningTools.applyOutline(data, to: project)
            try await self.commit(next, owner: owner, token: token)
            self.selectedSegmentID = next.segments.first?.id
        }
    }

    func refineSegments(_ ids: [String]) {
        guard let project else { return }
        let targets = project.segments.filter {
            ids.contains($0.id) && $0.detail == nil && $0.attempt == nil && $0.video == nil
        }.map(\.id)
        guard !targets.isEmpty else { return }
        if supportsAgentPlanning {
            startAgent(stage: .refine, targets: targets); return
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
                let request = try StoryPlanningTools.detailRequest(next, segmentID: id)
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

    func regenerateSegmentPlans(_ ids: [String]) {
        guard let project else { return }
        let targets = project.segments.filter {
            ids.contains($0.id) && $0.detail != nil && $0.attempt == nil && $0.video == nil
        }.map(\.id)
        guard !targets.isEmpty else { return }
        if supportsAgentPlanning {
            startAgent(stage: .refine, targets: targets)
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
                      next.segments[index].attempt == nil, next.segments[index].video == nil else { continue }
                self.operation = "重新生成第 \(index + 1) / \(next.segments.count) 段"
                let request = try StoryPlanningTools.detailRequest(next, segmentID: id)
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
            next.segments[index].useLastFrameForVideo = edited.useLastFrameForVideo
            next.relations.removeAll { $0.segmentID == edited.id }
            next.relations.append(contentsOf: relations)
            next.segments[index].confirmedFrameID = nil
            next.segments[index].confirmedLastFrameID = nil
            try next.validate()
            try await self.commit(next, owner: owner, token: token)
        }
    }

    func setUseLastFrameForVideo(_ enabled: Bool, segmentID: String) {
        guard var next = project, let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].attempt == nil, next.segments[index].video == nil,
              next.segments[index].useLastFrameForVideo != enabled else { return }
        next.segments[index].useLastFrameForVideo = enabled
        run("保存尾帧视频设置") { owner, token in
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

    func generateAsset(_ assetID: String) {
        guard !isBusy, !isLoading, let owner, let project, let asset = project.resource(id: assetID) else { return }
        let key = AssetGenerationKey(projectID: project.id, resourceID: assetID)
        guard !activeAssetGenerations.contains(key) else { return }
        guard asset.imageGenerationAttemptID == nil else { errorMessage = StoryError.unresolvedSubmission.localizedDescription; return }
        let token = session
        let attemptID = UUID()
        activeAssetGenerations.insert(key)
        assetGenerationErrors[key] = nil
        let generationTask = Task { [weak self] in
            guard let self else { return }
            await self.performAssetGeneration(key: key, attemptID: attemptID, owner: owner, token: token)
        }
        assetGenerationTasks[key] = generationTask
    }

    private func performAssetGeneration(key: AssetGenerationKey, attemptID: UUID, owner: String, token: UUID) async {
        defer {
            if session == token {
                activeAssetGenerations.remove(key)
                assetGenerationTasks[key] = nil
            }
        }
        do {
            try check(token)
            let intent = try await store.beginAssetImageGeneration(projectID: key.projectID,
                                                                    resourceID: key.resourceID,
                                                                    attemptID: attemptID, owner: owner)
            try check(token)
            publishProject(intent.project, token: token)
            let service = try await boundMedia(token)
            let result = try await service.generateImage(.init(
                modelConfigID: intent.project.models.imageModelID,
                prompt: StoryGenerationContext.assetPrompt(intent.project, resource: intent.resource), size: nil, count: 1,
                clientRequestID: attemptID.uuidString, projectID: key.projectID.uuidString,
                resourceID: key.resourceID
            ))
            try check(token)
            guard result.clientRequestID == nil || result.clientRequestID == attemptID.uuidString,
                  result.projectID == nil || result.projectID == key.projectID.uuidString,
                  result.resourceID == nil || result.resourceID == key.resourceID,
                  let generated = result.images.first else { throw StoryError.invalidPlan }
            let data = try await MediaStudioImageLoader.data(for: generated)
            try check(token)
            guard let nsImage = NSImage(data: data), let tiff = nsImage.tiffRepresentation,
                  let bitmap = NSBitmapImageRep(data: tiff),
                  let png = bitmap.representation(using: .png, properties: [:]) else { throw StoryError.unsafeFile }
            let completed = try await store.completeAssetImageGeneration(
                png, mimeType: "image/png", projectID: key.projectID, resourceID: key.resourceID,
                attemptID: attemptID, providerResultID: result.id, providerAssetID: generated.id, owner: owner
            )
            try check(token)
            publishProject(completed.0, token: token)
        } catch is CancellationError {
            // The durable attempt remains unresolved if submission may have reached the provider.
        } catch {
            guard session == token else { return }
            assetGenerationErrors[key] = error.localizedDescription
        }
    }

    func updateAssetPrompt(_ assetID: String, prompt: String) {
        guard var next = project, var resource = next.resource(id: assetID),
              !next.segments.contains(where: { $0.resourceIDs.contains(assetID) && $0.attempt != nil && $0.video == nil }) else { return }
        resource.prompt = prompt
        resource.media.confirmedImageID = nil
        do { try next.replaceResource(resource) } catch { errorMessage = error.localizedDescription; return }
        for i in next.segments.indices where next.segments[i].resourceIDs.contains(assetID) && next.segments[i].attempt == nil {
            next.segments[i].confirmedFrameID = nil
            next.segments[i].confirmedLastFrameID = nil
        }
        run("保存素材描述") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    func importImage(_ image: GeneratedMediaAsset, assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first) {
        guard let project else { return }
        run("保存参考图片") { owner, token in
            try await self.attach(image, assetID: assetID, segmentID: segmentID, frameRole: frameRole,
                                  projectID: project.id, owner: owner, token: token)
        }
    }

    func uploadImage(_ url: URL, assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first) {
        guard let project else { return }
        run("导入本机图片") { owner, token in
            let data = try await Task.detached {
                let access = url.startAccessingSecurityScopedResource()
                defer { if access { url.stopAccessingSecurityScopedResource() } }
                guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 20 * 1024 * 1024 else { throw StoryError.unsafeFile }
                return try Data(contentsOf: url)
            }.value
            let image = GeneratedMediaAsset(id: UUID().uuidString, mimeType: "image/png", base64Data: data.base64EncodedString())
            try await self.attach(image, assetID: assetID, segmentID: segmentID, frameRole: frameRole,
                                  projectID: project.id, owner: owner, token: token)
        }
    }

    private func attach(_ image: GeneratedMediaAsset, assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first,
                        projectID: UUID, owner: String, token: UUID,
                        clearsGenerationAttempt: Bool = false) async throws {
        let data = try await MediaStudioImageLoader.data(for: image)
        try check(token)
        guard let nsImage = NSImage(data: data), let tiff = nsImage.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff), let png = bitmap.representation(using: .png, properties: [:]) else { throw StoryError.unsafeFile }
        let stored = try await store.saveImage(png, mimeType: "image/png", projectID: projectID, owner: owner)
        try check(token)
        guard var next = projects.first(where: { $0.id == projectID }) else { throw StoryError.invalidProject }
        if let assetID, var resource = next.resource(id: assetID) {
            resource.media.images.append(stored)
            if clearsGenerationAttempt { resource.media.generationAttemptID = nil }
            let automaticallyConfirmed = resource.confirmedImageID == nil
            if automaticallyConfirmed { resource.media.confirmedImageID = stored.id }
            try next.replaceResource(resource)
            if automaticallyConfirmed {
                for index in next.segments.indices
                    where next.segments[index].resourceIDs.contains(assetID)
                        && next.segments[index].attempt == nil {
                    next.segments[index].confirmedFrameID = nil
                    next.segments[index].confirmedLastFrameID = nil
                }
            }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }), next.segments[index].attempt == nil {
            switch frameRole {
            case .first:
                next.segments[index].firstFrames.images.append(stored)
                if clearsGenerationAttempt { next.segments[index].firstFrames.generationAttemptID = nil }
                if next.segments[index].confirmedFrameID == nil {
                    next.segments[index].confirmedFrameID = stored.id
                    next.segments[index].inheritedFirstFrameSourceSegmentID = nil
                }
            case .last:
                next.segments[index].lastFrames.images.append(stored)
                if clearsGenerationAttempt { next.segments[index].lastFrames.generationAttemptID = nil }
                if next.segments[index].confirmedLastFrameID == nil {
                    next.segments[index].confirmedLastFrameID = stored.id
                }
            }
        } else { throw StoryError.invalidPlan }
        try await commit(next, owner: owner, token: token)
    }

    func confirmImage(_ image: StoryImage, assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first) {
        guard var next = project else { return }
        if let assetID, var resource = next.resource(id: assetID), resource.images.contains(image) {
            guard resource.confirmedImageID != image.id else { return }
            guard !next.segments.contains(where: { $0.resourceIDs.contains(assetID) && $0.attempt != nil && $0.video == nil }) else {
                errorMessage = StoryError.unresolvedSubmission.localizedDescription; return
            }
            resource.media.confirmedImageID = image.id
            do { try next.replaceResource(resource) } catch { errorMessage = error.localizedDescription; return }
            for i in next.segments.indices where next.segments[i].resourceIDs.contains(assetID) && next.segments[i].attempt == nil {
                next.segments[i].confirmedFrameID = nil
                next.segments[i].confirmedLastFrameID = nil
            }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }), next.segments[index].attempt == nil {
            switch frameRole {
            case .first:
                guard next.segments[index].firstFrames.images.contains(image) else { return }
                next.segments[index].confirmedFrameID = image.id
                next.segments[index].inheritedFirstFrameSourceSegmentID = nil
            case .last:
                guard next.segments[index].lastFrames.images.contains(image) else { return }
                next.segments[index].confirmedLastFrameID = image.id
            }
        } else { return }
        run("确认素材版本") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    func confirmLatestAssetImages(_ assetIDs: [String]) {
        guard var next = project else { return }
        let requested = Set(assetIDs)
        guard !requested.isEmpty else { return }
        var changed = false
        for id in next.resources.map(\.id) where requested.contains(id) {
            guard var resource = next.resource(id: id), let latest = resource.images.last,
                  resource.confirmedImageID != latest.id else { continue }
            guard !next.segments.contains(where: {
                $0.resourceIDs.contains(id) && $0.attempt != nil && $0.video == nil
            }) else {
                errorMessage = StoryError.unresolvedSubmission.localizedDescription
                return
            }
            resource.media.confirmedImageID = latest.id
            do { try next.replaceResource(resource) } catch {
                errorMessage = error.localizedDescription
                return
            }
            for index in next.segments.indices
                where next.segments[index].resourceIDs.contains(id) && next.segments[index].attempt == nil {
                next.segments[index].confirmedFrameID = nil
                next.segments[index].confirmedLastFrameID = nil
            }
            changed = true
        }
        guard changed else { return }
        run("确认已生成素材") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    /// Promotes already-generated frame versions in one explicit, non-billable action.
    /// Existing confirmations are never replaced; only missing confirmations use the latest image.
    func confirmLatestFrames(_ segmentID: String, useConfirmedLastFrameForVideo: Bool) {
        guard var next = project,
              let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].attempt == nil, next.segments[index].video == nil else { return }
        var changed = false
        if next.segments[index].confirmedFrameID == nil,
                  let latest = next.segments[index].firstFrames.images.last {
            next.segments[index].confirmedFrameID = latest.id
            next.segments[index].inheritedFirstFrameSourceSegmentID = nil
            changed = true
        }
        if next.segments[index].confirmedLastFrameID == nil,
           let latest = next.segments[index].lastFrames.images.last {
            next.segments[index].confirmedLastFrameID = latest.id
            changed = true
        }
        if useConfirmedLastFrameForVideo,
           next.segments[index].confirmedLastFrameID != nil,
           !next.segments[index].useLastFrameForVideo {
            next.segments[index].useLastFrameForVideo = true
            changed = true
        }
        guard changed else { return }
        run("确认首尾帧") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    /// Explicit user recovery after checking that an ambiguous image submission did not produce a usable result.
    func allowImageRetryAfterVerification(assetID: String?, segmentID: String?, frameRole: StoryFrameRole = .first) {
        guard var next = project else { return }
        if let assetID, var resource = next.resource(id: assetID), resource.media.generationAttemptID != nil {
            resource.media.generationAttemptID = nil
            do { try next.replaceResource(resource) } catch { errorMessage = error.localizedDescription; return }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }) {
            switch frameRole {
            case .first:
                guard next.segments[index].firstFrames.generationAttemptID != nil else { return }
                next.segments[index].firstFrames.generationAttemptID = nil
            case .last:
                guard next.segments[index].lastFrames.generationAttemptID != nil else { return }
                next.segments[index].lastFrames.generationAttemptID = nil
            }
        } else { return }
        run("核对后允许重新生成图片") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    func generateFirstFrame(_ id: String) {
        guard let segment = project?.segments.first(where: { $0.id == id }) else { return }
        generateFirstFrame(id, referenceAssetIDs: segment.resourceIDs)
    }

    func generateFirstFrame(_ id: String, referenceAssetIDs requestedReferenceAssetIDs: [String]) {
        generateFrame(id, role: .first, referenceAssetIDs: requestedReferenceAssetIDs)
    }

    func generateLastFrame(_ id: String) {
        guard let segment = project?.segments.first(where: { $0.id == id }) else { return }
        generateFrame(id, role: .last, referenceAssetIDs: segment.resourceIDs)
    }

    func generateFrame(_ id: String, role: StoryFrameRole, referenceAssetIDs requestedReferenceAssetIDs: [String]) {
        guard let project, let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil, segment.attempt == nil else { return }
        let referenceAssetIDs = segment.resourceIDs.filter(requestedReferenceAssetIDs.contains)
        guard !referenceAssetIDs.isEmpty, Set(referenceAssetIDs) == Set(requestedReferenceAssetIDs) else {
            errorMessage = "请选择至少一个属于当前分段且已确认图片的素材。"
            return
        }
        let frameAttempt = role == .first ? segment.firstFrames.generationAttemptID : segment.lastFrames.generationAttemptID
        guard frameAttempt == nil else { errorMessage = StoryError.unresolvedSubmission.localizedDescription; return }
        run(role == .first ? "生成分段首帧" : "生成分段尾帧") { owner, token in
            let service = try await self.boundMedia(token)
            var references: [ImageGenerationInputImage] = []
            for assetID in referenceAssetIDs {
                guard let asset = project.resource(id: assetID), let image = asset.confirmedImage else { throw StoryError.invalidPlan }
                let url = try self.store.fileURL(image.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(id: image.id.uuidString, mimeType: image.mimeType, url: url))
                references.append(.init(name: asset.name + ".png", mimeType: image.mimeType, base64Data: data.base64EncodedString()))
            }
            var previousTailReferenceIndex: Int?
            var currentFirstFrameReferenceIndex: Int?
            if role == .first, let previous = StoryContinuityContext.previousTail(project, segmentID: id) {
                let url = try self.store.fileURL(previous.image.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(
                    id: previous.image.id.uuidString, mimeType: previous.image.mimeType, url: url
                ))
                references.append(.init(name: "previous-segment-last-frame.png",
                                        mimeType: previous.image.mimeType,
                                        base64Data: data.base64EncodedString()))
                previousTailReferenceIndex = references.count
            } else if role == .last, let firstFrame = segment.firstFrame {
                let url = try self.store.fileURL(firstFrame.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(
                    id: firstFrame.id.uuidString, mimeType: firstFrame.mimeType, url: url
                ))
                references.append(.init(name: "current-segment-first-frame.png",
                                        mimeType: firstFrame.mimeType,
                                        base64Data: data.base64EncodedString()))
                currentFirstFrameReferenceIndex = references.count
            }
            try self.check(token)
            let attemptID = UUID()
            let intent = try await self.store.beginFrameImageGeneration(projectID: project.id, segmentID: id,
                                                                         role: role, attemptID: attemptID, owner: owner)
            try self.check(token)
            self.publishProject(intent.project, token: token)
            let result = try await service.generateImage(.init(modelConfigID: project.models.imageModelID,
                prompt: StoryGenerationContext.framePrompt(intent.project, segment: intent.segment, role: role,
                                                           referenceResourceIDs: referenceAssetIDs,
                                                           previousTailReferenceIndex: previousTailReferenceIndex,
                                                           currentFirstFrameReferenceIndex: currentFirstFrameReferenceIndex),
                size: nil, count: 1, referenceImages: references,
                clientRequestID: attemptID.uuidString, projectID: project.id.uuidString,
                resourceID: "\(id):\(role.rawValue)"))
            try self.check(token)
            guard result.clientRequestID == nil || result.clientRequestID == attemptID.uuidString,
                  result.projectID == nil || result.projectID == project.id.uuidString,
                  result.resourceID == nil || result.resourceID == "\(id):\(role.rawValue)" else { throw StoryError.invalidPlan }
            guard let image = result.images.first else { throw StoryError.unsafeFile }
            let data = try await MediaStudioImageLoader.data(for: image)
            guard let nsImage = NSImage(data: data), let tiff = nsImage.tiffRepresentation,
                  let bitmap = NSBitmapImageRep(data: tiff),
                  let png = bitmap.representation(using: .png, properties: [:]) else { throw StoryError.unsafeFile }
            let completed = try await self.store.completeFrameImageGeneration(
                png, mimeType: "image/png", projectID: project.id, segmentID: id, role: role,
                attemptID: attemptID, providerResultID: result.id, providerAssetID: image.id, owner: owner
            )
            try self.check(token)
            self.publishProject(completed.0, token: token)
        }
    }

    func generateBatch(availableModels: [MediaGenerationModel]) {
        guard !isBusy, !isLoading, let owner, let project else { return }
        let ids = project.segments.filter { selectedSegments.contains($0.id) && $0.isReady }.map(\.id)
        guard !ids.isEmpty else { return }
        guard let model = availableModels.first(where: { $0.id == project.models.videoModelID }) else {
            errorMessage = StoryError.missingModel.localizedDescription
            return
        }
        let profile = VideoGenerationProfile(modelName: model.modelName)
        let unsupported = project.segments.filter {
            ids.contains($0.id) && !profile.durations.contains($0.seconds)
        }
        guard unsupported.isEmpty, let size = profile.sizes.first else {
            let durations = profile.durations.map(String.init).joined(separator: "、")
            errorMessage = "当前视频模型不支持所选分段时长。模型支持：\(durations) 秒；不支持："
                + unsupported.map { "\($0.title)（\($0.seconds)秒）" }.joined(separator: "、")
            return
        }
        errorMessage = nil
        let token = session
        for id in ids {
            startVideoGeneration(project: project, segmentID: id, model: model, size: size,
                                 owner: owner, token: token)
        }
    }

    private func startVideoGeneration(project: StoryProject, segmentID: String,
                                      model: MediaGenerationModel, size: String,
                                      owner: String, token: UUID) {
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              segment.isReady else { return }
        let key = VideoGenerationKey(projectID: project.id, segmentID: segmentID)
        guard !activeVideoGenerations.contains(key) else { return }
        let prompt: String
        do { prompt = try StoryGenerationContext.videoPrompt(project, segment: segment) }
        catch { errorMessage = error.localizedDescription; return }
        let attempt = StoryVideoAttempt(
            modelConfigID: project.models.videoModelID,
            prompt: prompt,
            size: size,
            ratio: project.ratio,
            seconds: segment.seconds
        )
        launchVideoGeneration(key: key, attemptID: attempt.id, owner: owner, token: token) { service in
            // The store actor merges this intent into the latest manifest before the billable POST.
            let intent = try await self.store.beginVideoGeneration(
                projectID: project.id, segmentID: segmentID, attempt: attempt, owner: owner
            )
            try self.check(token)
            self.publishProject(intent.project, token: token)

            guard let frame = intent.segment.firstFrame else { throw StoryError.invalidPlan }
            let firstURL = try self.store.fileURL(frame.filename, projectID: project.id, owner: owner)
            let firstData = try await MediaStudioImageLoader.data(for: .init(
                id: frame.id.uuidString, mimeType: frame.mimeType, url: firstURL
            ))
            var lastFrameInput: ImageGenerationInputImage?
            if model.supportsVideoLastFrame, intent.segment.useLastFrameForVideo,
               let lastFrame = intent.segment.lastFrame {
                let lastURL = try self.store.fileURL(lastFrame.filename, projectID: project.id, owner: owner)
                let lastData = try await MediaStudioImageLoader.data(for: .init(
                    id: lastFrame.id.uuidString, mimeType: lastFrame.mimeType, url: lastURL
                ))
                lastFrameInput = .init(
                    name: "last-frame.png", mimeType: lastFrame.mimeType,
                    base64Data: lastData.base64EncodedString()
                )
            }
            try self.check(token)
            let request = VideoGenerationRequest(
                modelConfigID: attempt.modelConfigID,
                prompt: attempt.prompt,
                size: attempt.size,
                seconds: attempt.seconds,
                inputImage: .init(
                    name: "first-frame.png", mimeType: frame.mimeType,
                    base64Data: firstData.base64EncodedString()
                ),
                lastFrameImage: lastFrameInput,
                ratio: attempt.ratio
            )
            try await self.executeParallelVideo(
                request, jobID: nil, projectID: project.id, segmentID: segmentID,
                attemptID: attempt.id, owner: owner, token: token, service: service
            )
        }
    }

    private func launchVideoGeneration(
        key: VideoGenerationKey,
        attemptID: UUID,
        owner: String,
        token: UUID,
        operation: @escaping @MainActor (any MediaGenerationServicing) async throws -> Void
    ) {
        guard !activeVideoGenerations.contains(key) else { return }
        activeVideoGenerations.insert(key)
        videoGenerationProgress[key] = .init(status: "preparing")
        videoGenerationTasks[key] = Task { [weak self] in
            guard let self else { return }
            defer {
                if self.session == token {
                    self.activeVideoGenerations.remove(key)
                    self.videoGenerationProgress[key] = nil
                    self.videoGenerationTasks[key] = nil
                }
            }
            do {
                let service = try await self.boundMedia(token)
                try await operation(service)
            } catch is CancellationError {
                // A submitted attempt stays recoverable and is never automatically re-posted.
            } catch {
                guard self.session == token else { return }
                do {
                    let failed: StoryProject
                    if let failure = error as? any MediaGenerationSubmissionFailure,
                       !failure.requestMayHaveBeenSubmitted {
                        failed = try await self.store.failVideoGenerationBeforeSubmission(
                            error.localizedDescription, projectID: key.projectID,
                            segmentID: key.segmentID, attemptID: attemptID, owner: owner
                        )
                    } else {
                        failed = try await self.store.failVideoGeneration(
                            error.localizedDescription, projectID: key.projectID,
                            segmentID: key.segmentID, attemptID: attemptID, owner: owner
                        )
                    }
                    self.publishProject(failed, token: token)
                } catch {
                    self.errorMessage = error.localizedDescription
                }
            }
        }
    }

    private func executeParallelVideo(
        _ request: VideoGenerationRequest,
        jobID: String?,
        projectID: UUID,
        segmentID: String,
        attemptID: UUID,
        owner: String,
        token: UUID,
        service: any MediaGenerationServicing
    ) async throws {
        let key = VideoGenerationKey(projectID: projectID, segmentID: segmentID)
        videoGenerationProgress[key] = .init(
            status: jobID == nil ? "submitting" : "checking",
            jobID: jobID
        )
        let callback: @Sendable (VideoGenerationProgress) async -> Void = { [weak self] value in
            await self?.recordParallelVideoProgress(
                value, projectID: projectID, segmentID: segmentID,
                attemptID: attemptID, owner: owner, token: token
            )
        }
        let result: VideoGenerationResult
        if let jobID {
            guard let resumable = service as? any ResumableVideoGenerationServicing else {
                throw StoryError.unavailable
            }
            result = try await resumable.resumeVideo(request, jobID: jobID, progress: callback)
        } else {
            result = try await service.generateVideo(request, progress: callback)
        }
        try check(token)
        let completed = try await store.completeVideoGeneration(
            result, projectID: projectID, segmentID: segmentID,
            attemptID: attemptID, owner: owner
        )
        try check(token)
        publishProject(completed.0, token: token)
        selectedSegments.remove(segmentID)
    }

    private func recordParallelVideoProgress(
        _ value: VideoGenerationProgress,
        projectID: UUID,
        segmentID: String,
        attemptID: UUID,
        owner: String,
        token: UUID
    ) async {
        guard session == token else { return }
        let key = VideoGenerationKey(projectID: projectID, segmentID: segmentID)
        videoGenerationProgress[key] = value
        do {
            let updated = try await store.updateVideoGenerationProgress(
                value, projectID: projectID, segmentID: segmentID,
                attemptID: attemptID, owner: owner
            )
            try check(token)
            publishProject(updated, token: token)
        } catch {
            guard session == token else { return }
            errorMessage = "保存任务 ID 失败，请保留任务 ID \(value.jobID ?? "未知")，勿重复提交：\(error.localizedDescription)"
            videoGenerationTasks[key]?.cancel()
        }
    }

    func resumeVideo(_ id: String) {
        guard !isBusy, !isLoading, let owner, let project,
              let segment = project.segments.first(where: { $0.id == id }),
              segment.video == nil, let attempt = segment.attempt else { return }
        guard let jobID = attempt.jobID else {
            errorMessage = StoryError.unresolvedSubmission.localizedDescription
            return
        }
        let key = VideoGenerationKey(projectID: project.id, segmentID: id)
        guard !activeVideoGenerations.contains(key) else { return }
        errorMessage = nil
        let token = session
        launchVideoGeneration(key: key, attemptID: attempt.id, owner: owner, token: token) { service in
            let request = VideoGenerationRequest(
                modelConfigID: attempt.modelConfigID, prompt: attempt.prompt,
                size: attempt.size, seconds: attempt.seconds, ratio: attempt.ratio
            )
            try await self.executeParallelVideo(
                request, jobID: jobID, projectID: project.id, segmentID: id,
                attemptID: attempt.id, owner: owner, token: token, service: service
            )
        }
    }

    /// Only called after an explicit user confirmation, never by the planning model.
    func allowRetryAfterVerification(_ id: String) {
        guard var next = project, let index = next.segments.firstIndex(where: { $0.id == id }),
              next.segments[index].video == nil, let attempt = next.segments[index].attempt else { return }
        next.segments[index].previousAttempts.append(attempt)
        next.segments[index].attempt = nil
        next.segments[index].error = nil
        run("保留旧任务记录并允许手动重试") { owner, token in
            try await self.commit(next, owner: owner, token: token)
        }
    }

    func mediaAsset(_ image: StoryImage, projectID: UUID) -> GeneratedMediaAsset? {
        guard let owner, let url = try? store.fileURL(image.filename, projectID: projectID, owner: owner) else { return nil }
        return .init(id: image.id.uuidString, mimeType: image.mimeType, url: url)
    }
    func videoURL(_ video: StoryVideo, projectID: UUID) -> URL? {
        guard let owner else { return nil }
        return try? store.fileURL(video.filename, projectID: projectID, owner: owner)
    }
    private func validateModels(_ selection: StoryModelSelection, available: [MediaGenerationModel]) throws {
        let ids = Set(available.filter { $0.enabled && $0.hasAPIKey }.map(\.id))
        guard [selection.textModelID, selection.imageModelID, selection.videoModelID].allSatisfy(ids.contains) else { throw StoryError.missingModel }
    }

    private func loadAgentHistory(_ projectID: UUID) {
        guard let owner else { return }
        let token = session
        historyTask?.cancel(); isLoadingAgentRuns = true
        historyTask = Task {
            defer { if session == token, selectedProjectID == projectID { isLoadingAgentRuns = false } }
            do {
                let result = try await store.loadRuns(owner: owner, projectID: projectID)
                let batches = try await store.loadMediaBatches(owner: owner, projectID: projectID)
                guard session == token, selectedProjectID == projectID, !Task.isCancelled else { return }
                var runs = result.runs
                if let canonical = projects.first(where: { $0.id == projectID }) {
                    let canonicalDigest = try StoryAgentRun.digest(canonical)
                    if let candidateIndex = runs.firstIndex(where: {
                        !$0.applied && $0.abandonedAt == nil
                            && ($0.baseDigest == canonicalDigest || (try? StoryAgentRun.digest($0.draft)) == canonicalDigest)
                    }), let recovered = try await store.applyValidatedRunIfPossible(runs[candidateIndex], owner: owner) {
                        runs[candidateIndex] = recovered.0
                        if let projectIndex = projects.firstIndex(where: { $0.id == projectID }) {
                            projects[projectIndex] = recovered.1
                        }
                    }
                }
                for run in runs { publishAgentRun(run, token: token) }
                for batch in batches.batches { publishMediaBatch(batch, token: token, updateProject: false) }
                if result.unreadable > 0 { errorMessage = "有 \(result.unreadable) 条规划运行记录无法读取，原文件已保留。" }
                if batches.unreadable > 0 { errorMessage = "有 \(batches.unreadable) 条制作批次无法读取，原文件已保留。" }
            } catch { if session == token { errorMessage = error.localizedDescription } }
        }
    }

    private func publishAgentRun(_ value: StoryAgentRun, token: UUID) {
        guard session == token, owner == value.owner else { return }
        if let index = agentRuns.firstIndex(where: { $0.id == value.id }) {
            if agentRuns[index].updatedAt <= value.updatedAt { agentRuns[index] = value }
        } else { agentRuns.append(value) }
        agentRuns.sort { $0.updatedAt > $1.updatedAt }
        if isBusy, activeProjectID == value.projectID, let event = value.events.last { operation = event.detail }
    }

    private func startAgent(stage: StoryAgentRun.Stage, targets: [String]) {
        guard let project else { return }
        run("启动分步剧情规划") { owner, token in
            let draft = try StoryAgentRun(project: project, owner: owner, stage: stage, targetIDs: targets,
                                           policy: self.effectiveAgentPolicy())
            try await self.executeAgent(draft, resume: false, owner: owner, token: token)
        }
    }

    func resumeAgent(_ runID: UUID) {
        guard let project else { return }
        run("恢复剧情规划记录") { owner, token in
            let history = try await self.store.loadRuns(owner: owner, projectID: project.id)
            try self.check(token)
            guard let saved = history.runs.first(where: { $0.id == runID }),
                  !saved.applied, saved.abandonedAt == nil else { throw StoryAgentError.invalidRun }
            let digest = try StoryAgentRun.digest(project)
            let draftDigest = try StoryAgentRun.digest(saved.draft)
            guard digest == saved.baseDigest || (saved.checkpoint.status == .completed && digest == draftDigest) else { throw StoryAgentError.projectChanged }
            try await self.executeAgent(saved, resume: true, owner: owner, token: token)
        }
    }

    func abandonAgent(_ runID: UUID) {
        guard let project else { return }
        run("放弃中断的剧情规划草稿") { owner, token in
            let history = try await self.store.loadRuns(owner: owner, projectID: project.id)
            try self.check(token)
            guard var saved = history.runs.first(where: { $0.id == runID }),
                  !saved.applied, saved.abandonedAt == nil else { throw StoryAgentError.invalidRun }
            saved.abandonedAt = Date()
            saved.checkpoint.stopReason = "用户已放弃这份中断草稿；正式项目未被修改。"
            saved.events.append(.init(kind: "abandoned", detail: "用户放弃中断草稿，正式项目保持不变",
                                      modelCalls: saved.checkpoint.modelCalls))
            saved.updatedAt = Date()
            try await self.store.saveRun(saved, owner: owner)
            try self.check(token)
            self.publishAgentRun(saved, token: token)
        }
    }

    private func executeAgent(_ initial: StoryAgentRun, resume: Bool, owner: String, token: UUID) async throws {
        guard let services = agentServices else { throw StoryAgentError.unavailable }
        try check(token)
        var saved = initial
        activeAgentRunID = saved.id
        defer {
            if session == token, activeAgentRunID == initial.id { activeAgentRunID = nil }
        }
        var context: AgentMemoryContextProvider?
        if saved.checkpoint.status != .completed {
            let scope = try AgentMemoryScope(tenantID: owner, profile: "story", projectID: saved.projectID,
                                            runID: saved.id, runtimeScope: saved.checkpoint.scope)
            let memory = try await services.makeAgentMemory(scope: scope)
            try check(token)
            let provider = AgentMemoryContextProvider(scope: scope, service: memory)
            saved.checkpoint = try provider.bind(saved.checkpoint)
            context = provider
        }
        try await store.saveRun(saved, owner: owner)
        publishAgentRun(saved, token: token)
        let coordinator = StoryAgentSession(run: saved, store: store) { [weak self] snapshot in
            await self?.publishAgentRun(snapshot, token: token)
        }
        do {
            if resume { saved = try await coordinator.prepareForResume(policy: effectiveAgentPolicy()) }
            if saved.checkpoint.status != .completed {
                let model = try await services.makeAgentModel(configID: saved.draft.models.textModelID, policy: saved.policy)
                try check(token)
                let result = try await AgentRuntime().run(checkpoint: saved.checkpoint, scope: saved.checkpoint.scope, policy: saved.policy,
                    model: model, tools: StoryAgentTools.definitions(stage: saved.stage), execute: { call in try await coordinator.execute(call) },
                    completionCheck: { await coordinator.validatedCompletion() },
                    contextProvider: context, shouldPause: { [weak self] in
                        await self?.shouldPauseAgent(token) ?? true
                    }, onModelStreamEvent: { [weak self] event in
                        await self?.receiveModelStream(event, token: token)
                    }, record: { checkpoint, event in try await coordinator.record(checkpoint, event: event) })
                saved = try await coordinator.finish(result)
            }
            try check(token)
            if saved.checkpoint.status == .completed {
                let (applied, project) = try await store.applyRun(saved, owner: owner)
                try check(token)
                if let index = projects.firstIndex(where: { $0.id == project.id }) { projects[index] = project }
                publishAgentRun(applied, token: token)
                if selectedSegmentID == nil { selectedSegmentID = project.segments.first?.id }
            } else if let reason = saved.checkpoint.stopReason { errorMessage = reason }
        } catch {
            try await coordinator.abort(error.localizedDescription)
            throw error
        }
    }

    private func shouldPauseAgent(_ token: UUID) -> Bool { session != token || pauseRequested }

    private func receiveModelStream(_ event: AgentModelStreamEvent, token: UUID) {
        guard session == token else { return }
        switch event {
        case .responseCreated:
            streamingModelText = ""; streamingToolName = nil
            operation = "文本模型正在流式分析…"
        case let .textDelta(delta):
            streamingModelText += delta
            if streamingModelText.count > 12_000 { streamingModelText.removeFirst(streamingModelText.count - 12_000) }
            operation = "文本模型正在流式输出…"
        case let .toolCallDelta(_, _, name, _):
            if let name, !name.isEmpty { streamingToolName = (streamingToolName ?? "") + name }
            operation = "正在组装工具调用：\(streamingToolName ?? "参数")"
        case .completed:
            operation = streamingToolName.map { "已接收完整工具调用：\($0)" } ?? "本轮流式响应已完成"
        }
    }

    func previewMediaBatch(kind: StoryMediaBatch.Kind, targets: [String], models: [MediaGenerationModel]) throws -> StoryMediaBatch {
        guard let project, let owner else { throw StoryError.invalidProject }
        return try .init(project: project, owner: owner, kind: kind, targets: targets, models: models)
    }
    func startMediaBatch(_ batch: StoryMediaBatch) {
        guard let project, batch.draft.id == project.id else { return }
        run("准备批量制作") { owner, token in
            guard owner == batch.owner, try StoryAgentRun.digest(project) == batch.expectedProjectDigest else { throw StoryAgentError.projectChanged }
            let existing = try await self.store.loadMediaBatches(owner: owner, projectID: project.id)
            guard existing.unreadable == 0, !existing.batches.contains(where: { !$0.finished }) else { throw StoryBatchError.activeBatch }
            try await self.executeMediaBatch(batch, token: token)
        }
    }
    func resumeMediaBatch(_ id: UUID) {
        guard let project else { return }
        run("恢复原制作批次") { owner, token in
            let saved = try await self.store.loadMediaBatches(owner: owner, projectID: project.id)
            guard let batch = saved.batches.first(where: { $0.id == id }), !batch.finished else { throw StoryAgentError.invalidRun }
            try await self.executeMediaBatch(batch, token: token)
        }
    }
    /// Explicit UI confirmation only. Unresolved video intents are retained for the existing
    /// verified-job recovery UI; only this batch's image locks can be released.
    func abandonMediaBatch(_ id: UUID) {
        guard let project else { return }
        run("结束已核对的制作批次") { owner, token in
            let history = try await self.store.loadMediaBatches(owner: owner, projectID: project.id)
            guard var batch = history.batches.first(where: { $0.id == id }), !batch.finished else { throw StoryAgentError.invalidRun }
            try self.check(token)
            batch.draft = project; batch.expectedProjectDigest = try StoryAgentRun.digest(project)
            let intents = Set(batch.jobs.values.map(\.intentID))
            for var resource in batch.draft.resources where resource.imageGenerationAttemptID.map(intents.contains) == true {
                resource.media.generationAttemptID = nil
                try batch.draft.replaceResource(resource)
            }
            for i in batch.draft.segments.indices {
                if batch.draft.segments[i].imageGenerationAttemptID.map(intents.contains) == true {
                    batch.draft.segments[i].imageGenerationAttemptID = nil
                }
                if batch.draft.segments[i].lastFrameGenerationAttemptID.map(intents.contains) == true {
                    batch.draft.segments[i].lastFrameGenerationAttemptID = nil
                }
            }
            batch.status = .abandoned; batch.updatedAt = Date()
            batch.events.append(.init(detail: "用户核对后结束本批，保留已有结果及视频任务"))
            try await self.store.commitMediaBatch(batch)
            self.publishMediaBatch(batch, token: token)
        }
    }
    private func executeMediaBatch(_ batch: StoryMediaBatch, token: UUID) async throws {
        try check(token)
        let service = try await boundMedia(token)
        let coordinator = StoryMediaBatchSession(batch: batch, store: store, media: service,
            check: { [weak self] in guard let self else { throw CancellationError() }; try await self.check(token) },
            shouldPause: { [weak self] in await self?.shouldPauseAgent(token) ?? true },
            publish: { [weak self] snapshot in await self?.publishMediaBatch(snapshot, token: token) })
        let result = try await coordinator.run()
        try check(token)
        if let error = result.error { errorMessage = error }
    }
    private func boundMedia(_ token: UUID) async throws -> any MediaGenerationServicing {
        try check(token)
        let service: any MediaGenerationServicing
        if let bindable = media as? any SessionBoundMediaGenerationServicing {
            service = try await bindable.boundToCurrentSession()
        } else { service = media }
        try check(token)
        return service
    }
    private func publishMediaBatch(_ batch: StoryMediaBatch, token: UUID, updateProject: Bool = true) {
        guard session == token, owner == batch.owner else { return }
        if let index = mediaBatches.firstIndex(where: { $0.id == batch.id }) { mediaBatches[index] = batch }
        else { mediaBatches.append(batch) }
        mediaBatches.sort { $0.updatedAt > $1.updatedAt }
        if updateProject, let index = projects.firstIndex(where: { $0.id == batch.draft.id }) { projects[index] = batch.draft }
        if isBusy, activeProjectID == batch.draft.id {
            operation = "\(batch.completedCount) / \(batch.steps.count) · \(batch.events.last?.detail ?? "准备制作")"
            if let step = batch.steps.first(where: { batch.jobs[$0.id] != nil && batch.jobs[$0.id]?.completed != true }), step.kind == .videos {
                activeSegmentID = step.targetID; selectedSegmentID = step.targetID
                progress = .init(status: batch.draft.segments.first { $0.id == step.targetID }?.attempt?.status ?? "submitting", jobID: batch.jobs[step.id]?.jobID)
            }
        }
    }
    private func run(_ label: String, body: @escaping @MainActor (String, UUID) async throws -> Void) {
        guard !isBusy, !isLoading, activeAssetGenerations.isEmpty, let owner else {
            if !activeAssetGenerations.isEmpty {
                errorMessage = "图片正在并行生成。生成期间可以继续启动其它素材，但项目结构与模型设置暂时不能修改。"
            }
            return
        }
        let token = session
        isBusy = true; operation = label; errorMessage = nil; pauseRequested = false; progress = nil; activeSegmentID = nil
        streamingModelText = ""; streamingToolName = nil
        activeProjectID = selectedProjectID
        task = Task {
            do { try await body(owner, token) }
            catch { if session == token { errorMessage = error.localizedDescription } }
            guard session == token else { return }
            isBusy = false; operation = ""; task = nil; progress = nil; activeSegmentID = nil; activeProjectID = nil
        }
    }
    private func check(_ token: UUID) throws {
        try Task.checkCancellation()
        guard session == token else { throw CancellationError() }
    }
    private func commit(_ project: StoryProject, owner: String, token: UUID) async throws {
        try check(token)
        var next = project
        StoryContinuityContext.reconcileInheritedFirstFrames(&next)
        next.updatedAt = Date()
        let activeSegmentIDs = Set(activeVideoGenerations.lazy
            .filter { $0.projectID == next.id }
            .map(\.segmentID))
        next = try await store.save(
            next, owner: owner, preservingVideoSegmentIDs: activeSegmentIDs
        )
        try check(token)
        if let index = projects.firstIndex(where: { $0.id == next.id }) { projects[index] = next }
        else { projects.append(next) }
        projects.sort { $0.updatedAt > $1.updatedAt }
    }
    private func publishProject(_ project: StoryProject, token: UUID) {
        guard session == token else { return }
        if let index = projects.firstIndex(where: { $0.id == project.id }) { projects[index] = project }
        else { projects.append(project) }
        projects.sort { $0.updatedAt > $1.updatedAt }
    }
}
