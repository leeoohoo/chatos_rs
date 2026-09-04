import CoreGraphics
import Foundation

public struct NativeScreenCaptureRegion: Sendable {
    public let displayID: CGDirectDisplayID
    public let sourceRect: CGRect
    public let outputSize: CGSize

    public init(
        displayID: CGDirectDisplayID,
        sourceRect: CGRect,
        outputSize: CGSize
    ) {
        self.displayID = displayID
        self.sourceRect = sourceRect
        self.outputSize = outputSize
    }
}

public enum NativeScreenCaptureError: LocalizedError {
    case permissionDenied
    case displayUnavailable
    case invalidRegion

    public var errorDescription: String? {
        switch self {
        case .permissionDenied:
            "Screen recording permission has not been granted."
        case .displayUnavailable:
            "The selected display is no longer available."
        case .invalidRegion:
            "The selected screenshot region is invalid."
        }
    }
}
