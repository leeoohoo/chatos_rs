import ChatOSCore
import Foundation

enum NativePluginPermissionInspector {
    static func permissions(
        record: NativeInstalledPluginRecord,
        manifest: NativePluginManifest
    ) -> [LocalConnectorPluginPermission] {
        let diagnostics = permissionDiagnostics(record: record, manifest: manifest)
        var requirements = manifest.permissions.map(PermissionRequirement.init)
        if manifest.name == "open-computer-use" {
            for permissionID in ["computer.accessibility", "computer.screen-recording"]
            where !requirements.contains(where: { $0.permission == permissionID }) {
                requirements.append(.init(permission: permissionID, required: true))
            }
        }
        return requirements.map { requirement in
            makePermission(requirement, diagnostic: diagnostics[requirement.permission])
        }
    }

    static func request(
        record: NativeInstalledPluginRecord,
        manifest: NativePluginManifest,
        permissionID: String
    ) throws -> Bool {
        guard isSystemPermission(permissionID) else { return false }
        guard manifest.name == "open-computer-use" else { return false }
        _ = try runLauncher(record: record, command: "doctor", timeout: 35)
        return true
    }

    private static func permissionDiagnostics(
        record: NativeInstalledPluginRecord,
        manifest: NativePluginManifest
    ) -> [String: DiagnosticPermission] {
        guard manifest.name == "open-computer-use",
              launcherSupportsPermissionCheck(record: record),
              let data = try? runLauncher(
                record: record,
                command: "check-permissions",
                timeout: 15
              ),
              let response = try? JSONDecoder().decode(DiagnosticResponse.self, from: data) else {
            return [:]
        }
        return Dictionary(uniqueKeysWithValues: response.permissions.compactMap { item in
            guard let permissionID = manifestPermissionID(for: item.kind) else { return nil }
            return (permissionID, item)
        })
    }

