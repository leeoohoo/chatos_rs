import AppKit
import AVFoundation
import ChatOSCore
import ChatOSNetworking
import Foundation

enum StoryVideoFrameExtractor {
    private static let maximumFramePixelSize = 4_096

    static func lastFramePNG(
        from videoURL: URL,
        taskID: String? = nil,
        ownerSessionID: String? = nil,
        heavyMediaGovernor: ClientHeavyMediaWorkGovernor = .shared
    ) async throws -> Data {
        let taskComponent = taskID.map { String($0.prefix(128)) }
            ?? UUID().uuidString.lowercased()
        return try await heavyMediaGovernor.perform(context: .init(
            taskID: "video:\(taskComponent):last-frame",
            ownerSessionID: ownerSessionID,
            kind: .videoFrameExtraction,
            estimatedDecodedBytes: maximumFramePixelSize * maximumFramePixelSize * 4
        )) {
            try await AppCancellableDetachedWork.runAsync(priority: .utility) {
                try Task.checkCancellation()
                let asset = AVURLAsset(url: videoURL)
                let duration = try await asset.load(.duration)
                try Task.checkCancellation()
                guard duration.isNumeric, duration > .zero else { throw StoryError.unsafeFile }

                let oneTick = CMTime(value: 1, timescale: 600)
                let requestedTime = duration > oneTick ? CMTimeSubtract(duration, oneTick) : .zero
                let generator = AVAssetImageGenerator(asset: asset)
                generator.appliesPreferredTrackTransform = true
                generator.maximumSize = CGSize(
                    width: maximumFramePixelSize,
                    height: maximumFramePixelSize
                )
                generator.requestedTimeToleranceBefore = CMTime(value: 1, timescale: 30)
                generator.requestedTimeToleranceAfter = .zero
                let result = try await generator.image(at: requestedTime)
                try Task.checkCancellation()
                guard result.image.width > 0,
                      result.image.height > 0,
                      result.image.width <= maximumFramePixelSize,
                      result.image.height <= maximumFramePixelSize else {
                    throw StoryError.unsafeFile
                }
                guard let png = NSBitmapImageRep(cgImage: result.image)
                    .representation(using: .png, properties: [:]), !png.isEmpty else {
                    throw StoryError.unsafeFile
                }
                try Task.checkCancellation()
                return png
            }
        }
    }
}
