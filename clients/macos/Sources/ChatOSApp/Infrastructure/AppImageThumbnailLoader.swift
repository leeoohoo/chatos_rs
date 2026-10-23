import AppKit
import ChatOSAPI
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import Darwin
import Foundation
import ImageIO
import SwiftUI

struct AppDecodedThumbnail: @unchecked Sendable {
    let image: CGImage

    var memoryCost: Int {
        image.bytesPerRow * image.height
    }

    @MainActor
    func makeNSImage() -> NSImage {
        // NSImage(cgImage:size:) can create a screen-scale-dependent
        // representation. Keep the decoder's pixel bound and cache cost valid
        // on Retina displays by installing one explicit bitmap representation.
        let size = NSSize(width: image.width, height: image.height)
        let bitmap = NSBitmapImageRep(cgImage: image)
        bitmap.size = size
        let result = NSImage(size: size)
        result.addRepresentation(bitmap)
        return result
    }
}

enum AppImageThumbnailLoader {
    static func loadLocalFileUsingWorker(
        _ url: URL,
        sourceIdentity: MediaWorkerFileIdentity? = nil,
        ownerUserID: String,
        maximumBytes: Int,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared
    ) async throws -> AppDecodedThumbnail {
        guard url.isFileURL,
              !ownerUserID.isEmpty,
              maximumBytes > 0,
              maximumSourcePixelCount > 0,
              maximumDisplayPixelSize > 0,
              maximumDisplayPixelSize <= 8_192 else {
            throw AppImageThumbnailError.invalidImage
        }
        let output = try await ChatOSMediaArtifactFileWriter.writeNormalizedImageUsingWorker(
            maximumBytes: maximumBytes,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride
        ) { destinationURL, _ in
            let descriptor = try await mediaWorker.normalizePNG(
                sourceURL: url,
                sourceIdentity: sourceIdentity,
                destinationURL: destinationURL,
                ownerUserID: ownerUserID,
                maximumSourceBytes: maximumBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumOutputPixelSize: maximumDisplayPixelSize,
                maximumOutputBytes: maximumBytes
            )
            guard descriptor.path == destinationURL.path,
                  descriptor.mimeType == "image/png",
                  descriptor.byteCount > 0,
                  descriptor.byteCount <= maximumBytes,
                  descriptor.pixelWidth > 0,
                  descriptor.pixelHeight > 0,
                  max(descriptor.pixelWidth, descriptor.pixelHeight) <= maximumDisplayPixelSize else {
                throw AppImageThumbnailError.invalidImage
            }
        }
        defer { withExtendedLifetime(output) {} }
        try Task.checkCancellation()
        return try await loadLocalFile(
            output.url,
            maximumBytes: maximumBytes,
            maximumSourcePixelCount: min(
                maximumSourcePixelCount,
                maximumDisplayPixelSize * maximumDisplayPixelSize
            ),
            maximumDisplayPixelSize: maximumDisplayPixelSize
        )
    }

