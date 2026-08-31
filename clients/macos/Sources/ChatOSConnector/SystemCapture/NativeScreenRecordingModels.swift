import CoreGraphics
import Foundation

public enum NativeScreenRecordingTargetKind: String, Sendable, Hashable {
    case display
    case window
}

public struct NativeScreenRecordingTarget: Identifiable, Sendable, Hashable {
    public let id: String
    public let kind: NativeScreenRecordingTargetKind
    public let nativeID: UInt32
    public let title: String
    public let subtitle: String?
    public let width: Int
    public let height: Int

    public init(
        id: String,
        kind: NativeScreenRecordingTargetKind,
        nativeID: UInt32,
        title: String,
        subtitle: String?,
        width: Int,
        height: Int
    ) {
        self.id = id
        self.kind = kind
        self.nativeID = nativeID
        self.title = title
        self.subtitle = subtitle
        self.width = width
        self.height = height
    }
}

public enum NativeScreenRecordingError: LocalizedError {
    case permissionDenied
    case targetUnavailable
    case alreadyRecording
    case notRecording
    case writer(String)

    public var errorDescription: String? {
        switch self {
        case .permissionDenied: "Screen recording permission has not been granted."
        case .targetUnavailable: "The selected recording target is no longer available."
        case .alreadyRecording: "A screen recording is already active."
        case .notRecording: "No screen recording is active."
        case let .writer(message): message
        }
    }
}
