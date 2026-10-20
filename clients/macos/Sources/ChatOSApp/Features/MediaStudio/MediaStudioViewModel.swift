import ChatOSAPI
import ChatOSCore
import Combine
import Foundation
import UniformTypeIdentifiers

@MainActor
final class MediaStudioViewModel: ObservableObject {
    private enum VideoGenerationCompletion {
        case success(VideoHistoryItem)
        case failure(String)
    }

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
        if videoProgress?.status == "failed" { return .failed }
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
    private let modelCatalogUseCase: MediaStudioModelCatalogUseCase
    private let historyUseCase: MediaStudioHistoryUseCase
    private let imageReferenceUseCase: MediaStudioImageReferenceUseCase
    private let imageReferenceImporter: any MediaStudioImageReferenceImporting
    let stories: StoryStudioViewModel
    private var hasLoaded = false
    private var videoGenerationTask: Task<Void, Never>?
    private var imageGenerationTask: Task<Void, Never>?
    private var historyLoadTask: Task<Void, Never>?
    private var historyCursor: MediaStudioHistoryCursor?
    private var unreadableHistoryCount = 0
    private var historyLoadState = MediaStudioHistoryLoadState()
    private var modelsLoadTask: Task<Void, Never>?
    private var modelCatalogState = MediaStudioModelCatalogState()
    private let historyStore: MediaStudioHistoryStore
    private var ownerID: String?
    private var sessionID = UUID()
    private var imageGenerationState = MediaStudioImageGenerationState()
    private var videoOperationID = UUID()
    private var videoInputSelectionID = UUID()
    private var videoInputTask: Task<Void, Never>?
    private var videoAudioSelectionID = UUID()
    private var videoAudioTask: Task<Void, Never>?
    private var inputImagesTask: Task<Void, Never>?
    private var imageReferenceState = MediaStudioImageReferenceState()

    init(service: any MediaGenerationServicing, historyStore: MediaStudioHistoryStore = MediaStudioHistoryStore(), imageTransport: any HTTPTransport = URLSessionHTTPTransport(), storyPlanner: (any StoryPlanningServicing)? = nil) {
        self.service = service
        self.imageGenerationUseCase = MediaStudioImageGenerationUseCase(
            generator: service,
            history: historyStore
        )
        self.modelCatalogUseCase = MediaStudioModelCatalogUseCase(provider: service)
        self.historyUseCase = MediaStudioHistoryUseCase(history: historyStore)
        let imageReferenceImporter = MediaStudioImageReferenceImporter(transport: imageTransport)
        self.imageReferenceImporter = imageReferenceImporter
        self.imageReferenceUseCase = MediaStudioImageReferenceUseCase(
            importer: imageReferenceImporter
        )
        self.stories = StoryStudioViewModel(media: service, planner: storyPlanner)
        self.historyStore = historyStore
    }

    deinit {
        modelsLoadTask?.cancel()
        historyLoadTask?.cancel()
        imageGenerationTask?.cancel()
        videoGenerationTask?.cancel()
        inputImagesTask?.cancel()
        videoInputTask?.cancel()
        videoAudioTask?.cancel()
    }

    func activate(userID: String) {
        guard ownerID != userID else { return }
        resetForSignedOut()
        ownerID = userID
        stories.activate(userID: userID)
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
        case .initial: isLoadingHistory = false
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
        guard !hasLoaded else { return }
        hasLoaded = true
        loadModels()
    }

    func reloadModels() {
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
        let session = sessionID
        videoOperationID = UUID()
        let operation = videoOperationID
        let submittedPrompt = videoPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let request = VideoGenerationRequest(
            modelConfigID: selectedVideoModelID, prompt: submittedPrompt,
            size: videoSize, seconds: videoSeconds, inputImage: videoInputImage,
            referenceAudio: videoReferenceAudio, ratio: videoRatio
        )
        isGeneratingVideo = true
        videoProgress = .init(status: "submitting")
        errorMessage = nil
        let service = service
        let historyStore = historyStore
        videoGenerationTask?.cancel()
        videoGenerationTask = Task { [weak self] in
            var isSaving = false
            do {
                let result = try await service.generateVideo(request) { [weak self] progress in
                    await MainActor.run {
                        guard self?.sessionID == session, self?.videoOperationID == operation else { return }
                        self?.videoProgress = progress
                    }
                }
                guard !Task.isCancelled,
                      self?.beginSavingVideo(session: session, operation: operation) == true else {
                    return
                }
                isSaving = true
                let item = try await historyStore.saveVideo(result, prompt: submittedPrompt, owner: ownerID)
                guard !Task.isCancelled else { return }
                self?.finishVideoGeneration(
                    .success(item),
                    session: session,
                    operation: operation,
                    failedWhileSaving: false
                )
            } catch is CancellationError {
                guard !Task.isCancelled else { return }
                self?.finishCancelledVideoGeneration(session: session, operation: operation)
            } catch {
                guard !Task.isCancelled else { return }
                self?.finishVideoGeneration(
                    .failure(error.localizedDescription),
                    session: session,
                    operation: operation,
                    failedWhileSaving: isSaving
                )
            }
        }
    }

    private func beginSavingVideo(session: UUID, operation: UUID) -> Bool {
        guard sessionID == session, videoOperationID == operation else { return false }
        videoProgress = .init(status: "saving")
        return true
    }

