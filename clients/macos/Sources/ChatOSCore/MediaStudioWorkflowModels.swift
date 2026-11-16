import Foundation

public enum MediaStudioWorkflowNodeKind: String, Codable, CaseIterable, Sendable {
    case text
    case image
    case video
}

public struct MediaStudioWorkflowImageReference: Codable, Identifiable, Equatable, Sendable {
    public var id: UUID
    public var name: String
    public var mimeType: String
    public var filename: String
    public var byteCount: Int

    public init(
        id: UUID = UUID(),
        name: String,
        mimeType: String,
        filename: String,
        byteCount: Int
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.filename = filename
        self.byteCount = byteCount
    }
}

public struct MediaStudioWorkflowNode: Codable, Identifiable, Equatable, Sendable {
    public var id: UUID
    public var kind: MediaStudioWorkflowNodeKind
    public var modelConfigID: String
    public var prompt: String
    public var referenceImages: [MediaStudioWorkflowImageReference]
    public var imageSize: String
    public var videoSize: String
    public var videoRatio: String
    public var videoSeconds: Int

    public init(
        id: UUID = UUID(),
        kind: MediaStudioWorkflowNodeKind,
        modelConfigID: String,
        prompt: String,
        referenceImages: [MediaStudioWorkflowImageReference] = [],
        imageSize: String = "1024x1024",
        videoSize: String = "1280x720",
        videoRatio: String = "16:9",
        videoSeconds: Int = 4
    ) {
        self.id = id
        self.kind = kind
        self.modelConfigID = modelConfigID
        self.prompt = prompt
        self.referenceImages = referenceImages
        self.imageSize = imageSize
        self.videoSize = videoSize
        self.videoRatio = videoRatio
        self.videoSeconds = videoSeconds
    }
}

public struct MediaStudioWorkflow: Codable, Identifiable, Equatable, Sendable {
    public var id: UUID
    public var name: String
    public var inputPrompt: String
    public var inputImages: [MediaStudioWorkflowImageReference]
    public var nodes: [MediaStudioWorkflowNode]
    public var createdAt: Date
    public var updatedAt: Date

    public init(
        id: UUID = UUID(),
        name: String,
        inputPrompt: String = "",
        inputImages: [MediaStudioWorkflowImageReference] = [],
        nodes: [MediaStudioWorkflowNode] = [],
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.name = name
        self.inputPrompt = inputPrompt
        self.inputImages = inputImages
        self.nodes = nodes
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public enum MediaStudioWorkflowValidationError: LocalizedError, Equatable, Sendable {
    case invalidName
    case invalidPrompt
    case invalidNodeCount
    case duplicateNode
    case invalidModel
    case invalidReference
    case tooManyReferences
    case invalidVideoPosition
    case invalidVideoOptions

    public var errorDescription: String? {
        switch self {
        case .invalidName: "工作流名称无效。"
        case .invalidPrompt: "节点提示词不能为空或过长。"
        case .invalidNodeCount: "工作流必须包含 1–8 个模型节点。"
        case .duplicateNode: "工作流包含重复节点。"
        case .invalidModel: "节点缺少有效模型。"
        case .invalidReference: "工作流参考图无效。"
        case .tooManyReferences: "文本和图片节点最多 8 张节点参考图，视频节点最多 1 张。"
        case .invalidVideoPosition: "视频节点只能出现一次并且必须位于流程末尾。"
        case .invalidVideoOptions: "视频节点尺寸、比例或时长无效。"
        }
    }
}

public enum MediaStudioWorkflowValidator {
    public static let maximumNodeCount = 8
    public static let maximumReferenceCount = 8
    public static let maximumImageBytes = 20 * 1_024 * 1_024
    private static let maximumTextLength = 20_000

    public static func validate(_ workflow: MediaStudioWorkflow) throws {
        let name = workflow.name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name.count <= 128 else {
            throw MediaStudioWorkflowValidationError.invalidName
        }
        guard workflow.inputPrompt.count <= maximumTextLength,
              (1...maximumNodeCount).contains(workflow.nodes.count) else {
            throw workflow.nodes.isEmpty || workflow.nodes.count > maximumNodeCount
                ? MediaStudioWorkflowValidationError.invalidNodeCount
                : MediaStudioWorkflowValidationError.invalidPrompt
        }
        guard Set(workflow.nodes.map(\.id)).count == workflow.nodes.count else {
            throw MediaStudioWorkflowValidationError.duplicateNode
        }
        try validateReferences(workflow.inputImages, maximum: maximumReferenceCount)
        let videoIndexes = workflow.nodes.indices.filter { workflow.nodes[$0].kind == .video }
        guard videoIndexes.isEmpty || (videoIndexes.count == 1 && videoIndexes[0] == workflow.nodes.count - 1) else {
            throw MediaStudioWorkflowValidationError.invalidVideoPosition
        }
        for node in workflow.nodes {
            guard !node.modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                throw MediaStudioWorkflowValidationError.invalidModel
            }
            let prompt = node.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !prompt.isEmpty, prompt.count <= maximumTextLength else {
                throw MediaStudioWorkflowValidationError.invalidPrompt
            }
            try validateReferences(
                node.referenceImages,
                maximum: node.kind == .video ? 1 : maximumReferenceCount
            )
            if node.kind == .video {
                guard !node.videoSize.isEmpty, !node.videoRatio.isEmpty,
                      (1...120).contains(node.videoSeconds) else {
                    throw MediaStudioWorkflowValidationError.invalidVideoOptions
                }
            }
        }
    }

    private static func validateReferences(
        _ references: [MediaStudioWorkflowImageReference],
        maximum: Int
    ) throws {
        guard references.count <= maximum else {
            throw MediaStudioWorkflowValidationError.tooManyReferences
        }
        guard Set(references.map(\.id)).count == references.count else {
            throw MediaStudioWorkflowValidationError.invalidReference
        }
        for reference in references {
            guard reference.name.count <= 255,
                  reference.mimeType.lowercased().hasPrefix("image/"),
                  reference.filename == URL(fileURLWithPath: reference.filename).lastPathComponent,
                  !reference.filename.hasPrefix("."),
                  (1...maximumImageBytes).contains(reference.byteCount) else {
                throw MediaStudioWorkflowValidationError.invalidReference
            }
        }
    }
}
