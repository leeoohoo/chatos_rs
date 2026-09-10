import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Combine
import Foundation

@MainActor
final class StoryStudioViewModel: ObservableObject {
    struct SourceDraft { var source: String; var style: String; var ratio: String }
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
    @Published private(set) var pauseRequested = false
    @Published private(set) var agentRuns: [StoryAgentRun] = []
    @Published private(set) var isLoadingAgentRuns = false
    @Published private(set) var mediaBatches: [StoryMediaBatch] = []
    @Published private(set) var optimizationSuggestion: StoryPlanningTools.OptimizationSuggestion?
    @Published private(set) var optimizationTarget: StoryPlanningTools.OptimizationTarget?
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
    var canCreate: Bool { owner != nil && !isLoading && !isBusy }
    var supportsAgentPlanning: Bool { agentServices != nil }
    var projectAgentRuns: [StoryAgentRun] { agentRuns.filter { $0.projectID == selectedProjectID } }
    var latestAgentRun: StoryAgentRun? { projectAgentRuns.first }
    var projectMediaBatches: [StoryMediaBatch] { mediaBatches.filter { $0.draft.id == selectedProjectID } }
    func effectiveAgentPolicy() throws -> AgentRunPolicy { try agentSettings.load().effective(.story) }

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
        historyTask?.cancel(); historyTask = nil; agentRuns = []; mediaBatches = []; isLoadingAgentRuns = false
        projects = []; selectedProjectID = nil; selectedSegmentID = nil; selectedSegments = []
        sourceDrafts = [:]; optimizationSuggestion = nil; optimizationTarget = nil
        isBusy = false; isLoading = false; operation = ""; errorMessage = nil; progress = nil; activeSegmentID = nil; activeProjectID = nil; pauseRequested = false
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
        guard !isBusy, let current = project, let owner, draft.id == current.id else { return false }
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
        if supportsAgentPlanning {
            let targets = project.segments.filter { ids.contains($0.id) && $0.detail == nil && $0.attempt == nil && $0.video == nil }.map(\.id)
            guard !targets.isEmpty else { return }
            startAgent(stage: .refine, targets: targets); return
        }
        run("逐段细化镜头计划") { owner, token in
            guard let planner = self.planner else { throw StoryError.unavailable }
            for id in ids {
                try self.check(token)
                if self.pauseRequested { break }
                guard var next = self.projects.first(where: { $0.id == project.id }),
                      let index = next.segments.firstIndex(where: { $0.id == id }), next.segments[index].detail == nil,
                      next.segments[index].attempt == nil, next.segments[index].video == nil else { continue }
                self.operation = "细化第 \(index + 1) / \(next.segments.count) 段"
                let request = try StoryPlanningTools.detailRequest(next, segmentID: id)
                next.segments[index].detail = try StoryPlanningTools.decodeDetail(await planner.plan(request))
                try await self.commit(next, owner: owner, token: token)
            }
        }
    }

    func saveSegment(_ edited: StorySegment, relations: [StorySegmentRelation]) {
        guard var next = project, let index = next.segments.firstIndex(where: { $0.id == edited.id }),
              next.segments[index].attempt == nil, next.segments[index].video == nil else { return }
        run("保存分段") { owner, token in
            if let detail = edited.detail { try detail.validate() }
            let original = next.segments[index]
            let originalRelations = next.relations(for: edited.id)
            if original.synopsis != edited.synopsis || original.detail?.continuityIn != edited.detail?.continuityIn
                || original.detail?.continuityOut != edited.detail?.continuityOut
                || original.characterIDs != edited.characterIDs || original.sceneIDs != edited.sceneIDs
                || original.propIDs != edited.propIDs || originalRelations != relations {
                for adjacent in [index - 1, index + 1] where next.segments.indices.contains(adjacent) && next.segments[adjacent].attempt == nil {
                    next.segments[adjacent].detail = nil
                    next.segments[adjacent].confirmedFrameID = nil
                }
            }
            next.segments[index].title = edited.title
            next.segments[index].synopsis = edited.synopsis
            next.segments[index].detail = edited.detail
            next.segments[index].characterIDs = edited.characterIDs
            next.segments[index].sceneIDs = edited.sceneIDs
            next.segments[index].propIDs = edited.propIDs
            next.relations.removeAll { $0.segmentID == edited.id }
            next.relations.append(contentsOf: relations)
            next.segments[index].confirmedFrameID = nil
            try next.validate()
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
        for index in project.segments.indices { project.segments[index].detail = nil; project.segments[index].confirmedFrameID = nil }
    }

    func generateAsset(_ assetID: String) {
        guard let project, let asset = project.resource(id: assetID) else { return }
        guard asset.imageGenerationAttemptID == nil else { errorMessage = StoryError.unresolvedSubmission.localizedDescription; return }
        run("生成素材：\(asset.name)") { owner, token in
            let service = try await self.boundMedia(token)
            guard var intent = self.projects.first(where: { $0.id == project.id }), var resource = intent.resource(id: assetID),
                  resource.media.generationAttemptID == nil else { throw StoryError.unresolvedSubmission }
            resource.media.generationAttemptID = UUID()
            try intent.replaceResource(resource)
            try await self.commit(intent, owner: owner, token: token)
            let result = try await service.generateImage(.init(modelConfigID: project.models.imageModelID,
                prompt: "\(project.style)\n\(asset.prompt)", size: nil, count: 1))
            try self.check(token)
            guard let image = result.images.first else { throw StoryError.unsafeFile }
            try await self.attach(image, assetID: assetID, segmentID: nil, projectID: project.id, owner: owner, token: token,
                                  clearsGenerationAttempt: true)
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
        }
        run("保存素材描述") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    func importImage(_ image: GeneratedMediaAsset, assetID: String?, segmentID: String?) {
        guard let project else { return }
        run("保存参考图片") { owner, token in
            try await self.attach(image, assetID: assetID, segmentID: segmentID, projectID: project.id, owner: owner, token: token)
        }
    }

    func uploadImage(_ url: URL, assetID: String?, segmentID: String?) {
        guard let project else { return }
        run("导入本机图片") { owner, token in
            let data = try await Task.detached {
                let access = url.startAccessingSecurityScopedResource()
                defer { if access { url.stopAccessingSecurityScopedResource() } }
                guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 20 * 1024 * 1024 else { throw StoryError.unsafeFile }
                return try Data(contentsOf: url)
            }.value
            let image = GeneratedMediaAsset(id: UUID().uuidString, mimeType: "image/png", base64Data: data.base64EncodedString())
            try await self.attach(image, assetID: assetID, segmentID: segmentID, projectID: project.id, owner: owner, token: token)
        }
    }

    private func attach(_ image: GeneratedMediaAsset, assetID: String?, segmentID: String?, projectID: UUID, owner: String, token: UUID,
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
            try next.replaceResource(resource)
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }), next.segments[index].attempt == nil {
            next.segments[index].firstFrames.images.append(stored)
            if clearsGenerationAttempt { next.segments[index].firstFrames.generationAttemptID = nil }
        } else { throw StoryError.invalidPlan }
        try await commit(next, owner: owner, token: token)
    }

    func confirmImage(_ image: StoryImage, assetID: String?, segmentID: String?) {
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
            }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }),
                  next.segments[index].attempt == nil, next.segments[index].firstFrames.images.contains(image) {
            next.segments[index].confirmedFrameID = image.id
        } else { return }
        run("确认素材版本") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    /// Explicit user recovery after checking that an ambiguous image submission did not produce a usable result.
    func allowImageRetryAfterVerification(assetID: String?, segmentID: String?) {
        guard var next = project else { return }
        if let assetID, var resource = next.resource(id: assetID), resource.media.generationAttemptID != nil {
            resource.media.generationAttemptID = nil
            do { try next.replaceResource(resource) } catch { errorMessage = error.localizedDescription; return }
        } else if let segmentID, let index = next.segments.firstIndex(where: { $0.id == segmentID }),
                  next.segments[index].firstFrames.generationAttemptID != nil {
            next.segments[index].firstFrames.generationAttemptID = nil
        } else { return }
        run("核对后允许重新生成图片") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    func generateFirstFrame(_ id: String) {
        guard let project, let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil, segment.attempt == nil else { return }
        guard segment.imageGenerationAttemptID == nil else { errorMessage = StoryError.unresolvedSubmission.localizedDescription; return }
        run("生成分段首帧") { owner, token in
            let service = try await self.boundMedia(token)
            var references: [ImageGenerationInputImage] = []
            for assetID in segment.resourceIDs {
                guard let asset = project.resource(id: assetID), let image = asset.confirmedImage else { throw StoryError.invalidPlan }
                let url = try self.store.fileURL(image.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(id: image.id.uuidString, mimeType: image.mimeType, url: url))
                references.append(.init(name: asset.name + ".png", mimeType: image.mimeType, base64Data: data.base64EncodedString()))
            }
            try self.check(token)
            guard var intent = self.projects.first(where: { $0.id == project.id }),
                  let index = intent.segments.firstIndex(where: { $0.id == id }),
                  intent.segments[index].firstFrames.generationAttemptID == nil else { throw StoryError.unresolvedSubmission }
            intent.segments[index].firstFrames.generationAttemptID = UUID()
            try await self.commit(intent, owner: owner, token: token)
            let result = try await service.generateImage(.init(modelConfigID: project.models.imageModelID,
                prompt: StoryGenerationContext.firstFramePrompt(project, segment: segment),
                size: nil, count: 1, referenceImages: references))
            try self.check(token)
            guard let image = result.images.first else { throw StoryError.unsafeFile }
            try await self.attach(image, assetID: nil, segmentID: id, projectID: project.id, owner: owner, token: token,
                                  clearsGenerationAttempt: true)
        }
    }

    func generateBatch(availableModels: [MediaGenerationModel]) {
        guard let project else { return }
        let ids = project.segments.filter { selectedSegments.contains($0.id) && $0.isReady }.map(\.id)
        guard !ids.isEmpty else { return }
        run("批量生成视频") { owner, token in
            let service = try await self.boundMedia(token)
            guard let model = availableModels.first(where: { $0.id == project.models.videoModelID }) else { throw StoryError.missingModel }
            let profile = VideoGenerationProfile(modelName: model.modelName)
            guard profile.durations.contains(15) else { throw StoryError.unsupportedDuration }
            for (position, id) in ids.enumerated() {
                try self.check(token)
                if self.pauseRequested { break }
                guard var next = self.projects.first(where: { $0.id == project.id }),
                      let index = next.segments.firstIndex(where: { $0.id == id }), next.segments[index].isReady,
                      next.segments[index].detail != nil, let frame = next.segments[index].firstFrame else { throw StoryError.invalidPlan }
                self.operation = "生成视频 \(position + 1) / \(ids.count)：\(next.segments[index].title)"
                let url = try self.store.fileURL(frame.filename, projectID: project.id, owner: owner)
                let data = try await MediaStudioImageLoader.data(for: .init(id: frame.id.uuidString, mimeType: frame.mimeType, url: url))
                try self.check(token)
                let prompt = try StoryGenerationContext.videoPrompt(next, segment: next.segments[index])
                let attempt = StoryVideoAttempt(modelConfigID: next.models.videoModelID, prompt: prompt, size: profile.sizes[0], ratio: next.ratio)
                // Commit an intent BEFORE POST. Ambiguous errors must not cause a second charged request.
                next.segments[index].attempt = attempt
                next.segments[index].error = nil
                try await self.commit(next, owner: owner, token: token)
                let request = VideoGenerationRequest(modelConfigID: attempt.modelConfigID, prompt: prompt, size: attempt.size, seconds: 15,
                    inputImage: .init(name: "first-frame.png", mimeType: frame.mimeType, base64Data: data.base64EncodedString()), ratio: attempt.ratio)
                try await self.executeVideo(request, jobID: nil, projectID: project.id, segmentID: id, owner: owner, token: token, service: service)
            }
        }
    }

    func resumeVideo(_ id: String) {
        guard let project, let segment = project.segments.first(where: { $0.id == id }), segment.video == nil, let attempt = segment.attempt else { return }
        guard let jobID = attempt.jobID else { errorMessage = StoryError.unresolvedSubmission.localizedDescription; return }
        run("查询原视频任务，不重新提交") { owner, token in
            let service = try await self.boundMedia(token)
            let request = VideoGenerationRequest(modelConfigID: attempt.modelConfigID, prompt: attempt.prompt,
                                                 size: attempt.size, seconds: 15, ratio: attempt.ratio)
            try await self.executeVideo(request, jobID: jobID, projectID: project.id, segmentID: id, owner: owner, token: token, service: service)
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

    func attachVerifiedJobID(_ jobID: String, segmentID: String) {
        guard var next = project, let index = next.segments.firstIndex(where: { $0.id == segmentID }),
              next.segments[index].attempt?.jobID == nil, next.segments[index].attempt != nil else { return }
        let value = jobID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty, value.count <= 256, !value.contains("/"), !value.contains("?") else { return }
        next.segments[index].attempt?.jobID = value
        run("保存已核实的任务 ID") { owner, token in try await self.commit(next, owner: owner, token: token) }
    }

    private func executeVideo(_ request: VideoGenerationRequest, jobID: String?, projectID: UUID, segmentID: String,
                              owner: String, token: UUID, service: any MediaGenerationServicing) async throws {
        activeSegmentID = segmentID
        selectedSegmentID = segmentID
        progress = .init(status: jobID == nil ? "submitting" : "checking", jobID: jobID)
        let callback: @Sendable (VideoGenerationProgress) async -> Void = { [weak self] progress in
            await self?.recordProgress(progress, projectID: projectID, segmentID: segmentID, owner: owner, token: token)
        }
        do {
            let result: VideoGenerationResult
            if let jobID {
                guard let resumable = service as? any ResumableVideoGenerationServicing else { throw StoryError.unavailable }
                result = try await resumable.resumeVideo(request, jobID: jobID, progress: callback)
            } else { result = try await service.generateVideo(request, progress: callback) }
            try check(token)
            let video = try await store.saveVideo(result, projectID: projectID, owner: owner)
            try check(token)
            guard var next = projects.first(where: { $0.id == projectID }), let index = next.segments.firstIndex(where: { $0.id == segmentID }) else { throw StoryError.invalidPlan }
            next.segments[index].video = video; next.segments[index].attempt?.status = "completed"; next.segments[index].error = nil
            try await commit(next, owner: owner, token: token)
            selectedSegments.remove(segmentID)
        } catch {
            if session == token, var next = projects.first(where: { $0.id == projectID }), let index = next.segments.firstIndex(where: { $0.id == segmentID }) {
                next.segments[index].error = error.localizedDescription
                do { try await commit(next, owner: owner, token: token) }
                catch { errorMessage = "任务记录保存失败，请勿重复提交：\(error.localizedDescription)" }
            }
            throw error
        }
    }

    private func recordProgress(_ value: VideoGenerationProgress, projectID: UUID, segmentID: String, owner: String, token: UUID) async {
        guard session == token, var next = projects.first(where: { $0.id == projectID }), let index = next.segments.firstIndex(where: { $0.id == segmentID }) else { return }
        progress = value
        next.segments[index].attempt?.status = value.status
        if let jobID = value.jobID { next.segments[index].attempt?.jobID = jobID }
        do { try await commit(next, owner: owner, token: token) }
        catch {
            // Keep the returned ID visible even if persisting it failed; the user can recover it.
            if session == token, let index = projects.firstIndex(where: { $0.id == projectID }) { projects[index] = next }
            errorMessage = "保存任务 ID 失败，请保留任务 ID \(value.jobID ?? next.segments[index].attempt?.jobID ?? "未知")，勿重复提交：\(error.localizedDescription)"
            pauseRequested = true; task?.cancel()
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
                for run in result.runs { publishAgentRun(run, token: token) }
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
                                           cloudMemory: true, policy: self.effectiveAgentPolicy())
            try await self.executeAgent(draft, resume: false, owner: owner, token: token)
        }
    }

    func resumeAgent(_ runID: UUID) {
        guard let project else { return }
        run("恢复剧情规划记录") { owner, token in
            let history = try await self.store.loadRuns(owner: owner, projectID: project.id)
            try self.check(token)
            guard let saved = history.runs.first(where: { $0.id == runID }), !saved.applied else { throw StoryAgentError.invalidRun }
            let digest = try StoryAgentRun.digest(project)
            let draftDigest = try StoryAgentRun.digest(saved.draft)
            guard digest == saved.baseDigest || (saved.checkpoint.status == .completed && digest == draftDigest) else { throw StoryAgentError.projectChanged }
            try await self.executeAgent(saved, resume: true, owner: owner, token: token)
        }
    }

    private func executeAgent(_ initial: StoryAgentRun, resume: Bool, owner: String, token: UUID) async throws {
        guard let services = agentServices else { throw StoryAgentError.unavailable }
        try check(token)
        var saved = initial
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
                    contextProvider: context, shouldPause: { [weak self] in
                        await self?.shouldPauseAgent(token) ?? true
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
            for i in batch.draft.segments.indices where batch.draft.segments[i].imageGenerationAttemptID.map(intents.contains) == true { batch.draft.segments[i].imageGenerationAttemptID = nil }
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
        guard !isBusy, !isLoading, let owner else { return }
        let token = session
        isBusy = true; operation = label; errorMessage = nil; pauseRequested = false; progress = nil; activeSegmentID = nil
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
        var next = project; next.updatedAt = Date()
        try await store.save(next, owner: owner)
        try check(token)
        if let index = projects.firstIndex(where: { $0.id == next.id }) { projects[index] = next }
        else { projects.append(next) }
        projects.sort { $0.updatedAt > $1.updatedAt }
    }
}
