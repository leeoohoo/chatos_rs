import ChatOSCore
import Foundation

enum MediaStudioWorkflowRunEvent: Sendable {
    case started(UUID)
    case progress(UUID, VideoGenerationProgress)
    case completed(UUID, String)
}

struct MediaStudioWorkflowRunResult: Sendable {
    var text: String
    var images: [ImageGenerationInputImage]
    var imageHistory: [MediaStudioImageHistoryItem]
    var videoHistory: [MediaStudioVideoHistoryItem]
}

struct MediaStudioWorkflowRunInput: Sendable {
    var prompt: String
    var images: [ImageGenerationInputImage]
}

struct MediaStudioWorkflowRunner: Sendable {
    let textGenerator: any MediaStudioTextGenerating
    let imageGenerator: MediaStudioImageGenerationUseCase
    let videoGenerator: MediaStudioVideoJobService
    let store: MediaStudioWorkflowStore

    func run(
        _ workflow: MediaStudioWorkflow,
        input: MediaStudioWorkflowRunInput,
        owner: String,
        sessionEpoch: UUID,
        event: @escaping @Sendable (MediaStudioWorkflowRunEvent) async -> Void
    ) async throws -> MediaStudioWorkflowRunResult {
        try MediaStudioWorkflowValidator.validate(workflow)
        var text = input.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        var images = input.images
        var imageHistory: [MediaStudioImageHistoryItem] = []
        var videoHistory: [MediaStudioVideoHistoryItem] = []

        for node in workflow.nodes {
            try Task.checkCancellation()
            await event(.started(node.id))
            let references = try await store.resolveImages(
                node.referenceImages,
                workflowID: workflow.id,
                owner: owner
            )
            let prompt = Self.prompt(node.prompt, currentText: text)
            switch node.kind {
            case .text:
                text = try await textGenerator.generateText(.init(
                    modelConfigID: node.modelConfigID,
                    prompt: prompt,
                    images: images + references
                ))
                await event(.completed(node.id, text))
            case .image:
                let inputs = images + references
                guard inputs.count <= MediaStudioWorkflowValidator.maximumReferenceCount else {
                    throw MediaStudioWorkflowRunError.tooManyImages
                }
                let outcome = try await imageGenerator.generate(
                    request: .init(
                        modelConfigID: node.modelConfigID,
                        prompt: prompt,
                        size: node.imageSize == "auto" ? nil : node.imageSize,
                        count: 1,
                        referenceImages: inputs
                    ),
                    owner: owner
                )
                guard case let .saved(item) = outcome else {
                    throw MediaStudioWorkflowRunError.imagePersistenceFailed
                }
                guard let asset = item.images.first,
                      let url = asset.url, url.isFileURL else {
                    throw MediaStudioWorkflowRunError.missingImageOutput
                }
                images = [.init(
                    name: "workflow-\(asset.id).\(url.pathExtension)",
                    mimeType: asset.mimeType,
                    fileURL: url
                )]
                imageHistory.append(item)
                await event(.completed(node.id, "已生成图片"))
            case .video:
                let firstFrame = references.first ?? images.last
                let outcome = try await videoGenerator.submit(
                    request: .init(
                        modelConfigID: node.modelConfigID,
                        prompt: prompt,
                        size: node.videoSize,
                        seconds: node.videoSeconds,
                        inputImage: firstFrame,
                        ratio: node.videoRatio
                    ),
                    owner: owner,
                    sessionEpoch: sessionEpoch
                ) { progress in
                    await event(.progress(node.id, progress))
                }
                guard case let .saved(item) = outcome else {
                    throw MediaStudioWorkflowRunError.videoPersistenceFailed
                }
                videoHistory.append(item)
                await event(.completed(node.id, "已生成视频"))
            }
        }
        return .init(
            text: text,
            images: images,
            imageHistory: imageHistory,
            videoHistory: videoHistory
        )
    }

    static func prompt(_ nodePrompt: String, currentText: String) -> String {
        """
        节点要求：
        \(nodePrompt.trimmingCharacters(in: .whitespacesAndNewlines))

        当前工作文本：
        \(currentText)
        """
    }
}

enum MediaStudioWorkflowRunError: LocalizedError, Equatable {
    case tooManyImages
    case missingImageOutput
    case imagePersistenceFailed
    case videoPersistenceFailed

    var errorDescription: String? {
        switch self {
        case .tooManyImages: "当前图片与节点参考图合计不能超过 8 张。"
        case .missingImageOutput: "图片节点没有返回可传递给下一节点的本机图片。"
        case .imagePersistenceFailed: "图片已生成但保存失败，工作流已停止。"
        case .videoPersistenceFailed: "视频已生成但保存失败，工作流已停止。"
        }
    }
}
