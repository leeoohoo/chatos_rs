import ChatOSAPI
import ChatOSConnector
import ChatOSCore
import CryptoKit
import Foundation

protocol MediaStudioHistoryIPCClient: Sendable {
    func mediaRecord(id: String) async throws -> LocalAgentMediaSnapshot
    func mediaRecords() async throws -> [LocalAgentMediaSnapshot]
    func putMediaRecord(
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentMediaDraft
    ) async throws -> LocalAgentMediaMutationResult
    func deleteMediaRecord(
        id: String,
        expectedRevision: UInt64
    ) async throws -> LocalAgentMediaMutationResult
}

extension NativeLocalAgentIPCClient: MediaStudioHistoryIPCClient {}

struct MediaStudioHistoryStorageContext: Sendable {
    let ownerUserID: String
    let client: any MediaStudioHistoryIPCClient
}

/// Media bytes remain in the account's private filesystem. Searchable history,
/// project association, generation status and integrity metadata live only in
/// the selected Rust Client Storage provider through typed Host IPC.
actor MediaStudioHistoryStore {
    typealias ContextProvider = @MainActor @Sendable (String) async throws
        -> MediaStudioHistoryStorageContext

    struct Snapshot: Sendable {
        var images: [MediaStudioViewModel.HistoryItem] = []
        var videos: [MediaStudioViewModel.VideoHistoryItem] = []
        var unreadableCount = 0
    }

    struct PendingGeneration: Sendable {
        let recordID: String
        let revision: UInt64
        let ownerUserID: String
        let projectID: String?
        let kind: LocalAgentMediaKind
        let prompt: String
        let modelName: String
        let generatedAt: String
    }

    private let root: URL
    private let payloadRoot: URL
    private let transport: any HTTPTransport
    private let contextProvider: ContextProvider

    init(
        root: URL? = nil,
        transport: any HTTPTransport = URLSessionHTTPTransport(),
        contextProvider: @escaping ContextProvider
    ) {
        let selectedRoot = root ?? FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        )[0]
            .appendingPathComponent("ChatOSSwift", isDirectory: true)
            .appendingPathComponent("MediaStudioV2", isDirectory: true)
        self.root = selectedRoot.standardizedFileURL
        self.payloadRoot = selectedRoot
            .appendingPathComponent("Payloads", isDirectory: true)
            .standardizedFileURL
        self.transport = transport
        self.contextProvider = contextProvider
    }

    func load(owner: String) async throws -> Snapshot {
        let context = try await context(owner: owner)
        var result = Snapshot()
        for record in try await context.client.mediaRecords() {
            guard record.draft.status == .completed else { continue }
            do {
                switch record.draft.kind {
                case .image:
                    result.images.append(try imageItem(record, owner: context.ownerUserID))
                case .video:
                    result.videos.append(try videoItem(record, owner: context.ownerUserID))
                }
            } catch {
                // Metadata remains authoritative and files remain untouched for recovery.
                result.unreadableCount += 1
            }
        }
        result.images.sort { $0.createdAt > $1.createdAt }
        result.videos.sort { $0.createdAt > $1.createdAt }
        return result
    }

    func beginGeneration(
        kind: LocalAgentMediaKind,
        prompt: String,
        modelName: String,
        owner: String,
        projectID: String? = nil,
        generatedAt: Date = Date()
    ) async throws -> PendingGeneration {
        let context = try await context(owner: owner)
        let recordID = UUID().uuidString.lowercased()
        let timestamp = Self.wireTimestamp(generatedAt)
        let mutation = try await context.client.putMediaRecord(
            id: recordID,
            expectedRevision: nil,
            draft: .init(
                projectID: projectID,
                kind: kind,
                status: .pending,
                prompt: prompt,
                modelName: modelName,
                generatedAt: timestamp,
                assets: []
            )
        )
        guard let record = mutation.record,
              record.ownerUserID == context.ownerUserID,
              record.recordID == recordID,
              record.draft.status == .pending,
              record.draft.assets.isEmpty else {
            throw HistoryError.invalidRecord
        }
        return PendingGeneration(
            recordID: record.recordID,
            revision: record.revision,
            ownerUserID: record.ownerUserID,
            projectID: record.draft.projectID,
            kind: record.draft.kind,
            prompt: record.draft.prompt,
            modelName: record.draft.modelName,
            generatedAt: record.draft.generatedAt
        )
    }

    func markFailed(_ pending: PendingGeneration) async throws {
        let context = try await context(owner: pending.ownerUserID)
        let mutation = try await context.client.putMediaRecord(
            id: pending.recordID,
            expectedRevision: pending.revision,
            draft: .init(
                projectID: pending.projectID,
                kind: pending.kind,
                status: .failed,
                prompt: pending.prompt,
                modelName: pending.modelName,
                generatedAt: pending.generatedAt,
                assets: []
            )
        )
        guard mutation.record?.draft.status == .failed else {
            throw HistoryError.invalidRecord
        }
        removePayloads(
            mutation.discardedPayloadReferences,
            owner: context.ownerUserID,
            recordID: pending.recordID
        )
    }

    func saveImage(
        _ result: ImageGenerationResult,
        prompt: String,
        owner: String,
        projectID: String? = nil
    ) async throws -> MediaStudioViewModel.HistoryItem {
        guard !result.images.isEmpty, result.images.count <= 8 else {
            throw HistoryError.invalidRecord
        }
        let pending = try await beginGeneration(
            kind: .image,
            prompt: prompt,
            modelName: result.modelName,
            owner: owner,
            projectID: projectID,
            generatedAt: Self.date(result.createdAt)
        )
        return try await completeImage(result, pending: pending)
    }

    func completeImage(
        _ result: ImageGenerationResult,
        pending: PendingGeneration
    ) async throws -> MediaStudioViewModel.HistoryItem {
        guard pending.kind == .image,
              !result.images.isEmpty,
              result.images.count <= 8 else {
            throw HistoryError.invalidRecord
        }
        let context = try await context(owner: pending.ownerUserID)
        let folder = try recordFolder(
            owner: context.ownerUserID,
            recordID: pending.recordID,
            create: true
        )
        var assets: [LocalAgentMediaAsset] = []
        for image in result.images {
            try Task.checkCancellation()
            let data = try await imageData(image)
            guard !data.isEmpty, data.count <= 20 * 1_024 * 1_024 else {
                throw HistoryError.invalidRecord
            }
            let filename = "\(UUID().uuidString.lowercased()).\(fileExtension(image.mimeType))"
            try data.write(to: folder.appendingPathComponent(filename), options: [.atomic])
            assets.append(.init(
                assetID: image.id,
                mimeType: image.mimeType,
                payloadReference: payloadReference(
                    owner: context.ownerUserID,
                    recordID: pending.recordID,
                    filename: filename
                ),
                contentHash: Self.digest(data),
                byteCount: UInt64(data.count),
                revisedPrompt: image.revisedPrompt
            ))
        }
        let mutation = try await context.client.putMediaRecord(
            id: pending.recordID,
            expectedRevision: pending.revision,
            draft: .init(
                projectID: pending.projectID,
                kind: .image,
                status: .completed,
                prompt: pending.prompt,
                modelName: result.modelName,
                generatedAt: Self.wireTimestamp(Self.date(result.createdAt)),
                assets: assets
            )
        )
        removePayloads(
            mutation.discardedPayloadReferences,
            owner: context.ownerUserID,
            recordID: pending.recordID
        )
        guard let record = mutation.record else { throw HistoryError.invalidRecord }
        return try imageItem(record, owner: context.ownerUserID)
    }

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String,
        projectID: String? = nil
    ) async throws -> MediaStudioViewModel.VideoHistoryItem {
        guard !result.videoData.isEmpty, result.mimeType.hasPrefix("video/") else {
            throw HistoryError.invalidRecord
        }
        let pending = try await beginGeneration(
            kind: .video,
            prompt: prompt,
            modelName: result.modelName,
            owner: owner,
            projectID: projectID,
            generatedAt: Self.date(result.createdAt)
        )
        return try await completeVideo(result, pending: pending)
    }

    func completeVideo(
        _ result: VideoGenerationResult,
        pending: PendingGeneration
    ) async throws -> MediaStudioViewModel.VideoHistoryItem {
        guard pending.kind == .video,
              !result.videoData.isEmpty,
              result.mimeType.hasPrefix("video/") else {
            throw HistoryError.invalidRecord
        }
        let context = try await context(owner: pending.ownerUserID)
        let folder = try recordFolder(
            owner: context.ownerUserID,
            recordID: pending.recordID,
            create: true
        )
        let filename = "video.\(result.mimeType == "video/quicktime" ? "mov" : "mp4")"
        try result.videoData.write(to: folder.appendingPathComponent(filename), options: [.atomic])
        let mutation = try await context.client.putMediaRecord(
            id: pending.recordID,
            expectedRevision: pending.revision,
            draft: .init(
                projectID: pending.projectID,
                kind: .video,
                status: .completed,
                prompt: pending.prompt,
                modelName: result.modelName,
                generatedAt: Self.wireTimestamp(Self.date(result.createdAt)),
                assets: [.init(
                    assetID: result.id,
                    mimeType: result.mimeType,
                    payloadReference: payloadReference(
                        owner: context.ownerUserID,
                        recordID: pending.recordID,
                        filename: filename
                    ),
                    contentHash: Self.digest(result.videoData),
                    byteCount: UInt64(result.videoData.count),
                    revisedPrompt: nil
                )]
            )
        )
        removePayloads(
            mutation.discardedPayloadReferences,
            owner: context.ownerUserID,
            recordID: pending.recordID
        )
        guard let record = mutation.record else { throw HistoryError.invalidRecord }
        return try videoItem(record, owner: context.ownerUserID)
    }

    private func context(owner: String) async throws -> MediaStudioHistoryStorageContext {
        do {
            let context = try await contextProvider(owner)
            guard !owner.isEmpty, context.ownerUserID == owner else {
                throw HistoryError.storageUnavailable
            }
            return context
        } catch let error as HistoryError {
            throw error
        } catch {
            throw HistoryError.storage(error.localizedDescription)
        }
    }

    private func imageItem(
        _ record: LocalAgentMediaSnapshot,
        owner: String
    ) throws -> MediaStudioViewModel.HistoryItem {
        try validate(record, owner: owner, expectedKind: .image)
        return .init(
            id: record.recordID,
            prompt: record.draft.prompt,
            modelName: record.draft.modelName,
            createdAt: try Self.requiredDate(record.draft.generatedAt),
            images: try record.draft.assets.map { asset in
                .init(
                    id: asset.assetID,
                    mimeType: asset.mimeType,
                    url: try validatedAssetURL(asset, record: record, owner: owner),
                    revisedPrompt: asset.revisedPrompt
                )
            }
        )
    }

    private func videoItem(
        _ record: LocalAgentMediaSnapshot,
        owner: String
    ) throws -> MediaStudioViewModel.VideoHistoryItem {
        try validate(record, owner: owner, expectedKind: .video)
        guard record.draft.assets.count == 1, let asset = record.draft.assets.first else {
            throw HistoryError.invalidRecord
        }
        return .init(
            id: record.recordID,
            prompt: record.draft.prompt,
            modelName: record.draft.modelName,
            createdAt: try Self.requiredDate(record.draft.generatedAt),
            fileURL: try validatedAssetURL(asset, record: record, owner: owner)
        )
    }

    private func validate(
        _ record: LocalAgentMediaSnapshot,
        owner: String,
        expectedKind: LocalAgentMediaKind
    ) throws {
        guard record.ownerUserID == owner,
              record.draft.kind == expectedKind,
              record.draft.status == .completed,
              UUID(uuidString: record.recordID) != nil,
              record.revision > 0,
              !record.draft.assets.isEmpty else {
            throw HistoryError.invalidRecord
        }
    }

    private func validatedAssetURL(
        _ asset: LocalAgentMediaAsset,
        record: LocalAgentMediaSnapshot,
        owner: String
    ) throws -> URL {
        let url = try payloadURL(
            asset.payloadReference,
            owner: owner,
            recordID: record.recordID
        )
        let values = try url.resourceValues(forKeys: [
            .isRegularFileKey,
            .isSymbolicLinkKey,
            .fileSizeKey,
        ])
        guard values.isRegularFile == true,
              values.isSymbolicLink != true,
              values.fileSize.map(UInt64.init) == asset.byteCount,
              try Self.digest(url) == asset.contentHash else {
            throw HistoryError.invalidRecord
        }
        return url
    }

    private func imageData(_ image: GeneratedMediaAsset) async throws -> Data {
        if let base64 = image.base64Data, let decoded = Data(base64Encoded: base64) {
            return decoded
        }
        guard let url = image.url, url.scheme?.lowercased() == "https" else {
            throw HistoryError.invalidRecord
        }
        let response = try await transport.send(.init(
            url: url,
            method: "GET",
            headers: ["Accept": "image/*"],
            timeoutInterval: 120
        ))
        guard (200..<300).contains(response.statusCode) else {
            throw HistoryError.imageDownloadFailed
        }
        return response.body
    }

    private func recordFolder(owner: String, recordID: String, create: Bool) throws -> URL {
        let folder = payloadRoot
            .appendingPathComponent(Self.ownerDirectory(owner), isDirectory: true)
            .appendingPathComponent(recordID, isDirectory: true)
            .standardizedFileURL
        guard folder.path.hasPrefix(payloadRoot.path + "/") else {
            throw HistoryError.invalidRecord
        }
        if create {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        return folder
    }

    private func payloadReference(owner: String, recordID: String, filename: String) -> String {
        "Payloads/\(Self.ownerDirectory(owner))/\(recordID)/\(filename)"
    }

    private func payloadURL(
        _ reference: String,
        owner: String,
        recordID: String
    ) throws -> URL {
        let components = reference.split(separator: "/", omittingEmptySubsequences: false)
        guard components.count == 4,
              components[0] == "Payloads",
              components[1] == Substring(Self.ownerDirectory(owner)),
              components[2] == Substring(recordID),
              !components[3].isEmpty,
              !reference.contains("\\"),
              !reference.contains(":"),
              !reference.contains("..") else {
            throw HistoryError.invalidRecord
        }
        let candidate = root.appendingPathComponent(reference).standardizedFileURL
        guard candidate.path.hasPrefix(payloadRoot.path + "/") else {
            throw HistoryError.invalidRecord
        }
        return candidate
    }

    private func removePayloads(
        _ references: [String],
        owner: String,
        recordID: String
    ) {
        for reference in references {
            guard let url = try? payloadURL(reference, owner: owner, recordID: recordID) else {
                continue
            }
            try? FileManager.default.removeItem(at: url)
        }
    }

    private func fileExtension(_ mimeType: String) -> String {
        switch mimeType.lowercased() {
        case "image/jpeg": "jpg"
        case "image/webp": "webp"
        case "image/gif": "gif"
        default: "png"
        }
    }

    private static func ownerDirectory(_ owner: String) -> String {
        SHA256.hash(data: Data(owner.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
    }

    private static func digest(_ data: Data) -> String {
        "sha256:" + SHA256.hash(data: data)
            .map { String(format: "%02x", $0) }
            .joined()
    }

    private static func digest(_ url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while true {
            let data = try handle.read(upToCount: 1_024 * 1_024) ?? Data()
            if data.isEmpty { break }
            hasher.update(data: data)
        }
        return "sha256:" + hasher.finalize()
            .map { String(format: "%02x", $0) }
            .joined()
    }

    private static func date(_ text: String) -> Date {
        (try? requiredDate(text)) ?? Date()
    }

    private static func requiredDate(_ text: String) throws -> Date {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let value = fractional.date(from: text) ?? ISO8601DateFormatter().date(from: text) else {
            throw HistoryError.invalidRecord
        }
        return value
    }

    private static func wireTimestamp(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.string(from: date)
    }
}

enum HistoryError: LocalizedError {
    case invalidRecord
    case imageDownloadFailed
    case storageUnavailable
    case storage(String)

    var errorDescription: String? {
        switch self {
        case .invalidRecord: "创作文件或记录无效。"
        case .imageDownloadFailed: "未能下载生成的图片到本机。"
        case .storageUnavailable: "当前账户的创作记录存储不可用。"
        case let .storage(message): message
        }
    }
}