    private static func launcherSupportsPermissionCheck(
        record: NativeInstalledPluginRecord
    ) -> Bool {
        let url = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .appendingPathComponent("bin/open-computer-use")
        guard let handle = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: 512 * 1_024) else { return false }
        return String(decoding: data, as: UTF8.self).contains("check-permissions")
    }

    private static func makePermission(
        _ requirement: PermissionRequirement,
        diagnostic: DiagnosticPermission?
    ) -> LocalConnectorPluginPermission {
        let presentation = presentation(for: requirement.permission)
        if let diagnostic {
            return .init(
                permissionID: requirement.permission,
                label: diagnostic.title,
                summary: presentation.summary,
                required: requirement.required,
                status: diagnostic.granted ? "ready" : "action_required",
                statusLabel: diagnostic.granted ? "已允许" : "需要开启",
                canRequest: !diagnostic.granted,
                requestLabel: diagnostic.granted ? "已允许" : "去开启",
                settingsTarget: diagnostic.systemSettingsTitle,
                note: diagnostic.granted
                    ? "该权限已授予当前插件运行副本。"
                    : "授权后请重新检测；屏幕录制权限可能需要重连 MCP 才会生效。"
            )
        }
        if isSystemPermission(requirement.permission) {
            return .init(
                permissionID: requirement.permission,
                label: presentation.label,
                summary: presentation.summary,
                required: requirement.required,
                status: "unknown",
                statusLabel: "等待检测",
                canRequest: true,
                requestLabel: "去开启",
                settingsTarget: presentation.settingsTarget,
                note: "插件尚未提供可读取的权限状态；可打开系统设置后重新检测。"
            )
        }
        return .init(
            permissionID: requirement.permission,
            label: presentation.label,
            summary: presentation.summary,
            required: requirement.required,
            status: "ready",
            statusLabel: "已可用",
            canRequest: false,
            requestLabel: "无需设置",
            note: requirement.required
                ? "安装后即可使用，无需在系统设置中开启。"
                : "无需系统授权；只有调用相关功能时才会用到。"
        )
    }

    private static func runLauncher(
        record: NativeInstalledPluginRecord,
        command: String,
        timeout: TimeInterval
    ) throws -> Data {
        let launcher = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .appendingPathComponent("bin/open-computer-use")
        let values = try launcher.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .isExecutableKey,
        ])
        guard values.isRegularFile == true,
              values.isSymbolicLink != true,
              values.isExecutable == true else {
            throw NativeConnectorError.pluginInstallation("Plugin 权限检测入口不可用")
        }
        let process = Process()
        process.executableURL = launcher
        process.arguments = [command]
        process.currentDirectoryURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        try process.run()
        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning, Date() < deadline {
            Thread.sleep(forTimeInterval: 0.05)
        }
        if process.isRunning {
            process.terminate()
            throw NativeConnectorError.pluginInstallation("Plugin 权限检测超时")
        }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        guard data.count <= 1_024 * 1_024 else {
            throw NativeConnectorError.pluginInstallation("Plugin 权限检测响应过大")
        }
        guard process.terminationStatus == 0 else {
            let detail = String(
                decoding: errors.fileHandleForReading.readDataToEndOfFile(),
                as: UTF8.self
            ).trimmingCharacters(in: .whitespacesAndNewlines)
            throw NativeConnectorError.pluginInstallation(
                detail.isEmpty ? "Plugin 权限检测失败" : detail
            )
        }
        return data
    }

    private static func isSystemPermission(_ permissionID: String) -> Bool {
        permissionID == "computer.accessibility"
            || permissionID == "computer.screen-recording"
    }

    private static func manifestPermissionID(for diagnosticKind: String) -> String? {
        switch diagnosticKind {
        case "accessibility": "computer.accessibility"
        case "screenRecording": "computer.screen-recording"
        default: nil
        }
    }

    private static func presentation(for permissionID: String) -> (
        label: String,
        summary: String,
        settingsTarget: String?
    ) {
        switch permissionID {
        case "process.spawn":
            ("启动本机进程", "启动插件声明的本机 MCP 服务。", nil)
        case "computer.control":
            ("控制本机应用", "发送鼠标、键盘和滚动事件。", nil)
        case "computer.accessibility":
            ("辅助功能", "允许插件向本机应用发送真实输入事件。", "隐私与安全性 > 辅助功能")
        case "computer.screen-recording":
            ("屏幕与系统音频录制", "允许插件获取真实屏幕画面。", "隐私与安全性 > 屏幕与系统音频录制")
        case "workspace.read":
            ("读取项目文件", "读取当前任务绑定的项目文件。", nil)
        case "workspace.write":
            ("写入项目文件", "在当前任务授权范围内修改项目文件。", nil)
        case "artifact.create":
            ("创建交付文件", "在 ChatOS 管理的产物目录中创建文件。", nil)
        case "browser.managed.launch":
            ("启动托管浏览器", "启动由 ChatOS 隔离管理的浏览器会话。", nil)
        case "browser.page.read":
            ("读取网页", "读取托管浏览器中的页面内容与结构。", nil)
        case "browser.page.control":
            ("控制网页", "在托管浏览器中导航、点击、输入和滚动。", nil)
        case "browser.chrome.attach":
            ("连接现有 Chrome", "连接用户明确配对的 Chrome 浏览器。", nil)
        case "browser.network.observe":
            ("查看浏览器网络", "查看控制台、网络请求、WebSocket 和 HAR 活动。", nil)
        case "browser.network.intercept":
            ("拦截浏览器请求", "中止或模拟用户明确选择的网络请求。", nil)
        case "browser.file.transfer":
            ("浏览器文件传输", "上传用户选择的文件，并把下载内容保存为受管文件。", nil)
        case "browser.cdp.raw":
            ("执行原始 CDP 指令", "执行经过明确审批的 Chrome DevTools Protocol 指令。", nil)
        default:
            (permissionID, "插件安装后提供的功能能力，无需在系统设置中开启。", nil)
        }
    }
}

private struct PermissionRequirement {
    var permission: String
    var required: Bool

    init(permission: String, required: Bool) {
        self.permission = permission
        self.required = required
    }

    init(_ permission: NativePluginManifest.Permission) {
        self.init(permission: permission.permission, required: permission.required)
    }
}

private struct DiagnosticResponse: Decodable {
    var permissions: [DiagnosticPermission]
}

private struct DiagnosticPermission: Decodable {
    var kind: String
    var title: String
    var granted: Bool
    var purpose: String
    var systemSettingsTitle: String
}
