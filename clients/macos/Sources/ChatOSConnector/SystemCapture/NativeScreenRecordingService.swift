import AVFoundation
import CoreGraphics
import CoreMedia
import CoreVideo
import Foundation
import ScreenCaptureKit

public actor NativeScreenRecordingService {
    private let processingQueue = DispatchQueue(
        label: "com.chatos.screen-recording.writer",
        qos: .userInitiated
    )
    private var stream: SCStream?
    private var streamDelegate: ScreenRecordingStreamDelegate?
    private var writer: ScreenRecordingWriter?

    public init() {}

    public func availableTargets() async throws -> [NativeScreenRecordingTarget] {
        guard NativeSystemPermissionService.hasScreenCaptureAccess else {
            throw NativeScreenRecordingError.permissionDenied
        }
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: false
        )
        var targets: [NativeScreenRecordingTarget] = content.displays.enumerated().map { index, display in
            NativeScreenRecordingTarget(
                id: "display:\(display.displayID)",
                kind: .display,
                nativeID: display.displayID,
                title: "Display \(index + 1)",
                subtitle: "\(display.width) × \(display.height)",
                width: display.width,
                height: display.height
            )
        }
        let ownBundleID = Bundle.main.bundleIdentifier
        var bestWindowByApplication: [String: (target: NativeScreenRecordingTarget, score: Double)] = [:]
        for window in content.windows {
            let applicationName = window.owningApplication?.applicationName
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let title = window.title?.trimmingCharacters(in: .whitespacesAndNewlines)
            let isOwnWindow = window.owningApplication?.bundleIdentifier == ownBundleID
            guard window.windowLayer == 0,
                  window.frame.width >= 240,
                  window.frame.height >= 140,
                  applicationName?.isEmpty == false,
                  title?.isEmpty == false,
                  !isOwnWindow || title != "Screen Recording" else {
                continue
            }
            let target = NativeScreenRecordingTarget(
                id: "window:\(window.windowID)",
                kind: .window,
                nativeID: window.windowID,
                title: applicationName ?? "Application",
                subtitle: title,
                width: max(1, Int(window.frame.width.rounded())),
                height: max(1, Int(window.frame.height.rounded()))
            )
            let identity = window.owningApplication?.bundleIdentifier ?? applicationName ?? target.id
            let area = Double(window.frame.width * window.frame.height)
            let score = (window.isOnScreen ? 1_000_000_000_000 : 0) + area
            if score > bestWindowByApplication[identity]?.score ?? -.infinity {
                bestWindowByApplication[identity] = (target, score)
            }
        }
        targets.append(contentsOf: bestWindowByApplication.values
            .map(\.target)
            .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending })
        return targets
    }

    public func start(
        target: NativeScreenRecordingTarget,
        outputURL: URL,
        capturesSystemAudio: Bool
    ) async throws {
        guard stream == nil else { throw NativeScreenRecordingError.alreadyRecording }
        guard NativeSystemPermissionService.hasScreenCaptureAccess else {
            throw NativeScreenRecordingError.permissionDenied
        }
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: false
        )

        let filter: SCContentFilter
        let width: Int
        let height: Int
        switch target.kind {
        case .display:
            guard let display = content.displays.first(where: { $0.displayID == target.nativeID }) else {
                throw NativeScreenRecordingError.targetUnavailable
            }
            // Record the display exactly as it is composited, including ChatOS
            // floating windows such as the pet, dialogs, and recording controls.
            filter = SCContentFilter(display: display, excludingWindows: [])
            width = max(2, display.width - display.width % 2)
            height = max(2, display.height - display.height % 2)
        case .window:
            guard let window = content.windows.first(where: { $0.windowID == target.nativeID }) else {
                throw NativeScreenRecordingError.targetUnavailable
            }
            filter = SCContentFilter(desktopIndependentWindow: window)
            let pixelScale = CGFloat(filter.pointPixelScale)
            width = Self.evenPixelDimension(filter.contentRect.width * pixelScale)
            height = Self.evenPixelDimension(filter.contentRect.height * pixelScale)
        }

        try FileManager.default.createDirectory(
            at: outputURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try? FileManager.default.removeItem(at: outputURL)
        let writer = try ScreenRecordingWriter(
            outputURL: outputURL,
            width: width,
            height: height,
            capturesAudio: capturesSystemAudio,
            processingQueue: processingQueue
        )
        let configuration = SCStreamConfiguration()
        configuration.width = width
        configuration.height = height
        configuration.pixelFormat = kCVPixelFormatType_32BGRA
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: 30)
        configuration.queueDepth = 6
        configuration.showsCursor = true
        configuration.capturesAudio = capturesSystemAudio
        configuration.sampleRate = 48_000
        configuration.channelCount = 2
        configuration.excludesCurrentProcessAudio = false

        let delegate = ScreenRecordingStreamDelegate()
        let stream = SCStream(filter: filter, configuration: configuration, delegate: delegate)
        try stream.addStreamOutput(writer, type: .screen, sampleHandlerQueue: processingQueue)
        if capturesSystemAudio {
            try stream.addStreamOutput(writer, type: .audio, sampleHandlerQueue: processingQueue)
        }
        self.writer = writer
        self.streamDelegate = delegate
        self.stream = stream
        do {
            try await stream.startCapture()
        } catch {
            self.stream = nil
            self.streamDelegate = nil
            self.writer = nil
            throw error
        }
    }

    public func stop() async throws -> URL {
        guard let stream, let writer else { throw NativeScreenRecordingError.notRecording }
        self.stream = nil
        self.streamDelegate = nil
        self.writer = nil
        do {
            try await stream.stopCapture()
        } catch {
            // Finalize frames already delivered even when ScreenCaptureKit reports a stop error.
        }
        return try await writer.finish()
    }

    public var isRecording: Bool {
        stream != nil
    }

    private static func evenPixelDimension(_ value: CGFloat) -> Int {
        max(2, Int(value.rounded()) / 2 * 2)
    }
}

private final class ScreenRecordingStreamDelegate: NSObject, SCStreamDelegate, @unchecked Sendable {
    func stream(_ stream: SCStream, didStopWithError error: any Error) {}
}