    static func loadLocalFile(
        _ url: URL,
        maximumBytes: Int,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) async throws -> AppDecodedThumbnail {
        guard url.isFileURL else { throw AppImageThumbnailError.unsupportedURL }
        let task = Task.detached(priority: .userInitiated) {
            try Task.checkCancellation()
            guard let decoded = try decodeFile(
                url,
                maximumBytes: maximumBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            ) else {
                throw AppImageThumbnailError.invalidImage
            }
            return decoded
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    nonisolated static func decodeFile(
        _ url: URL,
        maximumBytes: Int,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) throws -> AppDecodedThumbnail? {
        guard url.isFileURL else { throw AppImageThumbnailError.unsupportedURL }
        guard maximumBytes > 0 else { throw AppBoundedFileReadError.invalidLimit }
        try Task.checkCancellation()
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw AppBoundedFileReadError.notRegularFile }
        defer { Darwin.close(descriptor) }

        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size > 0 else {
            throw AppBoundedFileReadError.notRegularFile
        }
        guard status.st_size <= maximumBytes else {
            throw AppBoundedFileReadError.fileTooLarge(maximumBytes: maximumBytes)
        }
        return decodeOpenFileDescriptor(
            descriptor,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize
        )
    }

    nonisolated static func decodeOpenFileDescriptor(
        _ descriptor: Int32,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) -> AppDecodedThumbnail? {
        guard descriptor >= 0,
              lseek(descriptor, 0, SEEK_SET) == 0 else {
            return nil
        }
        let descriptorURL = URL(fileURLWithPath: "/dev/fd/\(descriptor)")
        guard let source = CGImageSourceCreateWithURL(descriptorURL as CFURL, [
            kCGImageSourceShouldCache: false,
        ] as CFDictionary) else {
            return nil
        }
        return decode(
            source,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize
        )
    }

    nonisolated static func decode(
        _ data: Data,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) -> AppDecodedThumbnail? {
        guard !Task.isCancelled,
              !data.isEmpty,
              maximumSourcePixelCount > 0,
              maximumDisplayPixelSize > 0,
              let source = CGImageSourceCreateWithData(data as CFData, [
                kCGImageSourceShouldCache: false,
              ] as CFDictionary) else {
            return nil
        }
        return decode(
            source,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize
        )
    }

    private nonisolated static func decode(
        _ source: CGImageSource,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) -> AppDecodedThumbnail? {
        guard !Task.isCancelled,
              maximumSourcePixelCount > 0,
              maximumDisplayPixelSize > 0,
              CGImageSourceGetCount(source) > 0,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
                as? [CFString: Any],
              let widthValue = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let heightValue = properties[kCGImagePropertyPixelHeight] as? NSNumber else {
            return nil
        }

        let width = widthValue.doubleValue
        let height = heightValue.doubleValue
        guard !Task.isCancelled,
              width.isFinite, height.isFinite,
              width > 0, height > 0,
              width * height <= Double(maximumSourcePixelCount),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumDisplayPixelSize,
                kCGImageSourceShouldCacheImmediately: true,
              ] as CFDictionary) else {
            return nil
        }
        guard !Task.isCancelled else { return nil }
        return AppDecodedThumbnail(image: image)
    }
}

enum AppImageThumbnailError: LocalizedError {
    case unsupportedURL
    case invalidImage

    var errorDescription: String? {
        switch self {
        case .unsupportedURL:
            "只允许读取本机图片文件。"
        case .invalidImage:
            "图片已损坏、尺寸过大或格式不受支持。"
        }
    }
}

@MainActor
final class AppSharedTaskPool<Output: Sendable> {
    private struct Load {
        let id: UUID
        let task: Task<Output?, Never>
        var waiters: Set<UUID>
    }

    private let priority: TaskPriority
    private var loads: [String: Load] = [:]

    init(priority: TaskPriority = .userInitiated) {
        self.priority = priority
    }

    func value(
        for key: String,
        operation: @escaping @Sendable () async -> Output?
    ) async -> Output? {
        guard !Task.isCancelled else { return nil }

        let waiterID = UUID()
        let load: Load
        if var existing = loads[key] {
            existing.waiters.insert(waiterID)
            loads[key] = existing
            load = existing
        } else {
            let created = Load(
                id: UUID(),
                task: Task.detached(priority: priority) {
                    guard !Task.isCancelled else { return nil }
                    return await operation()
                },
                waiters: [waiterID]
            )
            loads[key] = created
            load = created
        }

        return await withTaskCancellationHandler {
            let decoded = await load.task.value
            finishWaiter(waiterID, for: key, loadID: load.id)
            return Task.isCancelled ? nil : decoded
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.cancelWaiter(waiterID, for: key, loadID: load.id)
            }
        }
    }

    private func finishWaiter(_ waiterID: UUID, for key: String, loadID: UUID) {
        guard var load = loads[key], load.id == loadID else { return }
        load.waiters.remove(waiterID)
        if load.waiters.isEmpty {
            loads.removeValue(forKey: key)
        } else {
            loads[key] = load
        }
    }

    private func cancelWaiter(_ waiterID: UUID, for key: String, loadID: UUID) {
        guard var load = loads[key], load.id == loadID else { return }
        load.waiters.remove(waiterID)
        if load.waiters.isEmpty {
            loads.removeValue(forKey: key)
            load.task.cancel()
        } else {
            loads[key] = load
        }
    }
}

