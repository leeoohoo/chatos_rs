import AppKit
import ApplicationServices
import ChatOSCore
import CoreGraphics
import Foundation

enum NativeSystemPermissions {
    static func snapshot() -> LocalConnectorSystemPermissions {
        .init(
            platform: "macos",
            platformLabel: "macOS",
            items: [
                permission(
                    id: "terminal",
                    label: "本机终端执行",
                    summary: "由 Swift 客户端直接启动受控的本机进程",
                    ready: true,
                    canRequest: false,
                    requestLabel: "无需授权",
                    note: "命令仍受工作区边界与审批策略约束。"
                ),
                permission(
                    id: "accessibility",
                    label: "辅助功能",
                    summary: "允许 Computer Use 控制本机应用",
                    ready: isGranted("accessibility"),
                    canRequest: true,
                    requestLabel: "授权引导",
                    note: "授权目标是当前 ChatOS App；引导窗口可直接打开设置或拖入应用列表。"
                ),
                permission(
                    id: "screen_recording",
                    label: "屏幕与系统音频录制",
                    summary: "允许 Computer Use 获取屏幕画面",
                    ready: isGranted("screen_recording"),
                    canRequest: true,
                    requestLabel: "授权引导",
                    note: "授权目标是当前 ChatOS App；首次授权后可能需要重启 ChatOS。"
                ),
                permission(
                    id: "full_disk_access",
                    label: "完全磁盘访问",
                    summary: "访问受 macOS 隐私保护的目录",
                    ready: isGranted("full_disk_access"),
                    canRequest: true,
                    requestLabel: "授权引导",
                    note: "需要时可把引导窗口中的 ChatOS App 直接拖入系统设置应用列表。"
                ),
            ]
        )
    }

    @MainActor
    static func request(_ id: String) {
        NativeSystemPermissionOnboarding.present(permissionID: id)
    }

    @MainActor
    static func requestAndOpenSettings(_ id: String) {
        switch id {
        case "accessibility":
            let options = ["AXTrustedCheckOptionPrompt": true]
            AXIsProcessTrustedWithOptions(options as CFDictionary)
            openPrivacySettings(anchor: "Privacy_Accessibility")
        case "screen_recording":
            NativeSystemPermissionService.requestScreenCaptureAccess()
            openPrivacySettings(anchor: "Privacy_ScreenCapture")
        case "full_disk_access":
            openPrivacySettings(anchor: "Privacy_AllFiles")
        default:
            break
        }
    }

    static func isGranted(_ id: String) -> Bool {
        switch id {
        case "accessibility":
            AXIsProcessTrusted()
        case "screen_recording":
            NativeSystemPermissionService.hasScreenCaptureAccess
        case "full_disk_access":
            hasFullDiskAccess()
        default:
            false
        }
    }

    static func settingsTitle(_ id: String) -> String {
        switch id {
        case "accessibility": "隐私与安全性 > 辅助功能"
        case "screen_recording": "隐私与安全性 > 屏幕与系统音频录制"
        case "full_disk_access": "隐私与安全性 > 完全磁盘访问"
        default: "隐私与安全性"
        }
    }

    static func permissionPurpose(_ id: String) -> String {
        switch id {
        case "accessibility": "允许 ChatOS 向本机应用发送鼠标、键盘和滚动事件。"
        case "screen_recording": "允许 ChatOS 获取真实屏幕画面，用于视觉定位和操作验证。"
        case "full_disk_access": "允许 ChatOS 在你明确要求时访问受 macOS 隐私保护的目录。"
        default: "允许 ChatOS 使用所选的 macOS 系统能力。"
        }
    }

    @MainActor
    private static func openPrivacySettings(anchor: String) {
        guard let url = URL(
            string: "x-apple.systempreferences:com.apple.preference.security?\(anchor)"
        ) else { return }
        NSWorkspace.shared.open(url)
    }

    private static func hasFullDiskAccess() -> Bool {
        let databaseURL = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/com.apple.TCC/TCC.db")
        guard let handle = try? FileHandle(forReadingFrom: databaseURL) else { return false }
        defer { try? handle.close() }
        return (try? handle.read(upToCount: 1)) != nil
    }

    private static func permission(
        id: String,
        label: String,
        summary: String,
        ready: Bool,
        canRequest: Bool,
        requestLabel: String,
        note: String
    ) -> LocalConnectorSystemPermission {
        .init(
            id: id,
            label: label,
            summary: summary,
            status: ready ? "ready" : "action_required",
            statusLabel: ready ? "已就绪" : "需要授权",
            required: false,
            canRequest: canRequest,
            requestLabel: requestLabel,
            note: note,
            lastError: nil
        )
    }
}
