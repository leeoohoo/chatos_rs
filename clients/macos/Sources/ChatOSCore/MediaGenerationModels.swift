import Foundation

public struct MediaGenerationModel: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var name: String
    public var provider: String
    public var modelName: String
    public var enabled: Bool
    public var taskEnabled: Bool
    public var hasAPIKey: Bool

    public init(
        id: String,
        name: String,
        provider: String,
        modelName: String,
        enabled: Bool,
        taskEnabled: Bool,
        hasAPIKey: Bool
    ) {
        self.id = id
        self.name = name
        self.provider = provider
        self.modelName = modelName
        self.enabled = enabled
        self.taskEnabled = taskEnabled
        self.hasAPIKey = hasAPIKey
    }

    enum CodingKeys: String, CodingKey {
        case id, name, provider, enabled
        case modelName = "model"
        case taskEnabled = "task_enabled"
        case hasAPIKey = "has_api_key"
    }

    public var isLikelyVideoModel: Bool {
        let value = "\(name) \(modelName)".lowercased()
        return [
            "video", "sora", "veo", "kling", "seedance", "hailuo",
            "minimax", "runway", "luma", "pixverse", "vidu", "hunyuan",
            "cogvideo", "wan2", "wan-", "wan_",
        ].contains(where: value.contains)
    }

    /// NewAPI's unified `/v1/videos` contract exposes model capabilities by profile.
    public var supportsVideoLastFrame: Bool {
        VideoGenerationProfile(modelName: modelName).supportsLastFrame
    }

    /// MiniMax H3 reference mode can use a completed video as motion/style context.
    /// Frame inputs and reference-video inputs are mutually exclusive upstream.
    public var supportsVideoReference: Bool {
        VideoGenerationProfile(modelName: modelName).supportsReferenceVideo
    }

    /// True when the provider exposes a source-preserving video edit operation.
    public var supportsVideoEditing: Bool {
        VideoGenerationProfile(modelName: modelName).supportsVideoEditing
    }

    /// True when the provider can continue forward from a completed source video.
    public var supportsVideoExtension: Bool {
        VideoGenerationProfile(modelName: modelName).supportsVideoExtension
    }
}

public struct ImageGenerationRequest: Codable, Sendable, Equatable {
    public var modelConfigID: String
    public var prompt: String
    public var size: String?
    public var count: Int
    public var inputImage: ImageGenerationInputImage?
    public var referenceImages: [ImageGenerationInputImage]
    /// Stable caller identities. These travel with the request/result so concurrent
    /// generations never have to infer their target from array position.
    public var clientRequestID: String?
    public var projectID: String?
    public var resourceID: String?

    public init(
        modelConfigID: String,
        prompt: String,
        size: String?,
        count: Int,
        inputImage: ImageGenerationInputImage? = nil,
        referenceImages: [ImageGenerationInputImage] = [],
        clientRequestID: String? = nil,
        projectID: String? = nil,
        resourceID: String? = nil
    ) {
        self.modelConfigID = modelConfigID
        self.prompt = prompt
        self.size = size
        self.count = count
        self.inputImage = inputImage
        self.referenceImages = referenceImages
        self.clientRequestID = clientRequestID
        self.projectID = projectID
        self.resourceID = resourceID
    }

    enum CodingKeys: String, CodingKey {
        case modelConfigID = "model_config_id"
        case prompt, size, count
        case inputImage = "input_image"
        case referenceImages = "reference_images"
        case clientRequestID = "client_request_id"
        case projectID = "project_id"
        case resourceID = "resource_id"
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        modelConfigID = try values.decode(String.self, forKey: .modelConfigID)
        prompt = try values.decode(String.self, forKey: .prompt)
        size = try values.decodeIfPresent(String.self, forKey: .size)
        count = try values.decode(Int.self, forKey: .count)
        inputImage = try values.decodeIfPresent(ImageGenerationInputImage.self, forKey: .inputImage)
        referenceImages = try values.decodeIfPresent([ImageGenerationInputImage].self, forKey: .referenceImages) ?? []
        clientRequestID = try values.decodeIfPresent(String.self, forKey: .clientRequestID)
        projectID = try values.decodeIfPresent(String.self, forKey: .projectID)
        resourceID = try values.decodeIfPresent(String.self, forKey: .resourceID)
    }
}

