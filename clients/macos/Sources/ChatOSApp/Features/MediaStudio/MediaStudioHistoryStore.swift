import ChatOSAPI
import ChatOSCore
import CryptoKit
import Foundation

/// Each completed creation has its own atomic manifest and local media files.
/// Relative filenames keep records valid across app reinstalls and directory moves.
actor MediaStudioHistoryStore {
    struct Snapshot: Sendable {
        var images: [MediaStudioViewModel.HistoryItem] = []
        var videos: [MediaStudioViewModel.VideoHistoryItem] = []
        var unreadableCount = 0
    }

    private struct Asset: Codable {
        var id: String
        var mimeType: String
        var filename: String
        var revisedPrompt: String?
    }

    private struct Entry: Codable {
        var version = 1
        var id: String
        var kind: String
        var prompt: String
        var modelName: String
        var createdAt: Date
        var assets: [Asset]
    }

    private let root: URL
    private let transport: any HTTPTransport

    init(root: URL? = nil, transport: any HTTPTransport = URLSessionHTTPTransport()) {
        self.root = (root ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ChatOSSwift/MediaStudio", isDirectory: true)).resolvingSymlinksInPath()
        self.transport = transport
    }

    func load(owner: String) throws -> Snapshot {
        let directory = ownerDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return Snapshot() }
        let folders = try FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles]
        )
        var snapshot = Snapshot()
        for folder in folders where (try? folder.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true {
            let manifest = folder.appendingPathComponent("record.json")
            // An interrupted save has no committed manifest; it is not a completed creation.
            guard FileManager.default.fileExists(atPath: manifest.path) else { continue }
            do {
                let entry = try JSONDecoder().decode(Entry.self, from: Data(contentsOf: manifest))
                guard entry.version == 1, !entry.assets.isEmpty else { throw HistoryError.invalidRecord }
                for asset in entry.assets {
                    guard asset.filename == URL(fileURLWithPath: asset.filename).lastPathComponent,
                          !asset.filename.hasPrefix("."),
                          FileManager.default.fileExists(atPath: folder.appendingPathComponent(asset.filename).path)
                    else { throw HistoryError.invalidRecord }
                }
                switch entry.kind {
                case "image": snapshot.images.append(imageItem(entry, folder: folder))
                case "video": snapshot.videos.append(videoItem(entry, folder: folder))
                default: throw HistoryError.invalidRecord
                }
            } catch {
                // Preserve damaged files for recovery and keep other records visible.
                snapshot.unreadableCount += 1
            }
        }
        snapshot.images.sort { $0.createdAt > $1.createdAt }
        snapshot.videos.sort { $0.createdAt > $1.createdAt }
        return snapshot
    }

    func saveImage(_ result: ImageGenerationResult, prompt: String, owner: String) async throws -> MediaStudioViewModel.HistoryItem {
        guard !result.images.isEmpty else { throw HistoryError.invalidRecord }
        let folder = try newFolder(owner: owner)
        var assets: [Asset] = []
        for image in result.images {
            try Task.checkCancellation()
            let data: Data
            if let base64 = image.base64Data, let decoded = Data(base64Encoded: base64) {
                data = decoded
            } else if let url = image.url, url.scheme?.lowercased() == "https" {
                let response = try await transport.send(.init(
                    url: url, method: "GET", headers: ["Accept": "image/*"], timeoutInterval: 120
                ))
                guard (200..<300).contains(response.statusCode) else { throw HistoryError.imageDownloadFailed }
                data = response.body
            } else {
                throw HistoryError.invalidRecord
            }
            guard !data.isEmpty, data.count <= 20 * 1024 * 1024 else { throw HistoryError.invalidRecord }
            let filename = "\(UUID().uuidString).\(fileExtension(image.mimeType))"
            try data.write(to: folder.appendingPathComponent(filename), options: .atomic)
            assets.append(Asset(id: image.id, mimeType: image.mimeType, filename: filename, revisedPrompt: image.revisedPrompt))
        }
        let entry = Entry(id: folder.lastPathComponent, kind: "image", prompt: prompt,
                          modelName: result.modelName, createdAt: date(result.createdAt), assets: assets)
        try commit(entry, folder: folder)
        return imageItem(entry, folder: folder)
    }

    func saveVideo(_ result: VideoGenerationResult, prompt: String, owner: String) throws -> MediaStudioViewModel.VideoHistoryItem {
        guard !result.videoData.isEmpty else { throw HistoryError.invalidRecord }
        let folder = try newFolder(owner: owner)
        let filename = "video.\(result.mimeType == "video/quicktime" ? "mov" : "mp4")"
        try result.videoData.write(to: folder.appendingPathComponent(filename), options: .atomic)
        let entry = Entry(id: folder.lastPathComponent, kind: "video", prompt: prompt,
                          modelName: result.modelName, createdAt: date(result.createdAt),
                          assets: [Asset(id: result.id, mimeType: result.mimeType, filename: filename)])
        try commit(entry, folder: folder)
        return videoItem(entry, folder: folder)
    }

    private func ownerDirectory(_ owner: String) -> URL {
        let key = SHA256.hash(data: Data(owner.utf8)).map { String(format: "%02x", $0) }.joined()
        return root.appendingPathComponent("Accounts/\(key)", isDirectory: true)
    }

    private func newFolder(owner: String) throws -> URL {
        let folder = ownerDirectory(owner).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    private func commit(_ entry: Entry, folder: URL) throws {
        try JSONEncoder().encode(entry).write(to: folder.appendingPathComponent("record.json"), options: .atomic)
    }

    private func imageItem(_ entry: Entry, folder: URL) -> MediaStudioViewModel.HistoryItem {
        .init(id: entry.id, prompt: entry.prompt, modelName: entry.modelName, createdAt: entry.createdAt,
              images: entry.assets.map {
                  .init(id: $0.id, mimeType: $0.mimeType, url: localURL(folder: folder, filename: $0.filename), revisedPrompt: $0.revisedPrompt)
              })
    }

    private func videoItem(_ entry: Entry, folder: URL) -> MediaStudioViewModel.VideoHistoryItem {
        .init(id: entry.id, prompt: entry.prompt, modelName: entry.modelName, createdAt: entry.createdAt,
              fileURL: localURL(folder: folder, filename: entry.assets[0].filename))
    }

    private func localURL(folder: URL, filename: String) -> URL {
        // Directory enumeration can return a different alias (e.g. /private/var).
        // Normalize existing files consistently for both new and restored records.
        folder.appendingPathComponent(filename).resolvingSymlinksInPath()
    }

    private func date(_ text: String) -> Date {
        ISO8601DateFormatter().date(from: text) ?? Date()
    }

    private func fileExtension(_ mimeType: String) -> String {
        switch mimeType.lowercased() {
        case "image/jpeg": "jpg"
        case "image/webp": "webp"
        case "image/gif": "gif"
        default: "png"
        }
    }
}

private enum HistoryError: LocalizedError {
    case invalidRecord, imageDownloadFailed

    var errorDescription: String? {
        switch self {
        case .invalidRecord: "创作文件或记录无效。"
        case .imageDownloadFailed: "未能下载生成的图片到本机。"
        }
    }
}
