import ChatOSAPI
import ChatOSCore
import CryptoKit
import Foundation

/// Each completed creation has its own atomic manifest and local media files.
/// Relative filenames keep records valid across app reinstalls and directory moves.
actor MediaStudioHistoryStore:
    MediaStudioHistoryLoading,
    MediaStudioImageHistoryPersisting,
    MediaStudioVideoHistoryPersisting,
    MediaStudioDurableVideoHistoryPersisting
{
    private static let maximumManifestBytes = 2 * 1_024 * 1_024
    nonisolated static let defaultPageSize = MediaStudioHistoryUseCase.defaultPageSize

    typealias Cursor = MediaStudioHistoryCursor
    typealias Snapshot = MediaStudioHistoryPage

    private struct Candidate {
        var folder: URL
        var cursor: Cursor
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

    func load(owner: String, after cursor: Cursor? = nil,
              limit requestedLimit: Int = defaultPageSize) async throws -> Snapshot {
        try loadPage(owner: owner, after: cursor, limit: requestedLimit)
    }

    /// A generic durable video uses the same UUID for its job and committed history folder.
    /// Missing records are unreferenced; malformed or symlinked records fail closed.
    func referencesDurableVideoJob(id: UUID, owner: String) throws -> Bool {
        let folder = ownerDirectory(owner).appendingPathComponent(
            id.uuidString.lowercased(),
            isDirectory: true
        )
        guard FileManager.default.fileExists(atPath: folder.path) else { return false }
        let values = try folder.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true else {
            throw HistoryError.invalidRecord
        }
        let entry = try JSONDecoder().decode(
            Entry.self,
            from: AppBoundedFileReader.read(
                folder.appendingPathComponent("record.json"),
                maximumBytes: Self.maximumManifestBytes
            )
        )
        let asset = entry.assets.first
        guard entry.version == 1,
              entry.id == id.uuidString.lowercased(),
              entry.kind == "video",
              entry.assets.count == 1,
              let asset,
              asset.filename == URL(
                  fileURLWithPath: asset.filename
              ).lastPathComponent,
              !asset.filename.hasPrefix(".") else {
            throw HistoryError.invalidRecord
        }
        let assetValues = try folder.appendingPathComponent(asset.filename)
            .resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        guard assetValues.isRegularFile == true, assetValues.isSymbolicLink != true else {
            throw HistoryError.invalidRecord
        }
        return true
    }

    private func loadPage(owner: String, after cursor: Cursor?,
                          limit requestedLimit: Int) throws -> Snapshot {
        let directory = ownerDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return Snapshot() }
        let limit = min(max(requestedLimit, 1), 200)
        guard let folders = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey, .contentModificationDateKey],
            options: [.skipsHiddenFiles, .skipsSubdirectoryDescendants]
        ) else { throw CocoaError(.fileReadUnknown) }
        var candidates: [Candidate] = []
        candidates.reserveCapacity(limit + 1)
        for case let folder as URL in folders {
            try Task.checkCancellation()
            guard (try? folder.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true else { continue }
            let manifest = folder.appendingPathComponent("record.json")
            // An interrupted save has no committed manifest; it is not a completed creation.
            guard FileManager.default.fileExists(atPath: manifest.path) else { continue }
            let modifiedAt = (try? manifest.resourceValues(
                forKeys: [.contentModificationDateKey]
            ).contentModificationDate) ?? .distantPast
            let candidate = Candidate(
                folder: folder,
                cursor: .init(modifiedAt: modifiedAt, recordID: folder.lastPathComponent)
            )
            guard cursor.map({ Self.isAfter(candidate.cursor, cursor: $0) }) ?? true else { continue }
            Self.retain(candidate, in: &candidates, capacity: limit + 1)
        }

        let hasMore = candidates.count > limit
        let page = candidates.prefix(limit)
        var snapshot = Snapshot()
        for candidate in page {
            try Task.checkCancellation()
            let folder = candidate.folder
            let manifest = folder.appendingPathComponent("record.json")
            do {
                let entry = try JSONDecoder().decode(
                    Entry.self,
                    from: AppBoundedFileReader.read(
                        manifest,
                        maximumBytes: Self.maximumManifestBytes
                    )
                )
                guard entry.version == 1, entry.id == folder.lastPathComponent,
                      !entry.assets.isEmpty else { throw HistoryError.invalidRecord }
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
        if hasMore, let last = page.last { snapshot.nextCursor = last.cursor }
        return snapshot
    }

    /// Keeps only the newest `capacity` metadata records while walking the directory. JSON
    /// bodies are decoded only for the requested page, so a large history cannot allocate one
    /// decoded object per record during app activation.
    private static func retain(_ candidate: Candidate, in values: inout [Candidate], capacity: Int) {
        let index = values.firstIndex { isNewer(candidate.cursor, than: $0.cursor) } ?? values.endIndex
        guard index < capacity else { return }
        values.insert(candidate, at: index)
        if values.count > capacity { values.removeLast() }
    }

    private static func isNewer(_ lhs: Cursor, than rhs: Cursor) -> Bool {
        if lhs.modifiedAt != rhs.modifiedAt { return lhs.modifiedAt > rhs.modifiedAt }
        return lhs.recordID > rhs.recordID
    }

    private static func isAfter(_ value: Cursor, cursor: Cursor) -> Bool {
        if value.modifiedAt != cursor.modifiedAt { return value.modifiedAt < cursor.modifiedAt }
        return value.recordID < cursor.recordID
    }

    func saveImage(_ result: ImageGenerationResult, prompt: String, owner: String) async throws -> MediaStudioImageHistoryItem {
        guard !result.images.isEmpty else { throw HistoryError.invalidRecord }
        let folder = try newFolder(owner: owner)
        var assets: [Asset] = []
        for image in result.images {
            try Task.checkCancellation()
            let filename = "\(UUID().uuidString).\(fileExtension(image.mimeType))"
            let destination = folder.appendingPathComponent(filename)
            if let base64 = image.base64Data,
               base64.utf8.count <= MediaStudioImageLoader.maximumEncodedCharacters,
               let decoded = Data(base64Encoded: base64) {
                guard !decoded.isEmpty, decoded.count <= 20 * 1024 * 1024 else {
                    throw HistoryError.invalidRecord
                }
                try decoded.write(to: destination, options: .atomic)
            } else if let url = image.url, url.isFileURL {
                let expectedBytes: Int
                if let mediaFile = image.mediaFile, mediaFile.url == url {
                    expectedBytes = mediaFile.byteCount
                } else {
                    let values = try url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
                    guard values.isRegularFile == true, let fileSize = values.fileSize else {
                        throw HistoryError.invalidRecord
                    }
                    expectedBytes = fileSize
                }
                guard expectedBytes > 0, expectedBytes <= 20 * 1024 * 1024 else {
                    throw HistoryError.invalidRecord
                }
                try await AppCancellableDetachedWork.run {
                    try AppBoundedFileReader.copy(
                        url,
                        to: destination,
                        expectedBytes: expectedBytes,
                        maximumBytes: 20 * 1024 * 1024
                    )
                }
            } else if let url = image.url, url.scheme?.lowercased() == "https" {
                let response = try await transport.send(.init(
                    url: url, method: "GET", headers: ["Accept": "image/*"], timeoutInterval: 120,
                    maximumResponseBytes: 20 * 1_024 * 1_024
                ))
                guard (200..<300).contains(response.statusCode) else { throw HistoryError.imageDownloadFailed }
                guard !response.body.isEmpty, response.body.count <= 20 * 1024 * 1024 else {
                    throw HistoryError.invalidRecord
                }
                try response.body.write(to: destination, options: .atomic)
            } else {
                throw HistoryError.invalidRecord
            }
            assets.append(Asset(id: image.id, mimeType: image.mimeType, filename: filename, revisedPrompt: image.revisedPrompt))
        }
        let entry = Entry(id: folder.lastPathComponent, kind: "image", prompt: prompt,
                          modelName: result.modelName, createdAt: date(result.createdAt), assets: assets)
        try commit(entry, folder: folder)
        return imageItem(entry, folder: folder)
    }

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioVideoHistoryItem {
        try await saveVideo(
            result,
            prompt: prompt,
            owner: owner,
            durableRecordID: UUID().uuidString.lowercased()
        )
    }

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String,
        durableRecordID: String
    ) async throws -> MediaStudioVideoHistoryItem {
        let maximumBytes = 512 * 1_024 * 1_024
        guard (result.videoFile != nil) != (!result.videoData.isEmpty) else {
            throw HistoryError.invalidRecord
        }
        guard UUID(uuidString: durableRecordID) != nil,
              durableRecordID == URL(fileURLWithPath: durableRecordID).lastPathComponent,
              !durableRecordID.hasPrefix(".") else {
            throw HistoryError.invalidRecord
        }
        let folder = ownerDirectory(owner).appendingPathComponent(
            durableRecordID.lowercased(),
            isDirectory: true
        )
        let manifest = folder.appendingPathComponent("record.json")
        if FileManager.default.fileExists(atPath: manifest.path) {
            let entry = try JSONDecoder().decode(
                Entry.self,
                from: AppBoundedFileReader.read(
                    manifest,
                    maximumBytes: Self.maximumManifestBytes
                )
            )
            guard entry.version == 1,
                  entry.id == folder.lastPathComponent,
                  entry.kind == "video",
                  entry.prompt == prompt,
                  entry.assets.count == 1,
                  entry.assets[0].id == result.id,
                  FileManager.default.fileExists(
                      atPath: folder.appendingPathComponent(entry.assets[0].filename).path
                  ) else {
                throw HistoryError.invalidRecord
            }
            return videoItem(entry, folder: folder)
        }
        // Without a committed manifest the folder is an interrupted local finalize, not a
        // visible history record. Reusing the stable ID makes retry cleanup deterministic.
        if FileManager.default.fileExists(atPath: folder.path) {
            try FileManager.default.removeItem(at: folder)
        }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var committed = false
        defer {
            if !committed { try? FileManager.default.removeItem(at: folder) }
        }
        let filename = "video.\(result.mimeType == "video/quicktime" ? "mov" : "mp4")"
        let destinationURL = folder.appendingPathComponent(filename)
        if let videoFile = result.videoFile {
            try await AppCancellableDetachedWork.run {
                try AppBoundedFileReader.copy(
                    videoFile.url,
                    to: destinationURL,
                    expectedBytes: videoFile.byteCount,
                    maximumBytes: maximumBytes
                )
            }
        } else {
            guard result.videoData.count <= maximumBytes else { throw HistoryError.invalidRecord }
            try await AppCancellableDetachedWork.run {
                try result.videoData.write(to: destinationURL, options: .atomic)
            }
        }
        let entry = Entry(id: folder.lastPathComponent, kind: "video", prompt: prompt,
                          modelName: result.modelName, createdAt: date(result.createdAt),
                          assets: [Asset(id: result.id, mimeType: result.mimeType, filename: filename)])
        try commit(entry, folder: folder)
        committed = true
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
        let data = try JSONEncoder().encode(entry)
        guard data.count <= Self.maximumManifestBytes else { throw HistoryError.invalidRecord }
        try data.write(to: folder.appendingPathComponent("record.json"), options: .atomic)
    }

    private func imageItem(_ entry: Entry, folder: URL) -> MediaStudioImageHistoryItem {
        .init(id: entry.id, prompt: entry.prompt, modelName: entry.modelName, createdAt: entry.createdAt,
              images: entry.assets.map {
                  .init(id: $0.id, mimeType: $0.mimeType, url: localURL(folder: folder, filename: $0.filename), revisedPrompt: $0.revisedPrompt)
              })
    }

    private func videoItem(_ entry: Entry, folder: URL) -> MediaStudioVideoHistoryItem {
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