public struct ImageGenerationInputImage: Codable, Sendable, Equatable {
    public var name: String
    public var mimeType: String
    public var base64Data: String

    public init(name: String, mimeType: String, base64Data: String) {
        self.name = name
        self.mimeType = mimeType
        self.base64Data = base64Data
    }

    enum CodingKeys: String, CodingKey {
        case name
        case mimeType = "mime_type"
        case base64Data = "base64_data"
    }
}

public struct GeneratedMediaAsset: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var mimeType: String
    public var base64Data: String?
    public var url: URL?
    public var revisedPrompt: String?

    public init(
        id: String,
        mimeType: String,
        base64Data: String? = nil,
        url: URL? = nil,
        revisedPrompt: String? = nil
    ) {
        self.id = id
        self.mimeType = mimeType
        self.base64Data = base64Data
        self.url = url
        self.revisedPrompt = revisedPrompt
    }

    enum CodingKeys: String, CodingKey {
        case id
        case mimeType = "mime_type"
        case base64Data = "base64_data"
        case url
        case revisedPrompt = "revised_prompt"
    }
}

public struct ImageGenerationResult: Codable, Sendable, Equatable {
    public var id: String
    public var modelConfigID: String
    public var modelName: String
    public var createdAt: String
    public var images: [GeneratedMediaAsset]
    public var clientRequestID: String?
    public var projectID: String?
    public var resourceID: String?

    public init(
        id: String,
        modelConfigID: String,
        modelName: String,
        createdAt: String,
        images: [GeneratedMediaAsset],
        clientRequestID: String? = nil,
        projectID: String? = nil,
        resourceID: String? = nil
    ) {
        self.id = id
        self.modelConfigID = modelConfigID
        self.modelName = modelName
        self.createdAt = createdAt
        self.images = images
        self.clientRequestID = clientRequestID
        self.projectID = projectID
        self.resourceID = resourceID
    }

    enum CodingKeys: String, CodingKey {
        case id
        case modelConfigID = "model_config_id"
        case modelName = "model"
        case createdAt = "created_at"
        case images
        case clientRequestID = "client_request_id"
        case projectID = "project_id"
        case resourceID = "resource_id"
    }
}

public struct VideoGenerationRequest: Sendable, Equatable {
    public var modelConfigID: String
    public var prompt: String
    public var size: String
    public var seconds: Int
    public var inputImage: ImageGenerationInputImage?
    public var lastFrameImage: ImageGenerationInputImage?
    public var referenceVideo: VideoGenerationInputVideo?
    public var referenceAudio: VideoGenerationInputAudio?
    public var referencePurpose: VideoGenerationReferencePurpose
    public var ratio: String

    public init(
        modelConfigID: String,
        prompt: String,
        size: String,
        seconds: Int,
        inputImage: ImageGenerationInputImage? = nil,
        lastFrameImage: ImageGenerationInputImage? = nil,
        referenceVideo: VideoGenerationInputVideo? = nil,
        referenceAudio: VideoGenerationInputAudio? = nil,
        referencePurpose: VideoGenerationReferencePurpose = .reference,
        ratio: String = "16:9"
    ) {
        self.modelConfigID = modelConfigID
        self.prompt = prompt
        self.size = size
        self.seconds = seconds
        self.inputImage = inputImage
        self.lastFrameImage = lastFrameImage
        self.referenceVideo = referenceVideo
        self.referenceAudio = referenceAudio
        self.referencePurpose = referencePurpose
        self.ratio = ratio
    }
}

public enum VideoGenerationReferencePurpose: Sendable, Equatable {
    /// Use the source as visual/motion context for a new generation.
    case reference
    /// Modify the source while retaining its unaffected content where possible.
    case edit
    /// Continue forward from the end of the source video.
    case extend
}

