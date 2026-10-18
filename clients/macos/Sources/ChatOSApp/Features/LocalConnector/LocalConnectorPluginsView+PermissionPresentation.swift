import ChatOSCore
import SwiftUI

extension LocalConnectorPluginsView {
    func pluginPermissionLabel(_ permission: LocalConnectorPluginPermission) -> String {
        let english: String? = switch permission.permissionID {
        case "process.spawn": "Launch Local Processes"
        case "computer.control": "Control Mac Apps"
        case "computer.accessibility": "Accessibility"
        case "computer.screen-recording": "Screen & System Audio Recording"
        case "workspace.read": "Read Project Files"
        case "workspace.write": "Write Project Files"
        case "artifact.create": "Create Deliverables"
        case "network.client": "Access Network"
        case "system.developer-tools.read": "Read Developer Tools"
        case "browser.managed.launch": "Launch Managed Browser"
        case "browser.page.read": "Read Web Pages"
        case "browser.page.control": "Control Web Pages"
        case "browser.chrome.attach": "Connect to Existing Chrome"
        case "browser.network.observe": "Inspect Browser Network"
        case "browser.network.intercept": "Intercept Browser Requests"
        case "browser.file.transfer": "Browser File Transfer"
        case "browser.cdp.raw": "Run Raw CDP Commands"
        default: nil
        }
        return model.interfaceLanguage == .english ? (english ?? permission.label) : permission.label
    }

    func pluginPermissionSummary(_ permission: LocalConnectorPluginPermission) -> String {
        let english: String? = switch permission.permissionID {
        case "process.spawn": "Launch local MCP servers declared by the plugin."
        case "computer.control": "Send mouse, keyboard, and scroll events."
        case "computer.accessibility": "Allow the plugin to send real input events to apps on this Mac."
        case "computer.screen-recording": "Allow the plugin to capture the real screen."
        case "workspace.read": "Read project files bound to the current task."
        case "workspace.write": "Modify project files within the current task's authorization scope."
        case "artifact.create": "Create files in ChatOS-managed deliverable folders."
        case "network.client": "Connect to external network services."
        case "system.developer-tools.read": "Read and invoke tools from Xcode or Command Line Tools."
        case "browser.managed.launch": "Launch browser sessions isolated and managed by ChatOS."
        case "browser.page.read": "Read page content and structure in the managed browser."
        case "browser.page.control": "Navigate, click, type, and scroll in the managed browser."
        case "browser.chrome.attach": "Connect to a Chrome browser explicitly paired by the user."
        case "browser.network.observe": "Inspect console, network, WebSocket, and HAR activity."
        case "browser.network.intercept": "Abort or mock network requests explicitly selected by the user."
        case "browser.file.transfer": "Upload selected files and save downloads as managed files."
        case "browser.cdp.raw": "Run explicitly approved Chrome DevTools Protocol commands."
        default: nil
        }
        return model.interfaceLanguage == .english ? (english ?? permission.summary) : permission.summary
    }

    func pluginPermissionStatus(_ permission: LocalConnectorPluginPermission) -> String {
        switch permission.status {
        case "ready": return model.localized("已可用", english: "Available")
        case "action_required": return model.localized("需要开启", english: "Action Required")
        case "unknown": return model.localized("等待检测", english: "Not Checked")
        default: return permission.statusLabel
        }
    }

    func pluginPermissionRequestLabel(_ permission: LocalConnectorPluginPermission) -> String {
        switch permission.status {
        case "ready": model.localized("已允许", english: "Allowed")
        case "action_required", "unknown": model.localized("去开启", english: "Open Settings")
        default: permission.requestLabel
        }
    }

    func pluginPermissionSettingsTarget(
        _ permission: LocalConnectorPluginPermission,
        fallback: String
    ) -> String {
        guard model.interfaceLanguage == .english else { return fallback }
        return switch permission.permissionID {
        case "computer.accessibility": "Privacy & Security > Accessibility"
        case "computer.screen-recording": "Privacy & Security > Screen & System Audio Recording"
        default: fallback
        }
    }

    func pluginPermissionNote(
        _ permission: LocalConnectorPluginPermission,
        fallback: String
    ) -> String {
        guard model.interfaceLanguage == .english else { return fallback }
        if permission.status == "action_required" {
            return "After authorizing, check again. Screen recording may require reconnecting the MCP server."
        }
        if permission.status == "unknown" {
            return "The plugin has not provided a readable permission state. Open System Settings, then check again."
        }
        return permission.required
            ? "Available after installation; no System Settings action is required."
            : "No system authorization is required. This is used only when the related feature runs."
    }

    func permissionCountLabel(_ permissions: [LocalConnectorPluginPermission]) -> String {
        let required = permissions.filter(\.required).count
        let optional = permissions.count - required
        return optional > 0
            ? model.localized(
                "\(permissions.count) 项 · \(required) 必需 · \(optional) 可选",
                english: "\(permissions.count) total · \(required) required · \(optional) optional"
            )
            : model.localized(
                "\(permissions.count) 项 · 全部必需",
                english: "\(permissions.count) total · all required"
            )
    }

    func isSystemPermission(_ permission: LocalConnectorPluginPermission) -> Bool {
        permission.permissionID == "computer.accessibility"
            || permission.permissionID == "computer.screen-recording"
    }

    func permissionSymbol(_ status: String) -> String {
        switch status {
        case "ready": "checkmark.circle.fill"
        case "action_required": "exclamationmark.triangle.fill"
        case "unknown": "questionmark.circle.fill"
        default: "clock.fill"
        }
    }

    func permissionColor(_ status: String) -> Color {
        switch status {
        case "ready": .green
        case "action_required": .orange
        case "unknown": .secondary
        default: .blue
        }
    }

    func permissionBackground(_ status: String) -> Color {
        switch status {
        case "action_required": .orange.opacity(0.07)
        case "ready": .green.opacity(0.05)
        default: .secondary.opacity(0.06)
        }
    }
}
