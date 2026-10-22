import ChatOSAPI
import ChatOSCore
import Combine
import Foundation
import UniformTypeIdentifiers

@MainActor
final class MediaStudioViewModel: ObservableObject {
    enum Section: String, CaseIterable, Identifiable {
        case image
        case video
        case story
        case history

        var id: Self { self }
    }

    typealias HistoryItem = MediaStudioImageHistoryItem
    typealias VideoHistoryItem = MediaStudioVideoHistoryItem

    enum VideoCanvasState: Equatable {
        case progress(VideoGenerationProgress)
        case failed
        case result(VideoHistoryItem)
        case empty
    }

    var videoCanvasState: VideoCanvasState {
        if isGeneratingVideo { return .progress(videoProgress ?? .init(status: "submitting")) }
        if let status = videoProgress?.status,
           ["failed", "submission_uncertain", "cancellation_requested"].contains(status) {
            return .failed
        }
        if let latest = videoHistory.first { return .result(latest) }
        return .empty
    }

    @Published var section: Section = .image
    @Published var prompt = ""
    @Published var selectedModelID: String?
    @Published var size = "1024x1024"
    @Published var count = 1
    @Published var videoPrompt = ""
    @Published var selectedVideoModelID: String? {
        didSet { normalizeVideoOptions() }
    }
    @Published var videoSize = "1280x720"
    @Published var videoRatio = "16:9"
    @Published var videoSeconds = 4
    @Published private(set) var inputImages: [ImageGenerationInputImage] = []
    @Published private(set) var videoInputImage: ImageGenerationInputImage?
    @Published private(set) var videoReferenceAudio: VideoGenerationInputAudio?
    @Published private(set) var models: [MediaGenerationModel] = []
    @Published private(set) var videoModels: [MediaGenerationModel] = []
    @Published private(set) var history: [HistoryItem] = []
    @Published private(set) var videoHistory: [VideoHistoryItem] = []
    @Published private(set) var isLoadingModels = false
    @Published private(set) var isGenerating = false
    @Published private(set) var isGeneratingVideo = false
    @Published private(set) var isRecoveringVideoJob = false
    @Published private(set) var videoProgress: VideoGenerationProgress?
    @Published private(set) var errorMessage: String?
    @Published private(set) var historyErrorMessage: String?
    @Published private(set) var isLoadingHistory = false
    @Published private(set) var isLoadingMoreHistory = false
    @Published private(set) var hasMoreHistory = false
    @Published private(set) var isLoadingVideoInputImage = false
    @Published private(set) var isLoadingVideoReferenceAudio = false
    @Published private(set) var isLoadingInputImages = false

    private let service: any MediaGenerationServicing
    private let imageGenerationUseCase: MediaStudioImageGenerationUseCase
    private let videoJobService: MediaStudioVideoJobService
    private let modelCatalogUseCase: MediaStudioModelCatalogUseCase
    private let historyUseCase: MediaStudioHistoryUseCase
    private let imageReferenceUseCase: MediaStudioImageReferenceUseCase
    private let imageReferenceImporter: any MediaStudioImageReferenceImporting
    private let inputArtifacts: MediaStudioInputArtifactStore
    let stories: StoryStudioViewModel
    private var hasLoaded = false
    private var hasLoadedHistory = false
    private var videoGenerationTask: Task<Void, Never>?
    private var imageGenerationTask: Task<Void, Never>?
    private var historyLoadTask: Task<Void, Never>?
    private var historyCursor: MediaStudioHistoryCursor?
    private var unreadableHistoryCount = 0
    private var historyLoadState = MediaStudioHistoryLoadState()
    private var modelsLoadTask: Task<Void, Never>?
    private var modelCatalogState = MediaStudioModelCatalogState()
    private var ownerID: String?
    private var sessionID = UUID()
    private var imageGenerationState = MediaStudioImageGenerationState()
    private var videoGenerationState = MediaStudioVideoGenerationState()
    private var hasAttemptedVideoRecovery = false
    private var videoInputSelectionID = UUID()
    private var videoInputTask: Task<Void, Never>?
    private var videoAudioSelectionID = UUID()
    private var videoAudioTask: Task<Void, Never>?
    private var inputImagesTask: Task<Void, Never>?
    private var imageReferenceState = MediaStudioImageReferenceState()

