import ChatOSCore
import Foundation
import ImageIO

struct VisualSessionFrameIdentity: Hashable, Sendable {
    let adapterSessionID: String
    let frameSequence: UInt64
}

struct PreparedVisualSession: Sendable {
    var session: PluginVisualSession
    let frameImage: VisualSessionFrameImage?
}

enum VisualSessionFrameDecoder {
    private static let maximumDisplayPixelSize = 800

    static func prepare(
        _ sessions: [PluginVisualSession],
        reusing existingFrames: [VisualSessionFrameIdentity: VisualSessionFrameImage]
    ) async -> [PreparedVisualSession] {
        await Task.detached(priority: .userInitiated) {
            sessions.map { incoming in
                var session = incoming
                let identity = VisualSessionFrameIdentity(
                    adapterSessionID: session.adapterSessionID,
                    frameSequence: session.frameSequence
                )
                let frameImage = existingFrames[identity]
                    ?? session.frameData.flatMap(decode)
                session.frameData = nil
                return PreparedVisualSession(session: session, frameImage: frameImage)
            }
        }.value
    }

    private static func decode(_ data: Data) -> VisualSessionFrameImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, [
            kCGImageSourceShouldCache: false,
        ] as CFDictionary),
              CGImageSourceGetCount(source) > 0,
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumDisplayPixelSize,
                kCGImageSourceShouldCacheImmediately: true,
              ] as CFDictionary) else {
            return nil
        }
        return VisualSessionFrameImage(image: image)
    }
}
