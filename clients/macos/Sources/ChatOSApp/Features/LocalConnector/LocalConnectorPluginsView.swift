import ChatOSCore
import SwiftUI

struct LocalConnectorPluginsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.scenePhase) private var scenePhase
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var searchText = ""
    @State private var browserExtensionInstalled = false

    var body: some View {
        SettingsGroupedPage {
            LocalConnectorCard(
                model.localized("查找 Plugin", english: "Find Plugins"),
                subtitle: model.localized(
                    "按名称、Skill、发布者或分类筛选。",
                    english: "Filter by name, skill, publisher, or category."
                ),
                systemImage: "magnifyingglass"
            ) {
                HStack {
                    TextField(model.localized(
                        "搜索 Plugin、Skill 或发布者",
                        english: "Search plugins, skills, or publishers"
                    ), text: $searchText)
                    Text(model.localized(
                        "\(filteredPlugins.count) 项",
                        english: "\(filteredPlugins.count) items"
                    ))
                        .appFont(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }

            if viewModel.plugins.isEmpty && viewModel.isLoading {
                LocalConnectorCard(
                    model.localized("Plugin Marketplace", english: "Plugin Marketplace"),
                    systemImage: "puzzlepiece.extension"
                ) {
                    ProgressView(model.localized(
                        "正在同步 Plugin Marketplace…",
                        english: "Syncing Plugin Marketplace…"
                    ))
                        .frame(maxWidth: .infinity, minHeight: 180)
                }
            } else if viewModel.plugins.isEmpty, viewModel.errorMessage != nil {
                LocalConnectorCard(
                    model.localized("Plugin Marketplace", english: "Plugin Marketplace"),
                    systemImage: "wifi.exclamationmark"
                ) {
                    ContentUnavailableView(
                        model.localized("无法加载 Plugin", english: "Unable to load plugins"),
                        systemImage: "wifi.exclamationmark",
                        description: Text(model.localized(
                            "请检查登录状态或网关连接后重试。",
                            english: "Check your sign-in status or gateway connection, then try again."
                        ))
                    )
                    .frame(minHeight: 180)
                }
            } else if filteredPlugins.isEmpty {
                LocalConnectorCard(
                    model.localized("Plugin Marketplace", english: "Plugin Marketplace"),
                    systemImage: "puzzlepiece.extension"
                ) {
                    ContentUnavailableView(
                        model.localized("没有匹配的 Plugin", english: "No matching plugins"),
                        systemImage: "puzzlepiece.extension",
                        description: Text(searchText.isEmpty
                            ? model.localized("当前没有可用的 Plugin。", english: "No plugins are currently available.")
                            : model.localized("尝试更换搜索词。", english: "Try a different search term."))
                    )
                    .frame(minHeight: 180)
                }
            } else {
                ForEach(filteredPlugins) { plugin in
                    pluginCard(plugin)
                }
            }
        }
        .onAppear {
            refreshBrowserExtensionStatus()
            refreshBrowserExtensionPairingStatuses()
            automaticallyGuideBrowserExtensionIfNeeded()
        }
        .onChange(of: viewModel.plugins) { _, _ in
            refreshBrowserExtensionStatus()
            refreshBrowserExtensionPairingStatuses()
            automaticallyGuideBrowserExtensionIfNeeded()
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                refreshBrowserExtensionStatus()
                refreshBrowserExtensionPairingStatuses()
            }
        }
    }

    private var filteredPlugins: [LocalConnectorPlugin] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return viewModel.plugins }
        return viewModel.plugins.filter {
            [$0.displayName, $0.description, $0.publisher, $0.category]
                .joined(separator: " ")
                .lowercased()
                .contains(query)
        }
    }

    private func pluginCard(_ plugin: LocalConnectorPlugin) -> some View {
        let isOperating = viewModel.pluginOperationIDs.contains(plugin.id)
        return LocalConnectorCard(
            plugin.displayName,
            subtitle: "\(plugin.publisher) · \(plugin.latestVersion)",
            systemImage: "puzzlepiece.extension.fill"
        ) {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    if plugin.hasUI == true {
                        Label(model.localized("应用", english: "App"), systemImage: "macwindow")
                            .appFont(.caption2.weight(.semibold))
                            .foregroundStyle(.blue)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(.blue.opacity(0.12), in: Capsule())
                    }
                    if plugin.installed, !plugin.permissions.isEmpty {
                        permissionHealthBadge(plugin.permissions)
                    }
                    if isOperating {
                        ProgressView()
                            .controlSize(.small)
                    }
                    Spacer()
                    if plugin.installed {
                        Toggle(
                            model.localized("启用", english: "Enabled"),
                            isOn: Binding(
                                get: { plugin.enabled },
                                set: { viewModel.setPluginEnabled(id: plugin.id, enabled: $0) }
                            )
                        )
                        .toggleStyle(.switch)
                        .controlSize(.small)
                    }
                }
                Text(plugin.description)
                    .appFont(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                if plugin.installed {
                    permissionSection(plugin, isOperating: isOperating)
                }
                if plugin.installed, BrowserExtensionGuide.isBrowserPlugin(plugin) {
                    browserExtensionSection(plugin, isOperating: isOperating)
                }
                HStack {
                    Text(plugin.category)
                        .appFont(.caption2.weight(.medium))
                        .padding(.horizontal, 7)
                        .padding(.vertical, 3)
                        .background(.quaternary, in: Capsule())
                    if plugin.updateAvailable {
                        Label(model.localized("有更新", english: "Update available"), systemImage: "arrow.down.circle.fill")
                            .appFont(.caption2)
                            .foregroundStyle(.blue)
                    }
                    Spacer()
                    if plugin.installed {
                        if plugin.updateAvailable {
                            Button(model.localized("更新", english: "Update")) {
                                installPlugin(plugin)
                            }
                                .buttonStyle(.borderedProminent)
                        }
                        Button(model.localized("卸载", english: "Uninstall"), role: .destructive) { viewModel.uninstallPlugin(id: plugin.id) }
                    } else {
                        Button(model.localized("安装", english: "Install")) {
                            installPlugin(plugin)
                        }
                            .buttonStyle(.borderedProminent)
                            .disabled(!plugin.installAvailable)
                    }
                }
                .disabled(isOperating)
            }
        }
    }

    private func browserExtensionSection(
        _ plugin: LocalConnectorPlugin,
        isOperating: Bool
    ) -> some View {
        let isPaired = viewModel.browserExtensionPairedPluginIDs.contains(plugin.id)
        return VStack(alignment: .leading, spacing: 10) {
            Divider()
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: isPaired || browserExtensionInstalled
                    ? "checkmark.circle.fill"
                    : "puzzlepiece.extension.fill")
                    .foregroundStyle(isPaired || browserExtensionInstalled ? .green : .blue)
                    .font(.title3)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(model.localized(
                            isPaired ? "Google Chrome 已连接" : "连接您的 Google Chrome",
                            english: isPaired ? "Google Chrome Connected" : "Connect Your Google Chrome"
                        ))
                            .appFont(.subheadline.weight(.semibold))
                        Text(isPaired
                            ? model.localized("已连接", english: "Connected")
                            : browserExtensionInstalled
                            ? model.localized("已安装", english: "Installed")
                            : model.localized("还需安装扩展", english: "Extension Required"))
                            .appFont(.caption2.weight(.semibold))
                            .foregroundStyle(isPaired || browserExtensionInstalled ? .green : .orange)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(
                                (isPaired || browserExtensionInstalled ? Color.green : Color.orange).opacity(0.1),
                                in: Capsule()
                            )
                    }
                    Text(isPaired
                        ? model.localized(
                            "此 Chrome 用户配置已完成一次性授权，后续浏览器任务会自动连接。",
                            english: "This Chrome profile has completed one-time authorization. Future browser tasks reconnect automatically."
                        )
                        : model.localized(
                            "Browser CDP 使用您现有的 Chrome。点击后，Chatos 会自动启动一次性本机连接服务并打开正式扩展页面；首次连接完成后，后续浏览器任务会自动重连。",
                            english: "Browser CDP uses your existing Chrome. ChatOS automatically starts the one-time local pairing service and opens the official extension page; later browser tasks reconnect automatically."
                        ))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 12)
                VStack(alignment: .trailing, spacing: 5) {
                    if !isPaired {
                        Button {
                            startBrowserExtensionGuide(plugin)
                        } label: {
                            Label(
                                browserExtensionInstalled
                                    ? model.localized("连接 Google Chrome", english: "Connect Google Chrome")
                                    : model.localized("安装 Chrome 扩展", english: "Install Chrome Extension"),
                                systemImage: "arrow.up.forward.app"
                            )
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(isOperating)
                    }
                    if browserExtensionInstalled {
                        Button(model.localized("查看商店页面", english: "View Store Page")) {
                            BrowserExtensionGuide.openWebStore()
                        }
                        .buttonStyle(.borderless)
                        .controlSize(.small)
                    }
                }
            }
        }
    }

    private func installPlugin(_ plugin: LocalConnectorPlugin) {
        viewModel.installPlugin(id: plugin.id) {
            refreshBrowserExtensionStatus()
            if BrowserExtensionGuide.isBrowserPlugin(plugin) {
                startBrowserExtensionGuide(plugin, onlyIfStorePromptNeeded: true)
            }
        }
    }

    private func startBrowserExtensionGuide(
        _ plugin: LocalConnectorPlugin,
        onlyIfStorePromptNeeded: Bool = false
    ) {
        if onlyIfStorePromptNeeded,
           !BrowserExtensionGuide.shouldAutomaticallyGuide(pluginVersion: plugin.latestVersion) {
            return
        }
        viewModel.startBrowserExtensionGuide(pluginID: plugin.id) {
            refreshBrowserExtensionStatus()
            viewModel.refreshBrowserExtensionPairingStatus(pluginID: plugin.id)
            if browserExtensionInstalled {
                BrowserExtensionGuide.openOnboarding()
            } else if onlyIfStorePromptNeeded {
                BrowserExtensionGuide.openWebStoreAfterInstallIfNeeded(
                    pluginVersion: plugin.latestVersion
                )
            } else {
                BrowserExtensionGuide.openWebStore()
            }
        }
    }

    private func refreshBrowserExtensionStatus() {
        browserExtensionInstalled = BrowserExtensionGuide.isExtensionInstalled()
    }

    private func refreshBrowserExtensionPairingStatuses() {
        for plugin in viewModel.plugins where plugin.installed
            && BrowserExtensionGuide.isBrowserPlugin(plugin) {
            viewModel.refreshBrowserExtensionPairingStatus(pluginID: plugin.id)
        }
    }

    private func automaticallyGuideBrowserExtensionIfNeeded() {
        guard let plugin = viewModel.plugins.first(where: {
            $0.installed && BrowserExtensionGuide.isBrowserPlugin($0)
        }), BrowserExtensionGuide.shouldAutomaticallyGuide(
            pluginVersion: plugin.latestVersion
        ), !viewModel.pluginOperationIDs.contains(plugin.id) else {
            return
        }
        startBrowserExtensionGuide(plugin, onlyIfStorePromptNeeded: true)
    }

    private func permissionSection(
        _ plugin: LocalConnectorPlugin,
        isOperating: Bool
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Divider()
            HStack {
                Label(model.localized("权限与访问", english: "Permissions & Access"), systemImage: "lock.shield")
                    .appFont(.subheadline.weight(.semibold))
                if !plugin.permissions.isEmpty {
                    Text(permissionCountLabel(plugin.permissions))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    viewModel.refreshPluginPermissions(id: plugin.id)
                } label: {
                    Label(model.localized("重新检测", english: "Check Again"), systemImage: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .controlSize(.small)
                .disabled(isOperating)
            }
            if plugin.permissions.isEmpty {
                Text(model.localized(
                    "该 Plugin 未声明额外权限，或当前安装版本尚未提供可读取的权限清单。",
                    english: "This plugin declares no additional permissions, or its installed version does not expose a readable permission manifest."
                ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            } else {
                let systemPermissions = plugin.permissions.filter(isSystemPermission)
                let capabilities = plugin.permissions.filter { !isSystemPermission($0) }
                if !systemPermissions.isEmpty {
                    permissionGroupHeader(
                        model.localized("系统权限", english: "System Permissions"),
                        detail: model.localized(
                            "显示 macOS 当前真实授权状态",
                            english: "Shows the current macOS authorization status"
                        )
                    )
                    permissionGrid(
                        plugin: plugin,
                        permissions: systemPermissions,
                        isOperating: isOperating
                    )
                }
                if !capabilities.isEmpty {
                    permissionGroupHeader(
                        model.localized("插件能力", english: "Plugin Capabilities"),
                        detail: model.localized(
                            "安装后已可用，无需在系统设置中开启",
                            english: "Available after installation; no System Settings action required"
                        )
                    )
                    permissionGrid(
                        plugin: plugin,
                        permissions: capabilities,
                        isOperating: isOperating
                    )
                }
            }
        }
    }

    private func permissionGroupHeader(_ title: String, detail: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 7) {
            Text(title)
                .appFont(.caption.weight(.semibold))
            Text(detail)
                .appFont(.caption2)
                .foregroundStyle(.secondary)
        }
    }

    private func permissionGrid(
        plugin: LocalConnectorPlugin,
        permissions: [LocalConnectorPluginPermission],
        isOperating: Bool
    ) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 280), spacing: 8, alignment: .top)],
            alignment: .leading,
            spacing: 8
        ) {
            ForEach(permissions) { permission in
                permissionRow(plugin: plugin, permission: permission, isOperating: isOperating)
            }
        }
    }

    private func permissionRow(
        plugin: LocalConnectorPlugin,
        permission: LocalConnectorPluginPermission,
        isOperating: Bool
    ) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: permissionSymbol(permission.status))
                .foregroundStyle(permissionColor(permission.status))
                .frame(width: 18, height: 18)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 5) {
                    Text(pluginPermissionLabel(permission))
                        .appFont(.caption.weight(.semibold))
                    if permission.required {
                        Text(model.localized("必需", english: "Required"))
                            .appFont(.caption2.weight(.medium))
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 1)
                            .background(.quaternary, in: Capsule())
                    }
                    Spacer(minLength: 4)
                    Text(pluginPermissionStatus(permission))
                        .appFont(.caption2.weight(.semibold))
                        .foregroundStyle(permissionColor(permission.status))
                }
                Text(pluginPermissionSummary(permission))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                if permission.status == "action_required", let target = permission.settingsTarget {
                    Text(pluginPermissionSettingsTarget(permission, fallback: target))
                        .appFont(.caption2)
                        .foregroundStyle(.tertiary)
                }
                if let note = permission.note,
                   permission.status != "ready" || !permission.required {
                    Text(pluginPermissionNote(permission, fallback: note))
                        .appFont(.caption2)
                        .foregroundStyle(permission.status == "action_required" ? .orange : .secondary)
                        .lineLimit(2)
                }
                if permission.canRequest {
                    Button(pluginPermissionRequestLabel(permission)) {
                        viewModel.requestPluginPermission(
                            pluginID: plugin.id,
                            permissionID: permission.permissionID
                        )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(isOperating)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .padding(10)
        .background(permissionBackground(permission.status), in: RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(permissionColor(permission.status).opacity(0.16), lineWidth: 1)
        }
    }

    private func permissionHealthBadge(_ permissions: [LocalConnectorPluginPermission]) -> some View {
        let actionCount = permissions.filter { $0.status == "action_required" }.count
        let unknownCount = permissions.filter { $0.status == "unknown" }.count
        let title: String
        let color: Color
        let symbol: String
        if actionCount > 0 {
            title = model.localized(
                "\(actionCount) 项待开启",
                english: "\(actionCount) need attention"
            )
            color = .orange
            symbol = "exclamationmark.triangle.fill"
        } else if unknownCount > 0 {
            title = model.localized(
                "\(unknownCount) 项待检测",
                english: "\(unknownCount) unchecked"
            )
            color = .secondary
            symbol = "questionmark.circle.fill"
        } else {
            title = model.localized("权限正常", english: "Permissions ready")
            color = .green
            symbol = "checkmark.circle.fill"
        }
        return Label(title, systemImage: symbol)
            .appFont(.caption2.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(color.opacity(0.1), in: Capsule())
    }

    private func pluginPermissionLabel(_ permission: LocalConnectorPluginPermission) -> String {
        let english: String? = switch permission.permissionID {
        case "process.spawn": "Launch Local Processes"
        case "computer.control": "Control Mac Apps"
        case "computer.accessibility": "Accessibility"
        case "computer.screen-recording": "Screen & System Audio Recording"
        case "workspace.read": "Read Project Files"
        case "workspace.write": "Write Project Files"
        case "artifact.create": "Create Deliverables"
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

    private func pluginPermissionSummary(_ permission: LocalConnectorPluginPermission) -> String {
        let english: String? = switch permission.permissionID {
        case "process.spawn": "Launch local MCP servers declared by the plugin."
        case "computer.control": "Send mouse, keyboard, and scroll events."
        case "computer.accessibility": "Allow the plugin to send real input events to apps on this Mac."
        case "computer.screen-recording": "Allow the plugin to capture the real screen."
        case "workspace.read": "Read project files bound to the current task."
        case "workspace.write": "Modify project files within the current task's authorization scope."
        case "artifact.create": "Create files in ChatOS-managed deliverable folders."
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

    private func pluginPermissionStatus(_ permission: LocalConnectorPluginPermission) -> String {
        switch permission.status {
        case "ready": return model.localized("已可用", english: "Available")
        case "action_required": return model.localized("需要开启", english: "Action Required")
        case "unknown": return model.localized("等待检测", english: "Not Checked")
        default: return permission.statusLabel
        }
    }

    private func pluginPermissionRequestLabel(_ permission: LocalConnectorPluginPermission) -> String {
        switch permission.status {
        case "ready": model.localized("已允许", english: "Allowed")
        case "action_required", "unknown": model.localized("去开启", english: "Open Settings")
        default: permission.requestLabel
        }
    }

    private func pluginPermissionSettingsTarget(
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

    private func pluginPermissionNote(
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

    private func permissionCountLabel(_ permissions: [LocalConnectorPluginPermission]) -> String {
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

    private func isSystemPermission(_ permission: LocalConnectorPluginPermission) -> Bool {
        permission.permissionID == "computer.accessibility"
            || permission.permissionID == "computer.screen-recording"
    }

    private func permissionSymbol(_ status: String) -> String {
        switch status {
        case "ready": "checkmark.circle.fill"
        case "action_required": "exclamationmark.triangle.fill"
        case "unknown": "questionmark.circle.fill"
        default: "clock.fill"
        }
    }

    private func permissionColor(_ status: String) -> Color {
        switch status {
        case "ready": .green
        case "action_required": .orange
        case "unknown": .secondary
        default: .blue
        }
    }

    private func permissionBackground(_ status: String) -> Color {
        switch status {
        case "action_required": .orange.opacity(0.07)
        case "ready": .green.opacity(0.05)
        default: .secondary.opacity(0.06)
        }
    }
}