    init(
        service: any MediaGenerationServicing,
        historyStore: MediaStudioHistoryStore = MediaStudioHistoryStore(),
        videoJobStore: MediaStudioVideoJobStore = MediaStudioVideoJobStore(),
        imageTransport: any HTTPTransport = URLSessionHTTPTransport(),
        storyPlanner: (any StoryPlanningServicing)? = nil
    ) {
        let inputArtifacts = MediaStudioInputArtifactStore()
        self.service = service
        self.inputArtifacts = inputArtifacts
        self.imageGenerationUseCase = MediaStudioImageGenerationUseCase(
            generator: service,
            history: historyStore
        )
        self.videoJobService = MediaStudioVideoJobService(
            generator: service,
            history: historyStore,
            store: videoJobStore
        )
        self.modelCatalogUseCase = MediaStudioModelCatalogUseCase(provider: service)
        self.historyUseCase = MediaStudioHistoryUseCase(history: historyStore)
        let imageReferenceImporter = MediaStudioImageReferenceImporter(
            transport: imageTransport,
            artifacts: inputArtifacts
        )
        self.imageReferenceImporter = imageReferenceImporter
        self.imageReferenceUseCase = MediaStudioImageReferenceUseCase(
            importer: imageReferenceImporter
        )
        self.stories = StoryStudioViewModel(
            media: service,
            planner: storyPlanner,
            videoJobStore: videoJobStore
        )
    }

    deinit {
        modelsLoadTask?.cancel()
        historyLoadTask?.cancel()
        imageGenerationTask?.cancel()
        videoGenerationTask?.cancel()
        inputImagesTask?.cancel()
        videoInputTask?.cancel()
        videoAudioTask?.cancel()
        inputArtifacts.removeAll()
    }

    /// Authentication only establishes ownership. Catalogs and disk history are
    /// loaded by the visible workspace, not by the account bootstrap path.
    func bindAccount(userID: String) {
        guard ownerID != userID else { return }
        // A newly constructed studio is already in its signed-out baseline. Avoid
        // publishing dozens of redundant resets during authentication bootstrap;
        // besides unnecessary UI churn, Release Combine has crashed while lazily
        // installing the nested StoryStudio publisher on that first no-op reset.
        if ownerID != nil {
            resetForSignedOut()
        }
        ownerID = userID
    }

    func activate(userID: String) {
        bindAccount(userID: userID)
        loadHistoryIfNeeded()
        if !isLoadingHistory { recoverVideoJobIfNeeded() }
        loadSelectedSectionIfNeeded()
    }

