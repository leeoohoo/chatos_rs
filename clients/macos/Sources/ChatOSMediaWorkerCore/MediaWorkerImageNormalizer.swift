import ChatOSMediaWorkerProtocol
import AVFoundation
import CoreGraphics
import CryptoKit
import Darwin
import Foundation
import ImageIO

public enum MediaWorkerCoreError: Error, Sendable, Equatable {
    case invalidRequest(String)
    case unauthorizedPath(String)
    case invalidImage(String)
    case deadlineExceeded
    case outputTooLarge
    case insufficientDiskSpace

    public var payload: MediaWorkerErrorPayload {
        switch self {
        case let .invalidRequest(message):
            .init(code: "invalid_request", message: message)
        case let .unauthorizedPath(message):
            .init(code: "unauthorized_path", message: message)
        case let .invalidImage(message):
            .init(code: "invalid_image", message: message)
        case .deadlineExceeded:
            .init(code: "deadline_exceeded", message: "The media operation deadline expired.")
        case .outputTooLarge:
            .init(code: "output_too_large", message: "The normalized image exceeds its byte limit.")
        case .insufficientDiskSpace:
            .init(code: "insufficient_disk_space", message: "The destination volume is full.")
        }
    }
}

public enum ChatOSMediaWorkerEngine {
    public static func normalizePNG(
        _ request: MediaWorkerNormalizePNGRequest
    ) throws -> MediaWorkerImageDescriptor {
        try validateRequest(request)
        try checkDeadline(request.deadlineUnixMilliseconds)

        let sourceDescriptor = try openAuthorizedSource(request)
        defer { _ = Darwin.close(sourceDescriptor) }
        let destinationDescriptor = try openAuthorizedDestination(request)
        defer { _ = Darwin.close(destinationDescriptor) }

        var sourceStatus = stat()
        var destinationStatus = stat()
        guard fstat(sourceDescriptor, &sourceStatus) == 0,
              fstat(destinationDescriptor, &destinationStatus) == 0,
              sourceStatus.st_dev != destinationStatus.st_dev
                || sourceStatus.st_ino != destinationStatus.st_ino else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "Source and destination must be different files."
            )
        }

        let byteCount = Int(sourceStatus.st_size)
        let mapped = mmap(nil, byteCount, PROT_READ, MAP_PRIVATE, sourceDescriptor, 0)
        guard mapped != MAP_FAILED else {
            throw MediaWorkerCoreError.invalidImage("The source image could not be mapped.")
        }
        defer { _ = munmap(mapped, byteCount) }
        guard let imageData = CFDataCreateWithBytesNoCopy(
            kCFAllocatorDefault,
            mapped?.assumingMemoryBound(to: UInt8.self),
            byteCount,
            kCFAllocatorNull
        ), let source = CGImageSourceCreateWithData(imageData, [
            kCGImageSourceShouldCache: false,
        ] as CFDictionary),
           CGImageSourceGetCount(source) > 0,
           let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
           let widthValue = properties[kCGImagePropertyPixelWidth] as? NSNumber,
           let heightValue = properties[kCGImagePropertyPixelHeight] as? NSNumber else {
            throw MediaWorkerCoreError.invalidImage("The source is not a supported image.")
        }

        let width = widthValue.doubleValue
        let height = heightValue.doubleValue
        guard width.isFinite,
              height.isFinite,
              width > 0,
              height > 0,
              width * height <= Double(request.maximumSourcePixelCount),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailFromImageAlways: true,
                  kCGImageSourceCreateThumbnailWithTransform: true,
                  kCGImageSourceThumbnailMaxPixelSize: request.maximumOutputPixelSize,
                  kCGImageSourceShouldCacheImmediately: true,
              ] as CFDictionary) else {
            throw MediaWorkerCoreError.invalidImage(
                "The source image is damaged or exceeds its pixel limit."
            )
        }
        try checkDeadline(request.deadlineUnixMilliseconds)

        let consumerState = BoundedImageConsumer(
            descriptor: destinationDescriptor,
            maximumBytes: request.maximumOutputBytes
        )
        var callbacks = CGDataConsumerCallbacks(
            putBytes: { info, buffer, count in
                guard let info else { return 0 }
                return Unmanaged<BoundedImageConsumer>
                    .fromOpaque(info)
                    .takeUnretainedValue()
                    .write(buffer, count: count)
            },
            releaseConsumer: nil
        )
        guard let consumer = CGDataConsumer(
            info: Unmanaged.passUnretained(consumerState).toOpaque(),
            cbks: &callbacks
        ), let destination = CGImageDestinationCreateWithDataConsumer(
            consumer,
            "public.png" as CFString,
            1,
            nil
        ) else {
            throw MediaWorkerCoreError.invalidImage("The PNG encoder could not be created.")
        }
        CGImageDestinationAddImage(destination, image, nil)
        let finalized = CGImageDestinationFinalize(destination)
        try withExtendedLifetime(consumerState) {
            if let error = consumerState.error { throw error }
        }
        guard finalized, consumerState.bytesWritten > 0 else {
            throw MediaWorkerCoreError.invalidImage("The PNG encoder rejected the image.")
        }
        guard fsync(destinationDescriptor) == 0 else {
            if errno == ENOSPC { throw MediaWorkerCoreError.insufficientDiskSpace }
            throw POSIXFailure.current("The normalized image could not be synchronized.")
        }
        try checkDeadline(request.deadlineUnixMilliseconds)

        var finalStatus = stat()
        guard fstat(destinationDescriptor, &finalStatus) == 0,
              isOwnedRegularFile(finalStatus, requireSingleLink: true),
              finalStatus.st_dev == request.destinationIdentity.device,
              finalStatus.st_ino == request.destinationIdentity.inode,
              finalStatus.st_size == consumerState.bytesWritten,
              finalStatus.st_size <= request.maximumOutputBytes else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "The destination changed while the image was being encoded."
            )
        }

        return .init(
            path: request.destinationPath,
            mimeType: "image/png",
            byteCount: Int(finalStatus.st_size),
            sha256: try sha256(descriptor: destinationDescriptor),
            pixelWidth: image.width,
            pixelHeight: image.height
        )
    }

    public static func extractLastVideoFramePNG(
        _ request: MediaWorkerExtractVideoFrameRequest
    ) async throws -> MediaWorkerImageDescriptor {
        try validateRequest(request)
        try checkDeadline(request.deadlineUnixMilliseconds)

        let sourceDescriptor = try openAuthorizedSource(request)
        defer { _ = Darwin.close(sourceDescriptor) }
        let destinationDescriptor = try openAuthorizedDestination(request)
        defer { _ = Darwin.close(destinationDescriptor) }

        var sourceStatus = stat()
        var destinationStatus = stat()
        guard fstat(sourceDescriptor, &sourceStatus) == 0,
              fstat(destinationDescriptor, &destinationStatus) == 0,
              sourceStatus.st_dev != destinationStatus.st_dev
                || sourceStatus.st_ino != destinationStatus.st_ino else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "Source and destination must be different files."
            )
        }

        guard pathMatchesIdentity(
            request.sourcePath,
            identity: request.sourceIdentity
        ) else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "The source video path changed before frame extraction."
            )
        }
        // AVFoundation does not reliably recognize container formats through /dev/fd.
        // Keep the authorized descriptor open and fence the App-owned path before and after
        // extraction so a path replacement can never be accepted as the requested source.
        let asset = AVURLAsset(url: URL(fileURLWithPath: request.sourcePath))
        let duration = try await asset.load(.duration)
        try checkDeadline(request.deadlineUnixMilliseconds)
        guard duration.isNumeric, duration > .zero else {
            throw MediaWorkerCoreError.invalidImage("The source video has no usable duration.")
        }
        let oneTick = CMTime(value: 1, timescale: 600)
        let requestedTime = duration > oneTick ? CMTimeSubtract(duration, oneTick) : .zero
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(
            width: request.maximumOutputPixelSize,
            height: request.maximumOutputPixelSize
        )
        generator.requestedTimeToleranceBefore = CMTime(value: 1, timescale: 30)
        generator.requestedTimeToleranceAfter = .zero
        let result = try await generator.image(at: requestedTime)
        try checkDeadline(request.deadlineUnixMilliseconds)
        guard result.image.width > 0,
              result.image.height > 0,
              result.image.width <= request.maximumOutputPixelSize,
              result.image.height <= request.maximumOutputPixelSize else {
            throw MediaWorkerCoreError.invalidImage(
                "The extracted video frame exceeds its pixel limit."
            )
        }
        var finalSourceStatus = stat()
        guard fstat(sourceDescriptor, &finalSourceStatus) == 0,
              finalSourceStatus.st_dev == sourceStatus.st_dev,
              finalSourceStatus.st_ino == sourceStatus.st_ino,
              finalSourceStatus.st_size == sourceStatus.st_size,
              pathMatchesIdentity(
                  request.sourcePath,
                  identity: request.sourceIdentity
              ) else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "The source video changed while its frame was extracted."
            )
        }
        return try writePNG(
            result.image,
            descriptor: destinationDescriptor,
            destinationPath: request.destinationPath,
            destinationIdentity: request.destinationIdentity,
            maximumBytes: request.maximumOutputBytes,
            deadlineUnixMilliseconds: request.deadlineUnixMilliseconds
        )
    }

    private static func validateRequest(_ request: MediaWorkerNormalizePNGRequest) throws {
        guard request.type == "request",
              request.protocolVersion == ChatOSMediaWorkerProtocol.maximumVersion,
              request.operation == ChatOSMediaWorkerProtocol.normalizePNGOperation,
              !request.requestID.isEmpty,
              !request.ownerUserID.isEmpty,
              !request.accountSessionID.isEmpty,
              !request.sessionEpoch.isEmpty,
              !request.helperInstanceID.isEmpty,
              request.maximumSourceBytes > 0,
              request.maximumSourceBytes
                <= ChatOSMediaWorkerProtocol.maximumNormalizedImageBytes,
              request.maximumSourcePixelCount > 0,
              request.maximumSourcePixelCount
                <= ChatOSMediaWorkerProtocol.maximumNormalizedImagePixels,
              request.maximumOutputPixelSize > 0,
              request.maximumOutputPixelSize <= 8_192,
              request.maximumOutputBytes > 0,
              request.maximumOutputBytes
                <= ChatOSMediaWorkerProtocol.maximumNormalizedImageBytes else {
            throw MediaWorkerCoreError.invalidRequest("The normalize request is outside v1 limits.")
        }
    }

    private static func validateRequest(
        _ request: MediaWorkerExtractVideoFrameRequest
    ) throws {
        guard request.type == "request",
              request.protocolVersion == ChatOSMediaWorkerProtocol.maximumVersion,
              request.operation == ChatOSMediaWorkerProtocol.extractLastVideoFramePNGOperation,
              !request.requestID.isEmpty,
              !request.ownerUserID.isEmpty,
              !request.accountSessionID.isEmpty,
              !request.sessionEpoch.isEmpty,
              !request.helperInstanceID.isEmpty,
              request.maximumSourceBytes > 0,
              request.maximumSourceBytes <= 512 * 1_024 * 1_024,
              request.maximumOutputPixelSize > 0,
              request.maximumOutputPixelSize <= 4_096,
              request.maximumOutputBytes > 0,
              request.maximumOutputBytes <= 20 * 1_024 * 1_024 else {
            throw MediaWorkerCoreError.invalidRequest(
                "The video frame request is outside v1 limits."
            )
        }
    }

    private static func openAuthorizedSource(
        _ request: MediaWorkerNormalizePNGRequest
    ) throws -> Int32 {
        let descriptor = request.sourcePath.withCString {
            Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else {
            throw MediaWorkerCoreError.unauthorizedPath("The source file could not be opened safely.")
        }
        do {
            var status = stat()
            guard fstat(descriptor, &status) == 0,
                  isOwnedRegularFile(status, requireSingleLink: false),
                  status.st_dev == request.sourceIdentity.device,
                  status.st_ino == request.sourceIdentity.inode,
                  status.st_size == request.sourceIdentity.byteCount,
                  status.st_size > 0,
                  status.st_size <= request.maximumSourceBytes else {
                throw MediaWorkerCoreError.unauthorizedPath(
                    "The source file identity or size does not match its authorization."
                )
            }
            return descriptor
        } catch {
            _ = Darwin.close(descriptor)
            throw error
        }
    }

    private static func openAuthorizedSource(
        _ request: MediaWorkerExtractVideoFrameRequest
    ) throws -> Int32 {
        let descriptor = request.sourcePath.withCString {
            Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "The source video could not be opened safely."
            )
        }
        do {
            var status = stat()
            guard fstat(descriptor, &status) == 0,
                  isOwnedRegularFile(status, requireSingleLink: false),
                  status.st_dev == request.sourceIdentity.device,
                  status.st_ino == request.sourceIdentity.inode,
                  status.st_size == request.sourceIdentity.byteCount,
                  status.st_size > 0,
                  status.st_size <= request.maximumSourceBytes else {
                throw MediaWorkerCoreError.unauthorizedPath(
                    "The source video identity or size does not match its authorization."
                )
            }
            return descriptor
        } catch {
            _ = Darwin.close(descriptor)
            throw error
        }
    }

    private static func openAuthorizedDestination(
        _ request: MediaWorkerNormalizePNGRequest
    ) throws -> Int32 {
        guard request.destinationIdentity.byteCount == 0 else {
            throw MediaWorkerCoreError.unauthorizedPath("The destination was not empty.")
        }
        let descriptor = request.destinationPath.withCString {
            Darwin.open($0, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "The pre-created destination could not be opened safely."
            )
        }
        do {
            var status = stat()
            guard fstat(descriptor, &status) == 0,
                  isOwnedRegularFile(status, requireSingleLink: true),
                  status.st_dev == request.destinationIdentity.device,
                  status.st_ino == request.destinationIdentity.inode,
                  status.st_size == 0 else {
                throw MediaWorkerCoreError.unauthorizedPath(
                    "The destination identity does not match its authorization."
                )
            }
            return descriptor
        } catch {
            _ = Darwin.close(descriptor)
            throw error
        }
    }

    private static func openAuthorizedDestination(
        _ request: MediaWorkerExtractVideoFrameRequest
    ) throws -> Int32 {
        guard request.destinationIdentity.byteCount == 0 else {
            throw MediaWorkerCoreError.unauthorizedPath("The destination was not empty.")
        }
        let descriptor = request.destinationPath.withCString {
            Darwin.open($0, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "The pre-created destination could not be opened safely."
            )
        }
        do {
            var status = stat()
            guard fstat(descriptor, &status) == 0,
                  isOwnedRegularFile(status, requireSingleLink: true),
                  status.st_dev == request.destinationIdentity.device,
                  status.st_ino == request.destinationIdentity.inode,
                  status.st_size == 0 else {
                throw MediaWorkerCoreError.unauthorizedPath(
                    "The destination identity does not match its authorization."
                )
            }
            return descriptor
        } catch {
            _ = Darwin.close(descriptor)
            throw error
        }
    }

    private static func writePNG(
        _ image: CGImage,
        descriptor: Int32,
        destinationPath: String,
        destinationIdentity: MediaWorkerFileIdentity,
        maximumBytes: Int,
        deadlineUnixMilliseconds: Int64
    ) throws -> MediaWorkerImageDescriptor {
        let consumerState = BoundedImageConsumer(
            descriptor: descriptor,
            maximumBytes: maximumBytes
        )
        var callbacks = CGDataConsumerCallbacks(
            putBytes: { info, buffer, count in
                guard let info else { return 0 }
                return Unmanaged<BoundedImageConsumer>
                    .fromOpaque(info)
                    .takeUnretainedValue()
                    .write(buffer, count: count)
            },
            releaseConsumer: nil
        )
        guard let consumer = CGDataConsumer(
            info: Unmanaged.passUnretained(consumerState).toOpaque(),
            cbks: &callbacks
        ), let destination = CGImageDestinationCreateWithDataConsumer(
            consumer,
            "public.png" as CFString,
            1,
            nil
        ) else {
            throw MediaWorkerCoreError.invalidImage("The PNG encoder could not be created.")
        }
        CGImageDestinationAddImage(destination, image, nil)
        let finalized = CGImageDestinationFinalize(destination)
        try withExtendedLifetime(consumerState) {
            if let error = consumerState.error { throw error }
        }
        guard finalized, consumerState.bytesWritten > 0 else {
            throw MediaWorkerCoreError.invalidImage("The PNG encoder rejected the image.")
        }
        guard fsync(descriptor) == 0 else {
            if errno == ENOSPC { throw MediaWorkerCoreError.insufficientDiskSpace }
            throw POSIXFailure.current("The extracted frame could not be synchronized.")
        }
        try checkDeadline(deadlineUnixMilliseconds)

        var finalStatus = stat()
        guard fstat(descriptor, &finalStatus) == 0,
              isOwnedRegularFile(finalStatus, requireSingleLink: true),
              finalStatus.st_dev == destinationIdentity.device,
              finalStatus.st_ino == destinationIdentity.inode,
              finalStatus.st_size == consumerState.bytesWritten,
              finalStatus.st_size <= maximumBytes else {
            throw MediaWorkerCoreError.unauthorizedPath(
                "The destination changed while the frame was being encoded."
            )
        }
        return .init(
            path: destinationPath,
            mimeType: "image/png",
            byteCount: Int(finalStatus.st_size),
            sha256: try sha256(descriptor: descriptor),
            pixelWidth: image.width,
            pixelHeight: image.height
        )
    }

    private static func isOwnedRegularFile(
        _ status: stat,
        requireSingleLink: Bool
    ) -> Bool {
        (status.st_mode & S_IFMT) == S_IFREG
            && status.st_uid == geteuid()
            && (!requireSingleLink || status.st_nlink == 1)
    }

    private static func pathMatchesIdentity(
        _ path: String,
        identity: MediaWorkerFileIdentity
    ) -> Bool {
        var status = stat()
        return path.withCString { Darwin.lstat($0, &status) } == 0
            && isOwnedRegularFile(status, requireSingleLink: false)
            && UInt64(status.st_dev) == identity.device
            && UInt64(status.st_ino) == identity.inode
            && Int64(status.st_size) == identity.byteCount
    }

    private static func checkDeadline(_ deadlineUnixMilliseconds: Int64) throws {
        let now = Int64(Date().timeIntervalSince1970 * 1_000)
        guard deadlineUnixMilliseconds > now else {
            throw MediaWorkerCoreError.deadlineExceeded
        }
    }

    private static func sha256(descriptor: Int32) throws -> String {
        guard lseek(descriptor, 0, SEEK_SET) == 0 else {
            throw POSIXFailure.current("The normalized image could not be read for hashing.")
        }
        var hasher = SHA256()
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        while true {
            let count = Darwin.read(descriptor, &buffer, buffer.count)
            if count == 0 { break }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else {
                throw POSIXFailure.current("The normalized image hash could not be computed.")
            }
            hasher.update(data: Data(buffer[0..<count]))
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

private final class BoundedImageConsumer {
    let descriptor: Int32
    let maximumBytes: Int
    private(set) var error: Error?
    private(set) var bytesWritten: Int64 = 0

    init(descriptor: Int32, maximumBytes: Int) {
        self.descriptor = descriptor
        self.maximumBytes = maximumBytes
    }

    func write(_ buffer: UnsafeRawPointer, count: Int) -> Int {
        guard error == nil, count > 0 else { return 0 }
        guard Int64(count) <= Int64(maximumBytes) - bytesWritten else {
            error = MediaWorkerCoreError.outputTooLarge
            return 0
        }
        var written = 0
        while written < count {
            let result = Darwin.write(
                descriptor,
                buffer.advanced(by: written),
                count - written
            )
            if result > 0 {
                written += result
                bytesWritten += Int64(result)
            } else if result < 0, errno == EINTR {
                continue
            } else {
                error = errno == ENOSPC
                    ? MediaWorkerCoreError.insufficientDiskSpace
                    : POSIXFailure.current("The normalized image could not be written.")
                return written
            }
        }
        return written
    }
}

private struct POSIXFailure: LocalizedError {
    let message: String
    let code: Int32

    static func current(_ message: String) -> POSIXFailure {
        .init(message: message, code: errno)
    }

    var errorDescription: String? { "\(message) (errno \(code))" }
}
