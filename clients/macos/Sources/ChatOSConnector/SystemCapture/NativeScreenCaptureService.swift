import CoreGraphics
import Foundation
import ScreenCaptureKit

public actor NativeScreenCaptureService {
    public init() {}

    public func capture(region: NativeScreenCaptureRegion) async throws -> CGImage {
        guard NativeSystemPermissionService.hasScreenCaptureAccess else {
            throw NativeScreenCaptureError.permissionDenied
        }
        guard region.sourceRect.width > 0,
              region.sourceRect.height > 0,
              region.outputSize.width > 0,
              region.outputSize.height > 0 else {
            throw NativeScreenCaptureError.invalidRegion
        }

        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )
        guard let display = content.displays.first(where: { $0.displayID == region.displayID }) else {
            throw NativeScreenCaptureError.displayUnavailable
        }

        let ownApplications = content.applications.filter {
            $0.bundleIdentifier == Bundle.main.bundleIdentifier
        }
        let filter = SCContentFilter(
            display: display,
            excludingApplications: ownApplications,
            exceptingWindows: []
        )
        let configuration = SCStreamConfiguration()
        configuration.sourceRect = region.sourceRect
        configuration.width = max(1, Int(region.outputSize.width.rounded()))
        configuration.height = max(1, Int(region.outputSize.height.rounded()))
        configuration.showsCursor = false
        configuration.capturesAudio = false
        configuration.ignoreShadowsSingleWindow = true

        return try await SCScreenshotManager.captureImage(
            contentFilter: filter,
            configuration: configuration
        )
    }
}
