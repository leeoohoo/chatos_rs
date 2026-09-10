import ChatOSAPI
import ChatOSCore
import Foundation

enum MediaStudioImageLoader {
    static func data(for asset: GeneratedMediaAsset, transport: any HTTPTransport = URLSessionHTTPTransport()) async throws -> Data {
        try Task.checkCancellation()
        let data: Data
        if let base64 = asset.base64Data, let decoded = Data(base64Encoded: base64) {
            data = decoded
        } else if let url = asset.url, url.isFileURL {
            data = try await Task.detached(priority: .userInitiated) {
                let values = try url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
                guard values.isRegularFile == true, (values.fileSize ?? 0) <= 20 * 1024 * 1024 else {
                    throw ImageError.invalidImage
                }
                return try Data(contentsOf: url)
            }.value
        } else if let url = asset.url, url.scheme?.lowercased() == "https", url.host != nil,
                  url.user == nil, url.password == nil {
            // Generated CDN URLs authorize themselves; do not send a model API key.
            let response = try await transport.send(.init(
                url: url, method: "GET", headers: ["Accept": "image/*"], timeoutInterval: 60
            ))
            guard (200..<300).contains(response.statusCode) else { throw ImageError.downloadFailed }
            data = response.body
        } else {
            throw ImageError.invalidImage
        }
        try Task.checkCancellation()
        guard !data.isEmpty, data.count <= 20 * 1024 * 1024 else { throw ImageError.invalidImage }
        return data
    }

    enum ImageError: LocalizedError {
        case invalidImage, downloadFailed
        var errorDescription: String? {
            switch self {
            case .invalidImage: "无法读取图片，文件可能已丢失、损坏或超过 20 MB。"
            case .downloadFailed: "无法下载这张图片，请稍后重试。"
            }
        }
    }
}
