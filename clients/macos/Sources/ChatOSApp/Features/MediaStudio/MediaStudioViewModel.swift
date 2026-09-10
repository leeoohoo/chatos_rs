import AppKit
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

    struct HistoryItem: Identifiable, Equatable, Sendable {
        var id: String
        var prompt: String
        var modelName: String
        var createdAt: Date
        var images: [GeneratedMediaAsset]
    }

    struct VideoHistoryItem: Identifiable, Equatable, Sendable {
        var id: String
        var prompt: String
        var modelName: String
        var createdAt: Date
        var fileURL: URL
    }

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
    @Published private(set) var inputImage: ImageGenerationInputImage?
    @Published private(set) var videoInputImage: ImageGenerationInputImage?
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
    @Published private(set) var isLoadingVideoInputImage = false

    private let service: any MediaGenerationServicing
    let stories: StoryStudioViewModel
    private var hasLoaded = false
    private var videoGenerationTask: Task<Void, Never>?
    private var imageGenerationTask: Task<Void, Never>?
    private var historyLoadTask: Task<Void, Never>?
    private let historyStore: MediaStudioHistoryStore
    private var ownerID: String?
    private var sessionID = UUID()
    private var videoOperationID = UUID()
    private var videoInputSelectionID = UUID()
    private var videoInputTask: Task<Void, Never>?
    private let imageTransport: any HTTPTransport

    init(service: any MediaGenerationServicing, historyStore: MediaStudioHistoryStore = MediaStudioHistoryStore(), imageTransport: any HTTPTransport = URLSessionHTTPTransport(), storyPlanner: (any StoryPlanningServicing)? = nil) {
        self.service = service
        self.stories = StoryStudioViewModel(media: service, planner: storyPlanner)
        self.historyStore = historyStore
        self.imageTransport = imageTransport
    }

    func activate(userID: String) {
        guard ownerID != userID else { return }
        resetForSignedOut()
        ownerID = userID
        stories.activate(userID: userID)
        isLoadingHistory = true
        let session = sessionID
        historyLoadTask = Task {
            do {
                let snapshot = try await historyStore.load(owner: userID)
                guard sessionID == session else { return }
                history = snapshot.images
                videoHistory = snapshot.videos
                if snapshot.unreadableCount > 0 {
                    historyErrorMessage = "有 \(snapshot.unreadableCount) 条记录无法读取，原文件已保留。"
                }
            } catch {
                guard sessionID == session else { return }
                historyErrorMessage = "读取创作记录失败：\(error.localizedDescription)"
            }
            isLoadingHistory = false
        }
    }

    var selectedModel: MediaGenerationModel? {
        guard let selectedModelID else { return nil }
        return models.first(where: { $0.id == selectedModelID })
    }

    var canGenerate: Bool {
        !isGenerating
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
        let submittedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let request = ImageGenerationRequest(
            modelConfigID: selectedModelID, prompt: submittedPrompt,
            size: size == "auto" ? nil : size, count: count, inputImage: inputImage
        )
        isGenerating = true
        errorMessage = nil
        imageGenerationTask = Task {
            do {
                let result = try await service.generateImage(request)
                guard sessionID == session else { return }
                do {
                    let item = try await historyStore.saveImage(result, prompt: submittedPrompt, owner: ownerID)
                    guard sessionID == session else { return }
                    history.insert(item, at: 0)
                } catch {
                    guard sessionID == session else { return }
                    // Keep the generated result visible even when the disk/download fails.
                    history.insert(
                        HistoryItem(
                            id: result.id,
                            prompt: submittedPrompt,
                            modelName: result.modelName,
                            createdAt: ISO8601DateFormatter().date(from: result.createdAt) ?? Date(),
                            images: result.images
                        ),
                        at: 0
                    )
                    historyErrorMessage = "图片已生成，但未能保存到本机：\(error.localizedDescription)"
                    errorMessage = historyErrorMessage
                }
            } catch {
                guard sessionID == session else { return }
                errorMessage = error.localizedDescription
            }
            isGenerating = false
            imageGenerationTask = nil
        }
    }

    func generateVideo() {
        guard canGenerateVideo, let selectedVideoModelID, let ownerID else { return }
        let session = sessionID
        videoOperationID = UUID()
        let operation = videoOperationID
        let submittedPrompt = videoPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let request = VideoGenerationRequest(
            modelConfigID: selectedVideoModelID, prompt: submittedPrompt,
            size: videoSize, seconds: videoSeconds, inputImage: videoInputImage, ratio: videoRatio
        )
        isGeneratingVideo = true
        videoProgress = .init(status: "submitting")
        errorMessage = nil
        videoGenerationTask?.cancel()
        videoGenerationTask = Task { [weak self] in
            guard let self else { return }
            do {
                let result = try await service.generateVideo(request) { [weak self] progress in
                    await MainActor.run {
                        guard self?.sessionID == session, self?.videoOperationID == operation else { return }
                        self?.videoProgress = progress
                    }
                }
                guard sessionID == session, videoOperationID == operation else { return }
                videoProgress = .init(status: "saving")
                let item = try await historyStore.saveVideo(result, prompt: submittedPrompt, owner: ownerID)
                guard sessionID == session, videoOperationID == operation else { return }
                videoHistory.insert(item, at: 0)
                videoProgress = .init(status: "completed", percent: 100)
            } catch is CancellationError {
                guard sessionID == session, videoOperationID == operation else { return }
                videoProgress = nil
            } catch {
                guard sessionID == session, videoOperationID == operation else { return }
                if videoProgress?.status == "saving" {
                    historyErrorMessage = "视频已生成，但未能保存到本机：\(error.localizedDescription)"
                    errorMessage = historyErrorMessage
                } else {
                    errorMessage = error.localizedDescription
                }
                videoProgress = .init(status: "failed")
            }
            isGeneratingVideo = false
            videoGenerationTask = nil
        }
    }

    func cancelVideoGeneration() {
        videoOperationID = UUID()
        videoGenerationTask?.cancel()
        videoGenerationTask = nil
        isGeneratingVideo = false
        videoProgress = nil
    }

    func selectInputImage(from url: URL) {
        errorMessage = nil
        let session = sessionID
        Task {
            do {
                let image = try await Task.detached(priority: .userInitiated) {
                    try Self.loadInputImage(from: url)
                }.value
                guard sessionID == session else { return }
                inputImage = image
            } catch {
                guard sessionID == session else { return }
                errorMessage = error.localizedDescription
            }
        }
    }

    func removeInputImage() {
        inputImage = nil
    }

    func selectVideoInputImage(from url: URL) {
        loadVideoInputImage {
            try await Task.detached(priority: .userInitiated) {
                try Self.loadInputImage(from: url)
            }.value
        }
    }

    func useGeneratedImageForVideo(_ asset: GeneratedMediaAsset) {
        // A dismissed picker from a previous account must not import that account's media.
        guard ownerID != nil, history.contains(where: { $0.images.contains(asset) }) else { return }
        section = .video
        let transport = imageTransport
        loadVideoInputImage {
            let data = try await MediaStudioImageLoader.data(for: asset, transport: transport)
            return try await Task.detached(priority: .userInitiated) {
                try Self.makeInputImage(data: data, name: "generated-\(asset.id).png")
            }.value
        }
    }

    private func loadVideoInputImage(_ load: @escaping @Sendable () async throws -> ImageGenerationInputImage) {
        videoInputTask?.cancel()
        videoInputSelectionID = UUID()
        let selection = videoInputSelectionID
        errorMessage = nil
        isLoadingVideoInputImage = true
        let session = sessionID
        videoInputTask = Task {
            do {
                let image = try await load()
                guard sessionID == session, videoInputSelectionID == selection else { return }
                videoInputImage = image
            } catch {
                guard sessionID == session, videoInputSelectionID == selection else { return }
                errorMessage = error.localizedDescription
            }
            isLoadingVideoInputImage = false
            videoInputTask = nil
        }
    }

    func removeVideoInputImage() {
        videoInputSelectionID = UUID()
        videoInputTask?.cancel()
        videoInputTask = nil
        isLoadingVideoInputImage = false
        videoInputImage = nil
    }

    func reportInputImageError(_ error: Error) {
        errorMessage = error.localizedDescription
    }

    func resetForSignedOut() {
        stories.reset()
        sessionID = UUID()
        ownerID = nil
        imageGenerationTask?.cancel()
        imageGenerationTask = nil
        historyLoadTask?.cancel()
        historyLoadTask = nil
        isLoadingHistory = false
        historyErrorMessage = nil
        videoGenerationTask?.cancel()
        videoGenerationTask = nil
        hasLoaded = false
        prompt = ""
        selectedModelID = nil
        inputImage = nil
        videoPrompt = ""
        selectedVideoModelID = nil
        removeVideoInputImage()
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
        let session = sessionID
        isLoadingModels = true
        errorMessage = nil
        Task {
            do {
                let next = try await service.fetchModels()
                guard sessionID == session else { return }
                models = next
                videoModels = next.filter(\.isLikelyVideoModel)
                if !next.contains(where: { $0.id == selectedModelID }) {
                    selectedModelID = next.first?.id
                }
                if !videoModels.contains(where: { $0.id == selectedVideoModelID }) {
                    selectedVideoModelID = videoModels.first?.id
                }
                normalizeVideoOptions()
            } catch {
                guard sessionID == session else { return }
                errorMessage = error.localizedDescription
            }
            isLoadingModels = false
        }
    }

    nonisolated private static func loadInputImage(from url: URL) throws -> ImageGenerationInputImage {
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }

        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey, .contentTypeKey])
        guard values.isRegularFile == true,
              values.contentType?.conforms(to: .image) == true else {
            throw MediaStudioInputImageError.notAnImage
        }
        if let fileSize = values.fileSize, fileSize > 40 * 1024 * 1024 {
            throw MediaStudioInputImageError.tooLarge
        }

        let sourceData = try Data(contentsOf: url, options: [.mappedIfSafe])
        return try makeInputImage(data: sourceData, name: url.deletingPathExtension().lastPathComponent + ".png")
    }

    nonisolated private static func makeInputImage(data: Data, name: String) throws -> ImageGenerationInputImage {
        guard let image = NSImage(data: data),
              let tiff = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff),
              let pngData = bitmap.representation(using: .png, properties: [:]) else {
            throw MediaStudioInputImageError.cannotDecode
        }
        guard pngData.count <= 20 * 1024 * 1024 else {
            throw MediaStudioInputImageError.tooLarge
        }

        return ImageGenerationInputImage(
            name: name,
            mimeType: "image/png",
            base64Data: pngData.base64EncodedString()
        )
    }

}

private enum MediaStudioInputImageError: LocalizedError {
    case notAnImage
    case cannotDecode
    case tooLarge

    var errorDescription: String? {
        switch self {
        case .notAnImage:
            "请选择有效的图片文件。"
        case .cannotDecode:
            "无法读取这张图片，请换一张后重试。"
        case .tooLarge:
            "参考图不能超过 20 MB。"
        }
    }
}