    private func loadHistoryIfNeeded() {
        guard let userID = ownerID, !hasLoadedHistory else { return }
        hasLoadedHistory = true
        isLoadingHistory = true
        let token = historyLoadState.begin(
            ownerUserID: userID,
            accountSessionID: sessionID,
            kind: .initial
        )
        let useCase = historyUseCase
        historyLoadTask = Task { [weak self] in
            let result: Result<MediaStudioHistoryPage, Error>
            do {
                result = .success(try await useCase.loadInitial(owner: userID))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled else { return }
            self?.finishHistoryLoad(result, token: token)
        }
    }

    func loadMoreHistory() {
        guard let ownerID, let cursor = historyCursor,
              !isLoadingHistory, !isLoadingMoreHistory else { return }
        let token = historyLoadState.begin(
            ownerUserID: ownerID,
            accountSessionID: sessionID,
            kind: .next(cursor)
        )
        let useCase = historyUseCase
        isLoadingMoreHistory = true
        historyLoadTask = Task { [weak self] in
            let result: Result<MediaStudioHistoryPage, Error>
            do {
                result = .success(try await useCase.loadNext(owner: ownerID, after: cursor))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled else { return }
            self?.finishHistoryLoad(result, token: token)
        }
    }

    private func finishHistoryLoad(
        _ result: Result<MediaStudioHistoryPage, Error>,
        token: MediaStudioHistoryLoadToken
    ) {
        guard historyLoadState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ) else { return }
        switch result {
        case let .success(page):
            let collection: MediaStudioHistoryCollection
            switch token.kind {
            case .initial:
                collection = historyUseCase.replacing(with: page)
            case .next:
                collection = historyUseCase.merging(page, into: currentHistoryCollection())
            }
            applyHistoryCollection(collection)
        case let .failure(error):
            switch token.kind {
            case .initial:
                historyErrorMessage = "读取创作记录失败：\(error.localizedDescription)"
            case .next:
                historyErrorMessage = "读取更多创作记录失败：\(error.localizedDescription)"
            }
        }
        guard historyLoadState.finish(token) else { return }
        switch token.kind {
        case .initial:
            isLoadingHistory = false
            recoverVideoJobIfNeeded()
        case .next: isLoadingMoreHistory = false
        }
        historyLoadTask = nil
    }

    private func currentHistoryCollection() -> MediaStudioHistoryCollection {
        MediaStudioHistoryCollection(
            images: history,
            videos: videoHistory,
            unreadableCount: unreadableHistoryCount,
            nextCursor: historyCursor
        )
    }

    private func applyHistoryCollection(_ collection: MediaStudioHistoryCollection) {
        history = collection.images
        videoHistory = collection.videos
        historyCursor = collection.nextCursor
        hasMoreHistory = collection.nextCursor != nil
        unreadableHistoryCount = collection.unreadableCount
        updateHistoryReadError()
    }

    private func updateHistoryReadError() {
        historyErrorMessage = unreadableHistoryCount > 0
            ? "有 \(unreadableHistoryCount) 条记录无法读取，原文件已保留。"
            : nil
    }

    var selectedModel: MediaGenerationModel? {
        guard let selectedModelID else { return nil }
        return models.first(where: { $0.id == selectedModelID })
    }

    var inputImage: ImageGenerationInputImage? { inputImages.first }

    var canGenerate: Bool {
        !isGenerating && !isLoadingInputImages
            && ownerID != nil && !isLoadingHistory
            && selectedModelID != nil
            && !prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var selectedVideoModel: MediaGenerationModel? {
        guard let selectedVideoModelID else { return nil }
        return videoModels.first(where: { $0.id == selectedVideoModelID })
    }

    var canGenerateVideo: Bool {
        !isGeneratingVideo
            && !isRecoveringVideoJob
            && !isLoadingVideoInputImage
            && !isLoadingVideoReferenceAudio
            && ownerID != nil && !isLoadingHistory
            && selectedVideoModelID != nil
            && !videoPrompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var videoProfile: VideoGenerationProfile {
        VideoGenerationProfile(modelName: selectedVideoModel?.modelName ?? "")
    }

    private func normalizeVideoOptions() {
        if !videoProfile.sizes.contains(videoSize) { videoSize = videoProfile.sizes[0] }
        if !videoProfile.durations.contains(videoSeconds) { videoSeconds = videoProfile.durations[0] }
    }

    func loadIfNeeded() {
        guard ownerID != nil else { return }
        loadHistoryIfNeeded()
        loadSelectedSectionIfNeeded()
        guard !hasLoaded else { return }
        hasLoaded = true
        loadModels()
    }

    func loadSelectedSectionIfNeeded() {
        guard let ownerID else { return }
        switch section {
        case .story, .history:
            stories.activate(userID: ownerID)
        case .image, .video:
            break
        }
    }

    func reloadModels() {
        guard ownerID != nil else { return }
        hasLoaded = true
        loadModels()
    }

    func generate() {
        guard canGenerate, let selectedModelID, let ownerID else { return }
        let session = sessionID
        let token = imageGenerationState.begin(
            ownerUserID: ownerID,
            accountSessionID: session
        )
        let submittedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let request = ImageGenerationRequest(
            modelConfigID: selectedModelID, prompt: submittedPrompt,
            size: size == "auto" ? nil : size, count: count,
            referenceImages: inputImages
        )
        isGenerating = true
        errorMessage = nil
        let useCase = imageGenerationUseCase
        imageGenerationTask?.cancel()
        imageGenerationTask = Task { [weak self] in
            let outcome: Result<MediaStudioImageGenerationOutcome, Error>
            do {
                outcome = .success(try await useCase.generate(request: request, owner: ownerID))
            } catch {
                outcome = .failure(error)
            }
            guard !Task.isCancelled else { return }
            self?.finishImageGeneration(
                outcome,
                prompt: submittedPrompt,
                token: token
            )
        }
    }

    private func finishImageGeneration(
        _ result: Result<MediaStudioImageGenerationOutcome, Error>,
        prompt: String,
        token: MediaStudioImageGenerationToken
    ) {
        guard imageGenerationState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ) else { return }
        switch result {
        case let .success(outcome):
            switch outcome {
            case let .saved(item):
                history.insert(item, at: 0)
            case let .generatedWithoutPersistence(result, message):
                // Keep the generated result visible even when the disk/download fails.
                history.insert(
                    HistoryItem(
                        id: result.id,
                        prompt: prompt,
                        modelName: result.modelName,
                        createdAt: ISO8601DateFormatter().date(from: result.createdAt) ?? Date(),
                        images: result.images
                    ),
                    at: 0
                )
                historyErrorMessage = "图片已生成，但未能保存到本机：\(message)"
                errorMessage = historyErrorMessage
            }
        case let .failure(error):
            errorMessage = error.localizedDescription
        }
        guard imageGenerationState.finish(token) else { return }
        isGenerating = false
        imageGenerationTask = nil
    }

    func generateVideo() {
        guard canGenerateVideo, let selectedVideoModelID, let ownerID else { return }
        let token = videoGenerationState.begin(
            ownerUserID: ownerID,
            accountSessionID: sessionID
        )
        let submittedPrompt = videoPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let request = VideoGenerationRequest(
            modelConfigID: selectedVideoModelID, prompt: submittedPrompt,
            size: videoSize, seconds: videoSeconds, inputImage: videoInputImage,
            referenceAudio: videoReferenceAudio, ratio: videoRatio
        )
        isGeneratingVideo = true
        videoProgress = .init(status: "submitting")
        errorMessage = nil
        let jobService = videoJobService
        let sessionEpoch = sessionID
        videoGenerationTask?.cancel()
        videoGenerationTask = Task { [weak self] in
            let outcome: Result<MediaStudioVideoGenerationOutcome, Error>
            do {
                outcome = .success(try await jobService.submit(
                    request: request,
                    owner: ownerID,
                    sessionEpoch: sessionEpoch
                ) { [weak self] progress in
                    await MainActor.run {
                        self?.applyVideoProgress(progress, token: token)
                    }
                })
            } catch is CancellationError {
                guard !Task.isCancelled else { return }
                self?.finishCancelledVideoGeneration(token: token)
                return
            } catch {
                outcome = .failure(error)
            }
            self?.finishVideoGeneration(outcome, token: token)
        }
    }

    private func recoverVideoJobIfNeeded() {
        guard !hasAttemptedVideoRecovery,
              let ownerID,
              videoGenerationTask == nil else { return }
        hasAttemptedVideoRecovery = true
        let token = videoGenerationState.begin(
            ownerUserID: ownerID,
            accountSessionID: sessionID
        )
        let sessionEpoch = sessionID
        let jobService = videoJobService
        isRecoveringVideoJob = true
        videoGenerationTask = Task { [weak self] in
            do {
                let result = try await jobService.recover(
                    owner: ownerID,
                    sessionEpoch: sessionEpoch
                ) { [weak self] progress in
                    await MainActor.run {
                        self?.applyVideoProgress(progress, token: token)
                    }
                }
                switch result {
                case .none:
                    self?.finishEmptyVideoRecovery(token: token)
                case let .needsReview(progress, message):
                    self?.finishVideoRecoveryReview(
                        progress: progress,
                        message: message,
                        token: token
                    )
                case let .finished(outcome):
                    self?.finishVideoGeneration(.success(outcome), token: token)
                }
            } catch is CancellationError {
                return
            } catch {
                self?.finishVideoGeneration(.failure(error), token: token)
            }
        }
    }

    private func finishEmptyVideoRecovery(token: MediaStudioVideoGenerationToken) {
        guard videoGenerationState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ), videoGenerationState.finish(token) else { return }
        isGeneratingVideo = false
        isRecoveringVideoJob = false
        videoProgress = nil
        videoGenerationTask = nil
    }

    private func finishVideoRecoveryReview(
        progress: VideoGenerationProgress,
        message: String,
        token: MediaStudioVideoGenerationToken
    ) {
        guard videoGenerationState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ), videoGenerationState.finish(token) else { return }
        isGeneratingVideo = false
        isRecoveringVideoJob = false
        videoProgress = progress
        errorMessage = message
        videoGenerationTask = nil
    }

