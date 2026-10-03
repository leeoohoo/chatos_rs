import ChatOSCore
import Foundation

struct VisualSessionFrameIdentity: Hashable, Sendable {
    let adapterSessionID: String
    let frameSequence: UInt64
}

struct PreparedVisualSession: Sendable {
    var session: PluginVisualSession
    let frameImage: VisualSessionFrameImage?
}

enum VisualSessionFrameDecoder {
    static let maximumFrameBytes = 2 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000
    static let maximumDisplayPixelSize = 800

    static func prepare(
        _ sessions: [PluginVisualSession],
        reusing existingFrames: [VisualSessionFrameIdentity: VisualSessionFrameImage]
    ) async -> [PreparedVisualSession] {
        let task = Task.detached(priority: .userInitiated) {
            var prepared: [PreparedVisualSession] = []
            prepared.reserveCapacity(sessions.count)
            for incoming in sessions {
                guard !Task.isCancelled else { break }
                var session = incoming
                let identity = VisualSessionFrameIdentity(
                    adapterSessionID: session.adapterSessionID,
                    frameSequence: session.frameSequence
                )
                let frameImage = existingFrames[identity]
                    ?? session.frameData.flatMap(decode)
                session.frameData = nil
                prepared.append(PreparedVisualSession(session: session, frameImage: frameImage))
            }
            return prepared
        }
        return await withTaskCancellationHandler {
            await task.value
        } onCancel: {
            task.cancel()
        }
    }

    private static func decode(_ data: Data) -> VisualSessionFrameImage? {
        guard !Task.isCancelled,
              !data.isEmpty,
              data.count <= maximumFrameBytes,
              let decoded = AppImageThumbnailLoader.decode(
                data,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDisplayPixelSize: maximumDisplayPixelSize
              ),
              !Task.isCancelled else {
            return nil
        }
        return VisualSessionFrameImage(image: decoded.image)
    }
}