    private func finishVideoGeneration(
        _ result: VideoGenerationCompletion,
        session: UUID,
        operation: UUID,
        failedWhileSaving: Bool
    ) {
        guard sessionID == session, videoOperationID == operation else { return }
        switch result {
        case let .success(item):
            videoHistory.insert(item, at: 0)
            videoProgress = .init(status: "completed", percent: 100)
        case let .failure(message):
            if failedWhileSaving {
                historyErrorMessage = "视频已生成，但未能保存到本机：\(message)"
                errorMessage = historyErrorMessage
            } else {
                errorMessage = message
            }
            videoProgress = .init(status: "failed")
        }
        isGeneratingVideo = false
        videoGenerationTask = nil
    }

    private func finishCancelledVideoGeneration(session: UUID, operation: UUID) {
        guard sessionID == session, videoOperationID == operation else { return }
        videoProgress = nil
        isGeneratingVideo = false
        videoGenerationTask = nil
    }

    func cancelVideoGeneration() {
        videoOperationID = UUID()
        videoGenerationTask?.cancel()
        videoGenerationTask = nil
        isGeneratingVideo = false
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
        ) else { return }
        switch result {
        case let .success(images): inputImages = images
        case let .failure(error): errorMessage = error.localizedDescription
        }
        guard imageReferenceState.finish(token) else { return }
        isLoadingInputImages = false
        inputImagesTask = nil
    }

    func removeInputImage() {
        imageReferenceState.reset()
        inputImagesTask?.cancel()
        inputImagesTask = nil
        isLoadingInputImages = false
        inputImages = []
    }

    func removeInputImage(at index: Int) {
        guard inputImages.indices.contains(index), !isGenerating else { return }
        if isLoadingInputImages {
            imageReferenceState.reset()
            inputImagesTask?.cancel()
            inputImagesTask = nil
            isLoadingInputImages = false
        }
        inputImages.remove(at: index)
    }

    func selectVideoInputImage(from url: URL) {
        let importer = imageReferenceImporter
        loadVideoInputImage {
            guard let image = try await importer.importLocalFiles([url]).first else {
                throw MediaStudioInputImageError.cannotDecode
            }
            return image
        }
    }

    func useGeneratedImageForVideo(_ asset: GeneratedMediaAsset) {
        // A dismissed picker from a previous account must not import that account's media.
        guard ownerID != nil, history.contains(where: { $0.images.contains(asset) }) else { return }
        section = .video
        let importer = imageReferenceImporter
        loadVideoInputImage {
            guard let image = try await importer.importGeneratedAssets([asset]).first else {
                throw MediaStudioInputImageError.cannotDecode
            }
            return image
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
                  self.videoInputSelectionID == selection else { return }
            switch result {
            case let .success(image): self.videoInputImage = image
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
        videoInputImage = nil
    }

    func selectVideoReferenceAudio(from url: URL) {
        videoAudioTask?.cancel()
        videoAudioSelectionID = UUID()
        let selection = videoAudioSelectionID
        let session = sessionID
        errorMessage = nil
        isLoadingVideoReferenceAudio = true
        videoAudioTask = Task { [weak self] in
            let result: Result<VideoGenerationInputAudio, Error>
            do {
                result = .success(try await AppCancellableDetachedWork.run {
                    try Self.loadReferenceAudio(from: url)
                })
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.sessionID == session,
                  self.videoAudioSelectionID == selection else { return }
            switch result {
            case let .success(audio): self.videoReferenceAudio = audio
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
        videoReferenceAudio = nil
    }

    func reportInputImageError(_ error: Error) {
        errorMessage = error.localizedDescription
    }

    func resetForSignedOut() {
        stories.reset()
        sessionID = UUID()
        imageGenerationState.reset()
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
        videoProgress = nil
        errorMessage = nil
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

    nonisolated private static func loadReferenceAudio(from url: URL) throws -> VideoGenerationInputAudio {
        try Task.checkCancellation()
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }

        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey, .contentTypeKey])
        let supportedMIMETypes = [
            "audio/mpeg", "audio/mp3", "audio/wav", "audio/x-wav", "audio/vnd.wave",
            "audio/mp4", "audio/x-m4a", "audio/aac",
        ]
        guard values.isRegularFile == true,
              let mimeType = values.contentType?.preferredMIMEType,
              supportedMIMETypes.contains(mimeType.lowercased()) else {
            throw MediaStudioReferenceAudioError.unsupported
        }
        if let fileSize = values.fileSize, fileSize > 20 * 1024 * 1024 {
            throw MediaStudioReferenceAudioError.tooLarge
        }
        let data = try AppBoundedFileReader.read(
            url,
            maximumBytes: 20 * 1_024 * 1_024
        )
        guard !data.isEmpty, data.count <= 20 * 1024 * 1024 else {
            throw MediaStudioReferenceAudioError.tooLarge
        }
        try Task.checkCancellation()
        let base64Data = data.base64EncodedString()
        try Task.checkCancellation()
        return .init(
            name: url.lastPathComponent, mimeType: mimeType,
            base64Data: base64Data
        )
    }

}

private enum MediaStudioReferenceAudioError: LocalizedError {
    case unsupported
    case tooLarge

    var errorDescription: String? {
        switch self {
        case .unsupported:
            "请选择 MP3、WAV、M4A 或 AAC 音频文件。"
        case .tooLarge:
            "参考音频不能超过 20 MB。"
        }
    }
}
