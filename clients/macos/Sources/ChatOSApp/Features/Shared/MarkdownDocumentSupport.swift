import AppKit
import Foundation
import ImageIO

enum MarkdownLayoutPolicy {
    static let maximumInlineHeight: CGFloat = 520
    static let streamingUpdateDebounce: Duration = .milliseconds(40)
    static let boundedViewportByteThreshold = 1_500
    static let boundedViewportLineThreshold = 32

    static func shouldDebounceStreamingUpdate(
        previousSource: String,
        nextSource: String
    ) -> Bool {
        !previousSource.isEmpty
            && nextSource.count > previousSource.count
            && nextSource.hasPrefix(previousSource)
    }

    static func shouldUseBoundedViewport(_ source: String) -> Bool {
        source.utf8.count >= boundedViewportByteThreshold
            || source.lazy.filter(\.isNewline).prefix(boundedViewportLineThreshold).count
                >= boundedViewportLineThreshold
    }
}

enum MarkdownViewport: Equatable {
    case bounded(maximumHeight: CGFloat)
    case reader

    var fillsAvailableHeight: Bool { self == .reader }
}

enum MarkdownRemoteImageLoader {
    static let maximumBytes = 10 * 1_024 * 1_024
    static let maximumPixelCount = 40_000_000
    static let maximumDisplaySize = NSSize(width: 520, height: 420)

    struct DecodedImage: @unchecked Sendable {
        let image: CGImage
        let displaySize: NSSize
        let cost: Int
    }

    static func allowedURL(from rawValue: String) -> URL? {
        guard let url = URL(string: rawValue),
              matchesAllowedScheme(url.scheme),
              isChatOSAttachmentPath(url.path),
              URLComponents(url: url, resolvingAgainstBaseURL: false)?
                .queryItems?.contains(where: { $0.name == "token" && !($0.value ?? "").isEmpty }) == true
        else { return nil }
        return url
    }

    static func load(_ url: URL) async -> Data? {
        var request = URLRequest(url: url)
        request.timeoutInterval = 20
        guard let (data, response) = try? await URLSession.shared.data(for: request),
              !data.isEmpty,
              data.count <= maximumBytes,
              let response = response as? HTTPURLResponse,
              (200..<300).contains(response.statusCode),
              response.mimeType?.hasPrefix("image/") == true else { return nil }
        return data
    }

    nonisolated static func decode(_ data: Data) -> DecodedImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
                as? [CFString: Any],
              let pixelWidth = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let pixelHeight = properties[kCGImagePropertyPixelHeight] as? NSNumber else {
            return nil
        }
        let width = pixelWidth.doubleValue
        let height = pixelHeight.doubleValue
        guard width.isFinite, height.isFinite,
              width > 0, height > 0,
              width * height <= Double(maximumPixelCount) else { return nil }

        guard let displaySize = boundedDisplaySize(
            pixelWidth: width,
            pixelHeight: height
        ) else { return nil }
        let thumbnailPixelSize = max(
            1,
            Int(ceil(max(displaySize.width, displaySize.height) * 2))
        )
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: thumbnailPixelSize,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(
            source,
            0,
            options as CFDictionary
        ) else { return nil }
        return DecodedImage(
            image: image,
            displaySize: displaySize,
            cost: image.bytesPerRow * image.height
        )
    }

    nonisolated static func boundedDisplaySize(
        pixelWidth width: Double,
        pixelHeight height: Double
    ) -> NSSize? {
        guard width.isFinite, height.isFinite,
              width > 0, height > 0,
              width * height <= Double(maximumPixelCount) else { return nil }
        let displayScale = min(
            1,
            min(
                Double(maximumDisplaySize.width) / width,
                Double(maximumDisplaySize.height) / height
            )
        )
        return NSSize(
            width: max(1, width * displayScale),
            height: max(1, height * displayScale)
        )
    }

    private static func matchesAllowedScheme(_ scheme: String?) -> Bool {
        scheme?.lowercased() == "https" || scheme?.lowercased() == "http"
    }

    private static func isChatOSAttachmentPath(_ path: String) -> Bool {
        path == "/api/attachments/object"
            || path.hasSuffix("/attachments/object")
    }
}
