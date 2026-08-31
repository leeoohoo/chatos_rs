@preconcurrency import AppKit
@preconcurrency import ApplicationServices
@preconcurrency import CoreGraphics
import Foundation

enum MacPermissionKind: String, CaseIterable, Sendable {
    case accessibility
    case screenRecording

    var title: String {
        switch self {
        case .accessibility: "辅助功能"
        case .screenRecording: "屏幕与系统音频录制"
        }
    }

    var purpose: String {
        switch self {
        case .accessibility:
            "用于发送真实的鼠标、键盘和滚动事件。"
        case .screenRecording:
            "用于获取真实屏幕截图，让 AI 通过视觉定位界面。"
        }
    }

    var systemSettingsTitle: String {
        switch self {
        case .accessibility: "隐私与安全性 > 辅助功能"
        case .screenRecording: "隐私与安全性 > 屏幕与系统音频录制"
        }
    }

    var settingsURL: URL {
        switch self {
        case .accessibility:
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        case .screenRecording:
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")!
        }
    }

    var symbolName: String {
        switch self {
        case .accessibility: "hand.raised.fill"
        case .screenRecording: "rectangle.inset.filled.and.person.filled"
        }
    }

    func isGranted() -> Bool {
        switch self {
        case .accessibility: AXIsProcessTrusted()
        case .screenRecording: CGPreflightScreenCaptureAccess()
        }
    }

    @MainActor
    func requestAndOpenSettings() {
        switch self {
        case .accessibility:
            let options = [
                kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true
            ] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)
        case .screenRecording:
            if !CGPreflightScreenCaptureAccess() {
                _ = CGRequestScreenCaptureAccess()
            }
        }
        NSWorkspace.shared.open(settingsURL)
    }
}

enum PermissionSupport {
    static var applicationName: String {
        let displayName = Bundle.main.object(
            forInfoDictionaryKey: "CFBundleDisplayName"
        ) as? String
        let bundleName = Bundle.main.object(
            forInfoDictionaryKey: kCFBundleNameKey as String
        ) as? String
        return displayName ?? bundleName ?? "Visual Computer Use"
    }

    static var executableURL: URL {
        if let bundledExecutable = Bundle.main.executableURL {
            return bundledExecutable.standardizedFileURL
        }
        return URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL
    }

    static var appBundleURL: URL? {
        let bundleURL = Bundle.main.bundleURL.standardizedFileURL
        guard bundleURL.pathExtension.caseInsensitiveCompare("app") == .orderedSame else {
            return nil
        }
        return bundleURL
    }

    static var authorizationTargetURL: URL {
        appBundleURL ?? executableURL
    }

    static func diagnostics(onboardingPresented: Bool = false) -> PermissionDTO {
        let accessibility = MacPermissionKind.accessibility.isGranted()
        let screenRecording = MacPermissionKind.screenRecording.isGranted()
        let allGranted = accessibility && screenRecording
        let runningFromAppBundle = appBundleURL != nil
        let items = MacPermissionKind.allCases.map { permission in
            let granted = permission.isGranted()
            return PermissionItemDTO(
                kind: permission.rawValue,
                title: permission.title,
                granted: granted,
                purpose: permission.purpose,
                systemSettingsTitle: permission.systemSettingsTitle,
                settingsURL: permission.settingsURL.absoluteString,
                nextStep: granted
                    ? "已授权，无需操作。"
                    : "打开系统设置，添加或启用“\(authorizationTargetURL.lastPathComponent)”。"
            )
        }

        var guidance: [String] = []
        if allGranted {
            guidance.append("权限已完整，可以进行截图和真实输入。")
        } else {
            guidance.append("调用 request_permissions 显示原生 macOS 权限引导窗口。")
            guidance.append("在系统设置中添加或启用：\(authorizationTargetURL.path)")
            guidance.append("授权屏幕录制后，macOS 可能要求重新连接或重启 MCP。")
        }
        if !runningFromAppBundle {
            guidance.append(
                "当前从裸二进制运行。建议使用 dist/Visual Computer Use.app/Contents/MacOS/visual-computer-use-mcp，以获得更稳定的 macOS 权限身份。"
            )
        }

        return PermissionDTO(
            screenRecording: screenRecording,
            accessibility: accessibility,
            allGranted: allGranted,
            missingPermissions: items.filter { !$0.granted }.map(\.kind),
            applicationName: applicationName,
            bundleIdentifier: Bundle.main.bundleIdentifier,
            executable: executableURL.path,
            authorizationTarget: authorizationTargetURL.path,
            runningFromAppBundle: runningFromAppBundle,
            onboardingPresented: onboardingPresented,
            restartMayBeRequired: !screenRecording,
            permissions: items,
            guidance: guidance
        )
    }

    @MainActor
    static func revealAuthorizationTarget() {
        NSWorkspace.shared.activateFileViewerSelecting([authorizationTargetURL])
    }
}