public struct VideoGenerationInputVideo: Sendable, Equatable {
    public var name: String
    public var mimeType: String
    public var base64Data: String

    public init(name: String, mimeType: String, base64Data: String) {
        self.name = name
        self.mimeType = mimeType
        self.base64Data = base64Data
    }
}

public struct VideoGenerationInputAudio: Sendable, Equatable {
    public var name: String
    public var mimeType: String
    public var base64Data: String

    public init(name: String, mimeType: String, base64Data: String) {
        self.name = name
        self.mimeType = mimeType
        self.base64Data = base64Data
    }
}

/// Shared by request validation and the creation form. Capabilities remain model-specific,
/// while transport uses NewAPI's single `/v1/videos` contract for every video model.
public enum VideoGenerationProfile: Sendable {
    case openAI, miniMaxH3, miniMaxH3Max
    case seedance25, seedance20, seedance20Fast, seedance20Mini

    public init(modelName: String) {
        let value = modelName.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        switch value {
        case "minimax-h3": self = .miniMaxH3
        case "minimax-h3-max": self = .miniMaxH3Max
        default:
            if value.contains("seedance-2-5") || value.contains("seedance-2.5") {
                self = .seedance25
            } else if value.contains("seedance-2-0-fast") || value.contains("seedance-2.0-fast") {
                self = .seedance20Fast
            } else if value.contains("seedance-2-0-mini") || value.contains("seedance-2.0-mini") {
                self = .seedance20Mini
            } else if value.contains("seedance-2-0") || value.contains("seedance-2.0") {
                self = .seedance20
            } else {
                self = .openAI
            }
        }
    }

    public var isMiniMax: Bool {
        self == .miniMaxH3 || self == .miniMaxH3Max
    }
    public var isSeedance: Bool {
        switch self {
        case .seedance25, .seedance20, .seedance20Fast, .seedance20Mini: true
        default: false
        }
    }
    public var supportsLastFrame: Bool { isMiniMax || isSeedance }
    public var supportsReferenceVideo: Bool {
        switch self {
        case .miniMaxH3, .miniMaxH3Max,
             .seedance25, .seedance20, .seedance20Fast, .seedance20Mini: true
        case .openAI: false
        }
    }
    public var supportsVideoEditing: Bool { isSeedance }
    public var supportsVideoExtension: Bool { isSeedance }

    public var sizes: [String] {
        switch self {
        case .openAI: ["1280x720", "720x1280", "1792x1024", "1024x1792"]
        case .miniMaxH3: ["768P", "2K"]
        case .miniMaxH3Max: ["768P", "480P"]
        case .seedance25, .seedance20: ["720p", "1080p", "480p"]
        case .seedance20Fast, .seedance20Mini: ["720p", "480p"]
        }
    }

    public var durations: [Int] {
        switch self {
        case .openAI: [4, 8, 12]
        case .miniMaxH3: Array(4...15)
        case .miniMaxH3Max: Array(5...15)
        case .seedance25: Array(4...30)
        case .seedance20, .seedance20Fast, .seedance20Mini: Array(4...15)
        }
    }

    public static let miniMaxRatios = ["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"]
}

public struct VideoGenerationProgress: Sendable, Equatable {
    public var status: String
    public var percent: Double?
    public var jobID: String?

    public init(status: String, percent: Double? = nil, jobID: String? = nil) {
        self.status = status
        self.percent = percent
        self.jobID = jobID
    }
}

public struct VideoGenerationResult: Sendable, Equatable {
    public var id: String
    public var modelConfigID: String
    public var modelName: String
    public var createdAt: String
    public var mimeType: String
    public var videoData: Data

    public init(
        id: String,
        modelConfigID: String,
        modelName: String,
        createdAt: String,
        mimeType: String,
        videoData: Data
    ) {
        self.id = id
        self.modelConfigID = modelConfigID
        self.modelName = modelName
        self.createdAt = createdAt
        self.mimeType = mimeType
        self.videoData = videoData
    }
}

public protocol MediaGenerationServicing: Sendable {
    func fetchModels() async throws -> [MediaGenerationModel]
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult
    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult
}
