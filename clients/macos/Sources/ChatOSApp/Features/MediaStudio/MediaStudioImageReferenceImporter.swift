import ChatOSAPI
import ChatOSCore
import Foundation
import ImageIO
import UniformTypeIdentifiers

enum MediaStudioInputImagePolicy {
    static let maximumBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000
}

struct MediaStudioImageReferenceImporter: MediaStudioImageReferenceImporting {
    private let transport: any HTTPTransport
    private let artifacts: MediaStudioInputArtifactStore

    init(
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        artifacts: MediaStudioInputArtifactStore = MediaStudioInputArtifactStore()
    ) {
        self.transport = transport
        self.artifacts = artifacts
    }

    func importLocalFiles(_ urls: [URL]) async throws -> [ImageGenerationInputImage] {
        var imported: [ImageGenerationInputImage] = []
        imported.reserveCapacity(urls.count)
        do {
            for url in urls {
                try Task.checkCancellation()
                imported.append(try await artifacts.importImage(from: url))
            }
            return imported
        } catch {
            for image in imported { artifacts.removeIfManaged(image.fileURL) }
            throw error
        }
    }

    func importGeneratedAssets(
        _ assets: [GeneratedMediaAsset]
    ) async throws -> [ImageGenerationInputImage] {
        var imported: [ImageGenerationInputImage] = []
        imported.reserveCapacity(assets.count)
        for asset in assets {
            try Task.checkCancellation()
            do {
                let name = "generated-\(asset.id).\(Self.fileExtension(for: asset.mimeType))"
                if let url = asset.url, url.isFileURL {
                    imported.append(try await artifacts.importImage(
                        from: url,
                        name: name,
                        mimeType: asset.mimeType
                    ))
                } else {
                    let data = try await MediaStudioImageLoader.data(
                        for: asset,
                        transport: transport
                    )
                    imported.append(try await artifacts.importImage(
                        data: data,
                        name: name,
                        mimeType: asset.mimeType
                    ))
                }
            } catch {
                for image in imported { artifacts.removeIfManaged(image.fileURL) }
                throw error
            }
        }
        return imported
    }

    private static func fileExtension(for mimeType: String) -> String {
        UTType(mimeType: mimeType)?.preferredFilenameExtension ?? "png"
    }
}

enum MediaStudioInputImageError: LocalizedError {
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