typealias AppSharedThumbnailTaskPool = AppSharedTaskPool<AppDecodedThumbnail>

@MainActor
enum AppLocalImageThumbnailCache {
    private static let cache: NSCache<NSString, NSImage> = {
        let cache = NSCache<NSString, NSImage>()
        cache.countLimit = 128
        cache.totalCostLimit = 96 * 1_024 * 1_024
        return cache
    }()
    private static let taskPool = AppSharedThumbnailTaskPool()

    static func image(
        for url: URL,
        maximumBytes: Int,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) async -> NSImage? {
        let key = cacheKey(
            url: url,
            maximumBytes: maximumBytes,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize
        )
        if let cached = cache.object(forKey: key as NSString) {
            return cached
        }

        let decoded = await taskPool.value(for: key) {
            try? await AppImageThumbnailLoader.loadLocalFile(
                url,
                maximumBytes: maximumBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        }
        guard let decoded else { return nil }
        let image = decoded.makeNSImage()
        cache.setObject(image, forKey: key as NSString, cost: decoded.memoryCost)
        return image
    }

    private static func cacheKey(
        url: URL,
        maximumBytes: Int,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) -> String {
        "\(url.standardizedFileURL.path)|\(maximumBytes)|\(maximumSourcePixelCount)|\(maximumDisplayPixelSize)"
    }
}

@MainActor
enum AppImageDataThumbnailCache {
    private static let cache: NSCache<NSString, NSImage> = {
        let cache = NSCache<NSString, NSImage>()
        cache.countLimit = 160
        cache.totalCostLimit = 128 * 1_024 * 1_024
        return cache
    }()
    private static let taskPool = AppSharedThumbnailTaskPool()

    static func dataIdentity(
        _ data: Data,
        identity: String,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) -> String {
        let head = data.prefix(8).map { String(format: "%02x", $0) }.joined()
        let tail = data.suffix(8).map { String(format: "%02x", $0) }.joined()
        return "data|\(identity)|\(data.count)|\(head)|\(tail)|\(maximumSourcePixelCount)|\(maximumDisplayPixelSize)"
    }

    static func base64Identity(
        _ base64: String,
        identity: String,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) -> String {
        "base64|\(identity)|\(base64.count)|\(base64.prefix(12))|\(base64.suffix(12))|\(maximumSourcePixelCount)|\(maximumDisplayPixelSize)"
    }

