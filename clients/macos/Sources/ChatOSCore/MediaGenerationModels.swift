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
}

public struct ImageGenerationRequest: Codable, Sendable, Equatable {
    public var modelConfigID: String
    public var prompt: String
    public var size: String?
    public var count: Int
    public var inputImage: ImageGenerationInputImage?
    public var referenceImages: [ImageGenerationInputImage]

    public init(
        modelConfigID: String,
        prompt: String,
        size: String?,
        count: Int,
        inputImage: ImageGenerationInputImage? = nil,
        referenceImages: [ImageGenerationInputImage] = []
    ) {
        self.modelConfigID = modelConfigID
        self.prompt = prompt
        self.size = size
        self.count = count
        self.inputImage = inputImage
        self.referenceImages = referenceImages
    }

    enum CodingKeys: String, CodingKey {
        case modelConfigID = "model_config_id"
        case prompt, size, count
        case inputImage = "input_image"
        case referenceImages = "reference_images"
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        modelConfigID = try values.decode(String.self, forKey: .modelConfigID)
        prompt = try values.decode(String.self, forKey: .prompt)
        size = try values.decodeIfPresent(String.self, forKey: .size)
        count = try values.decode(Int.self, forKey: .count)
        inputImage = try values.decodeIfPresent(ImageGenerationInputImage.self, forKey: .inputImage)
        referenceImages = try values.decodeIfPresent([ImageGenerationInputImage].self, forKey: .referenceImages) ?? []
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

    public init(
        id: String,
        modelConfigID: String,
        modelName: String,
        createdAt: String,
        images: [GeneratedMediaAsset]
    ) {
        self.id = id
        self.modelConfigID = modelConfigID
        self.modelName = modelName
        self.createdAt = createdAt
        self.images = images
    }

    enum CodingKeys: String, CodingKey {
        case id
        case modelConfigID = "model_config_id"
        case modelName = "model"
        case createdAt = "created_at"
        case images
    }
}

public struct VideoGenerationRequest: Sendable, Equatable {
    public var modelConfigID: String
    public var prompt: String
    public var size: String
    public var seconds: Int
    public var inputImage: ImageGenerationInputImage?
    public var ratio: String

    public init(
        modelConfigID: String,
        prompt: String,
        size: String,
        seconds: Int,
        inputImage: ImageGenerationInputImage? = nil,
        ratio: String = "16:9"
    ) {
        self.modelConfigID = modelConfigID
        self.prompt = prompt
        self.size = size
        self.seconds = seconds
        self.inputImage = inputImage
        self.ratio = ratio
    }
}

/// Shared by request validation and the creation form. Values follow MiniMax's V2 API.
public enum VideoGenerationProfile: Sendable {
    case openAI, miniMaxH3, miniMaxH3Max

    public init(modelName: String) {
        switch modelName.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "minimax-h3": self = .miniMaxH3
        case "minimax-h3-max": self = .miniMaxH3Max
        default: self = .openAI
        }
    }

    public var isMiniMax: Bool { self != .openAI }

    public var sizes: [String] {
        switch self {
        case .openAI: ["1280x720", "720x1280", "1792x1024", "1024x1792"]
        case .miniMaxH3: ["768P", "2K"]
        case .miniMaxH3Max: ["768P", "480P"]
        }
    }

    public var durations: [Int] {
        switch self {
        case .openAI: [4, 8, 12]
        case .miniMaxH3: Array(4...15)
        case .miniMaxH3Max: Array(5...15)
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