    private func applyVideoProgress(
        _ progress: VideoGenerationProgress,
        token: MediaStudioVideoGenerationToken
    ) {
        guard videoGenerationState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ) else { return }
        isRecoveringVideoJob = false
        isGeneratingVideo = true
        videoProgress = progress
    }

    private func finishVideoGeneration(
        _ result: Result<MediaStudioVideoGenerationOutcome, Error>,
        token: MediaStudioVideoGenerationToken
    ) {
        guard videoGenerationState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ) else { return }
        switch result {
        case let .success(outcome):
            switch outcome {
            case let .saved(item):
                videoHistory.insert(item, at: 0)
                videoProgress = .init(status: "completed", percent: 100)
            case let .generatedWithoutPersistence(message):
                historyErrorMessage = "视频已生成，但未能保存到本机：\(message)"
                errorMessage = historyErrorMessage
                videoProgress = .init(status: "failed")
            }
        case let .failure(error):
            errorMessage = error.localizedDescription
            if let jobError = error as? MediaStudioVideoJobServiceError,
               jobError == .unresolvedSubmission {
                videoProgress = .init(status: "submission_uncertain")
            } else {
                videoProgress = .init(status: "failed")
            }
        }
        guard videoGenerationState.finish(token) else { return }
        isGeneratingVideo = false
        isRecoveringVideoJob = false
        videoGenerationTask = nil
    }

    private func finishCancelledVideoGeneration(token: MediaStudioVideoGenerationToken) {
        guard videoGenerationState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ) else { return }
        guard videoGenerationState.finish(token) else { return }
        videoProgress = nil
        isGeneratingVideo = false
        isRecoveringVideoJob = false
        videoGenerationTask = nil
    }

    func cancelVideoGeneration() {
        videoGenerationState.reset()
        videoGenerationTask?.cancel()
        videoGenerationTask = nil
        isGeneratingVideo = false
        isRecoveringVideoJob = false
        videoProgress = nil
    }

    func selectInputImage(from url: URL) {
        addInputImages(from: [url])
    }

    func addInputImages(from urls: [URL]) {
        guard let ownerID, !urls.isEmpty else { return }
        do {
            try imageReferenceUseCase.validateCapacity(
                currentCount: inputImages.count,
                requestedCount: urls.count
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        inputImagesTask?.cancel()
        let token = imageReferenceState.begin(
            ownerUserID: ownerID,
            accountSessionID: sessionID,
            source: .localFiles
        )
        let useCase = imageReferenceUseCase
        let current = inputImages
        errorMessage = nil
        isLoadingInputImages = true
        inputImagesTask = Task { [weak self] in
            let result: Result<[ImageGenerationInputImage], Error>
            do {
                result = .success(try await useCase.importLocalFiles(urls, current: current))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled else { return }
            self?.finishImageReferenceImport(result, token: token)
        }
    }

    func addGeneratedImagesAsReferences(_ assets: [GeneratedMediaAsset]) {
        guard let ownerID, !assets.isEmpty,
              assets.allSatisfy({ asset in history.contains { $0.images.contains(asset) } }) else { return }
        do {
            try imageReferenceUseCase.validateCapacity(
                currentCount: inputImages.count,
                requestedCount: assets.count
            )
        } catch {
            errorMessage = error.localizedDescription
            return
        }
        inputImagesTask?.cancel()
        let token = imageReferenceState.begin(
            ownerUserID: ownerID,
            accountSessionID: sessionID,
            source: .generatedHistory
        )
        let useCase = imageReferenceUseCase
        let current = inputImages
        let authorizedHistory = history
        errorMessage = nil
        isLoadingInputImages = true
        inputImagesTask = Task { [weak self] in
            let result: Result<[ImageGenerationInputImage], Error>
            do {
                result = .success(try await useCase.importGeneratedAssets(
                    assets,
                    authorizedBy: authorizedHistory,
                    current: current
                ))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled else { return }
            self?.finishImageReferenceImport(result, token: token)
        }
    }

    private func finishImageReferenceImport(
        _ result: Result<[ImageGenerationInputImage], Error>,
        token: MediaStudioImageReferenceToken
    ) {
        guard imageReferenceState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ) else {
            if case let .success(images) = result {
                for image in images where !inputImages.contains(image) {
                    inputArtifacts.removeIfManaged(image.fileURL)
                }
            }
            return
        }
        switch result {
        case let .success(images): inputImages = images
        case let .failure(error): errorMessage = error.localizedDescription
        }
        guard imageReferenceState.finish(token) else { return }
        isLoadingInputImages = false
        inputImagesTask = nil
    }

    func removeInputImage() {
        guard !isGenerating else { return }
        imageReferenceState.reset()
        inputImagesTask?.cancel()
        inputImagesTask = nil
        isLoadingInputImages = false
        let fileURLs = inputImages.compactMap(\.fileURL)
        inputImages = []
        for fileURL in fileURLs { inputArtifacts.removeIfManaged(fileURL) }
    }

    func removeInputImage(at index: Int) {
        guard inputImages.indices.contains(index), !isGenerating else { return }
        if isLoadingInputImages {
            imageReferenceState.reset()
            inputImagesTask?.cancel()
            inputImagesTask = nil
            isLoadingInputImages = false
        }
        let removed = inputImages.remove(at: index)
        inputArtifacts.removeIfManaged(removed.fileURL)
    }

    func selectVideoInputImage(from url: URL) {
        let artifacts = inputArtifacts
        loadVideoInputImage {
            try await artifacts.importImage(from: url)
        }
    }

    func useGeneratedImageForVideo(_ asset: GeneratedMediaAsset) {
        // A dismissed picker from a previous account must not import that account's media.
        guard ownerID != nil,
              history.contains(where: { $0.images.contains(asset) }),
              let url = asset.url,
              url.isFileURL else { return }
        section = .video
        let artifacts = inputArtifacts
        loadVideoInputImage {
            try await artifacts.importImage(
                from: url,
                name: "generated-\(asset.id).\(url.pathExtension)",
                mimeType: asset.mimeType
            )
        }
    }

    private func loadVideoInputImage(_ load: @escaping @Sendable () async throws -> ImageGenerationInputImage) {
        videoInputTask?.cancel()
        videoInputSelectionID = UUID()
        let selection = videoInputSelectionID
        errorMessage = nil
        isLoadingVideoInputImage = true
        let session = sessionID
        videoInputTask = Task { [weak self] in
            let result: Result<ImageGenerationInputImage, Error>
            do {
                result = .success(try await load())
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.sessionID == session,
                  self.videoInputSelectionID == selection else {
                if case let .success(image) = result {
                    self?.inputArtifacts.removeIfManaged(image.fileURL)
                }
                return
            }
            switch result {
            case let .success(image):
                let previousURL = self.videoInputImage?.fileURL
                self.videoInputImage = image
                self.inputArtifacts.removeIfManaged(previousURL)
            case let .failure(error): self.errorMessage = error.localizedDescription
            }
            self.isLoadingVideoInputImage = false
            self.videoInputTask = nil
        }
    }

    func removeVideoInputImage() {
        videoInputSelectionID = UUID()
        videoInputTask?.cancel()
        videoInputTask = nil
        isLoadingVideoInputImage = false
        let fileURL = videoInputImage?.fileURL
        videoInputImage = nil
        inputArtifacts.removeIfManaged(fileURL)
    }

    func selectVideoReferenceAudio(from url: URL) {
        videoAudioTask?.cancel()
        videoAudioSelectionID = UUID()
        let selection = videoAudioSelectionID
        let session = sessionID
        errorMessage = nil
        isLoadingVideoReferenceAudio = true
        let artifacts = inputArtifacts
        videoAudioTask = Task { [weak self] in
            let result: Result<VideoGenerationInputAudio, Error>
            do {
                result = .success(try await artifacts.importAudio(from: url))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.sessionID == session,
                  self.videoAudioSelectionID == selection else {
                if case let .success(audio) = result {
                    self?.inputArtifacts.removeIfManaged(audio.fileURL)
                }
                return
            }
            switch result {
            case let .success(audio):
                let previousURL = self.videoReferenceAudio?.fileURL
                self.videoReferenceAudio = audio
                self.inputArtifacts.removeIfManaged(previousURL)
            case let .failure(error): self.errorMessage = error.localizedDescription
            }
            self.isLoadingVideoReferenceAudio = false
            self.videoAudioTask = nil
        }
    }

    func removeVideoReferenceAudio() {
        videoAudioSelectionID = UUID()
        videoAudioTask?.cancel()
        videoAudioTask = nil
        isLoadingVideoReferenceAudio = false
        let fileURL = videoReferenceAudio?.fileURL
        videoReferenceAudio = nil
        inputArtifacts.removeIfManaged(fileURL)
    }

    func reportInputImageError(_ error: Error) {
        errorMessage = error.localizedDescription
    }

    func resetForSignedOut() {
        if stories.owner != nil {
            stories.reset()
        }
        sessionID = UUID()
        imageGenerationState.reset()
        videoGenerationState.reset()
        ownerID = nil
        imageGenerationTask?.cancel()
        imageGenerationTask = nil
        imageReferenceState.reset()
        inputImagesTask?.cancel()
        inputImagesTask = nil
        isLoadingInputImages = false
        historyLoadTask?.cancel()
        historyLoadTask = nil
        historyLoadState.reset()
        isLoadingHistory = false
        isLoadingMoreHistory = false
        hasMoreHistory = false
        historyCursor = nil
        unreadableHistoryCount = 0
        historyErrorMessage = nil
        modelCatalogState.reset()
        modelsLoadTask?.cancel()
        modelsLoadTask = nil
        videoGenerationTask?.cancel()
        videoGenerationTask = nil
        hasLoaded = false
        hasLoadedHistory = false
        hasAttemptedVideoRecovery = false
        prompt = ""
        selectedModelID = nil
        inputImages = []
        videoPrompt = ""
        selectedVideoModelID = nil
        removeVideoInputImage()
        removeVideoReferenceAudio()
        models = []
        videoModels = []
        history = []
        videoHistory = []
        isLoadingModels = false
        isGenerating = false
        isGeneratingVideo = false
        isRecoveringVideoJob = false
        videoProgress = nil
        errorMessage = nil
        inputArtifacts.removeAll()
    }

    private func loadModels() {
        guard let ownerID else { return }
        let token = modelCatalogState.begin(
            ownerUserID: ownerID,
            accountSessionID: sessionID
        )
        modelsLoadTask?.cancel()
        let useCase = modelCatalogUseCase
        isLoadingModels = true
        errorMessage = nil
        modelsLoadTask = Task { [weak self] in
            let result: Result<MediaStudioModelCatalog, Error>
            do {
                result = .success(try await useCase.load())
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled else { return }
            self?.finishModelCatalogLoad(result, token: token)
        }
    }

    private func finishModelCatalogLoad(
        _ result: Result<MediaStudioModelCatalog, Error>,
        token: MediaStudioModelCatalogToken
    ) {
        guard modelCatalogState.accepts(
            token,
            ownerUserID: ownerID,
            accountSessionID: sessionID
        ) else { return }
        switch result {
        case let .success(catalog):
            models = catalog.imageModels
            videoModels = catalog.videoModels
            if !models.contains(where: { $0.id == selectedModelID }) {
                selectedModelID = models.first?.id
            }
            if !videoModels.contains(where: { $0.id == selectedVideoModelID }) {
                selectedVideoModelID = videoModels.first?.id
            }
            normalizeVideoOptions()
        case let .failure(error):
            errorMessage = error.localizedDescription
        }
        guard modelCatalogState.finish(token) else { return }
        isLoadingModels = false
        modelsLoadTask = nil
    }

}