    static func image(
        for data: Data,
        cacheKey: String,
        maximumBytes: Int,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) async -> NSImage? {
        if let cached = cache.object(forKey: cacheKey as NSString) {
            return cached
        }
        guard !data.isEmpty, data.count <= maximumBytes else { return nil }
        return await resolve(cacheKey: cacheKey) {
            AppImageThumbnailLoader.decode(
                data,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        }
    }

    static func image(
        forBase64 base64: String,
        cacheKey: String,
        maximumEncodedCharacters: Int,
        maximumSourcePixelCount: Int,
        maximumDisplayPixelSize: Int
    ) async -> NSImage? {
        if let cached = cache.object(forKey: cacheKey as NSString) {
            return cached
        }
        guard !base64.isEmpty, base64.count <= maximumEncodedCharacters else { return nil }
        return await resolve(cacheKey: cacheKey) {
            guard let data = Data(base64Encoded: base64), !data.isEmpty else { return nil }
            return AppImageThumbnailLoader.decode(
                data,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        }
    }

    private static func resolve(
        cacheKey: String,
        operation: @escaping @Sendable () async -> AppDecodedThumbnail?
    ) async -> NSImage? {
        let decoded = await taskPool.value(for: cacheKey, operation: operation)
        guard let decoded else { return nil }
        let image = decoded.makeNSImage()
        cache.setObject(image, forKey: cacheKey as NSString, cost: decoded.memoryCost)
        return image
    }
}

struct AppAsyncLocalFileImage<Content: View, Placeholder: View>: View {
    let url: URL
    let maximumBytes: Int
    let maximumSourcePixelCount: Int
    let maximumDisplayPixelSize: Int
    @ViewBuilder let content: (Image) -> Content
    @ViewBuilder let placeholder: () -> Placeholder
    @State private var image: NSImage?

    init(
        url: URL,
        maximumBytes: Int = 20 * 1_024 * 1_024,
        maximumSourcePixelCount: Int = 64_000_000,
        maximumDisplayPixelSize: Int,
        @ViewBuilder content: @escaping (Image) -> Content,
        @ViewBuilder placeholder: @escaping () -> Placeholder
    ) {
        self.url = url
        self.maximumBytes = maximumBytes
        self.maximumSourcePixelCount = maximumSourcePixelCount
        self.maximumDisplayPixelSize = maximumDisplayPixelSize
        self.content = content
        self.placeholder = placeholder
    }

    var body: some View {
        Group {
            if let image {
                content(Image(nsImage: image))
            } else {
                placeholder()
            }
        }
        .task(id: url.standardizedFileURL) {
            image = nil
            let loaded = await AppLocalImageThumbnailCache.image(
                for: url,
                maximumBytes: maximumBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
            guard !Task.isCancelled else { return }
            image = loaded
        }
    }
}

struct AppAsyncDataImage<Content: View, Placeholder: View>: View {
    let data: Data
    let identity: String
    let maximumBytes: Int
    let maximumSourcePixelCount: Int
    let maximumDisplayPixelSize: Int
    @ViewBuilder let content: (Image) -> Content
    @ViewBuilder let placeholder: () -> Placeholder
    @State private var image: NSImage?

    init(
        data: Data,
        identity: String,
        maximumBytes: Int = 20 * 1_024 * 1_024,
        maximumSourcePixelCount: Int = 64_000_000,
        maximumDisplayPixelSize: Int,
        @ViewBuilder content: @escaping (Image) -> Content,
        @ViewBuilder placeholder: @escaping () -> Placeholder
    ) {
        self.data = data
        self.identity = identity
        self.maximumBytes = maximumBytes
        self.maximumSourcePixelCount = maximumSourcePixelCount
        self.maximumDisplayPixelSize = maximumDisplayPixelSize
        self.content = content
        self.placeholder = placeholder
    }

    private var loadIdentity: String {
        AppImageDataThumbnailCache.dataIdentity(
            data,
            identity: identity,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize
        )
    }

    var body: some View {
        Group {
            if let image {
                content(Image(nsImage: image))
            } else {
                placeholder()
            }
        }
        .task(id: loadIdentity) {
            image = nil
            let loaded = await AppImageDataThumbnailCache.image(
                for: data,
                cacheKey: loadIdentity,
                maximumBytes: maximumBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
            guard !Task.isCancelled else { return }
            image = loaded
        }
    }
}

struct AppAsyncBase64Image<Content: View, Placeholder: View>: View {
    let base64: String
    let identity: String
    let maximumEncodedCharacters: Int
    let maximumSourcePixelCount: Int
    let maximumDisplayPixelSize: Int
    @ViewBuilder let content: (Image) -> Content
    @ViewBuilder let placeholder: () -> Placeholder
    @State private var image: NSImage?

    init(
        base64: String,
        identity: String,
        maximumEncodedCharacters: Int = 28 * 1_024 * 1_024,
        maximumSourcePixelCount: Int = 64_000_000,
        maximumDisplayPixelSize: Int,
        @ViewBuilder content: @escaping (Image) -> Content,
        @ViewBuilder placeholder: @escaping () -> Placeholder
    ) {
        self.base64 = base64
        self.identity = identity
        self.maximumEncodedCharacters = maximumEncodedCharacters
        self.maximumSourcePixelCount = maximumSourcePixelCount
        self.maximumDisplayPixelSize = maximumDisplayPixelSize
        self.content = content
        self.placeholder = placeholder
    }

    private var loadIdentity: String {
        AppImageDataThumbnailCache.base64Identity(
            base64,
            identity: identity,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize
        )
    }

    var body: some View {
        Group {
            if let image {
                content(Image(nsImage: image))
            } else {
                placeholder()
            }
        }
        .task(id: loadIdentity) {
            image = nil
            let loaded = await AppImageDataThumbnailCache.image(
                forBase64: base64,
                cacheKey: loadIdentity,
                maximumEncodedCharacters: maximumEncodedCharacters,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
            guard !Task.isCancelled else { return }
            image = loaded
        }
    }
}
