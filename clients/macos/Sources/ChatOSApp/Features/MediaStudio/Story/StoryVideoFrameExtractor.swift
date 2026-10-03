import AppKit
import AVFoundation
import ChatOSCore
import Foundation

enum StoryVideoFrameExtractor {
    static func lastFramePNG(from videoURL: URL) async throws -> Data {
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
            generator.requestedTimeToleranceBefore = CMTime(value: 1, timescale: 30)
            generator.requestedTimeToleranceAfter = .zero
            let result = try await generator.image(at: requestedTime)
            try Task.checkCancellation()
            guard let png = NSBitmapImageRep(cgImage: result.image)
                .representation(using: .png, properties: [:]), !png.isEmpty else {
                throw StoryError.unsafeFile
            }
            try Task.checkCancellation()
            return png
        }
    }
}
