import CoreGraphics
import Foundation
import ScreenCaptureKit

public actor NativeScreenCaptureService {
    public init() {}

    public func capture(region: NativeScreenCaptureRegion) async throws -> CGImage {
        let captures = try await capture(regions: [region])
        guard let image = captures[region.displayID] else {
            throw NativeScreenCaptureError.displayUnavailable
        }
        return image
    }

    public func capture(
        regions: [NativeScreenCaptureRegion]
    ) async throws -> [CGDirectDisplayID: CGImage] {
        guard NativeSystemPermissionService.hasScreenCaptureAccess else {
            throw NativeScreenCaptureError.permissionDenied
        }
        guard !regions.isEmpty,
              regions.allSatisfy({
                  $0.sourceRect.width > 0
                      && $0.sourceRect.height > 0
                      && $0.outputSize.width > 0
                      && $0.outputSize.height > 0
              }) else {
            throw NativeScreenCaptureError.invalidRegion
        }

        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )
        var captures: [CGDirectDisplayID: CGImage] = [:]

        for region in regions {
            guard let display = content.displays.first(where: {
                $0.displayID == region.displayID
            }) else {
                throw NativeScreenCaptureError.displayUnavailable
            }

            // Capture the display's real composited contents. In particular, do not
            // filter windows by owning application, level, or type: floating ChatOS
            // UI such as the desktop pet and its conversation must remain visible.
            let filter = SCContentFilter(display: display, excludingWindows: [])
            let configuration = SCStreamConfiguration()
            configuration.sourceRect = region.sourceRect
            configuration.width = max(1, Int(region.outputSize.width.rounded()))
            configuration.height = max(1, Int(region.outputSize.height.rounded()))
            configuration.showsCursor = false
            configuration.capturesAudio = false
            configuration.ignoreShadowsSingleWindow = true

            captures[region.displayID] = try await SCScreenshotManager.captureImage(
                contentFilter: filter,
                configuration: configuration
            )
        }

        return captures
    }
}
