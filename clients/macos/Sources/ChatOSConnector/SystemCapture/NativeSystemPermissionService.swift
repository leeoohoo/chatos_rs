import AppKit
import CoreGraphics
import Foundation

public enum NativeSystemPermissionService {
    public static var hasScreenCaptureAccess: Bool {
        CGPreflightScreenCaptureAccess()
    }

    @discardableResult
    public static func requestScreenCaptureAccess() -> Bool {
        CGRequestScreenCaptureAccess()
    }

    @MainActor
    public static func openScreenCapturePrivacySettings() {
        guard let url = URL(
            string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        ) else { return }
        NSWorkspace.shared.open(url)
    }
}
