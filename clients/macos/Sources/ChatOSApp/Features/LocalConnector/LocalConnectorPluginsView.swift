import ChatOSCore
import AppKit
import SwiftUI

struct LocalConnectorPluginsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.scenePhase) private var scenePhase
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var searchText = ""
    @State private var browserExtensionInstalled = false
    @State private var browserExtensionStatusTask: Task<Void, Never>?
    @State private var showsCustomInstaller = false

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
                    Button(model.localized("检查更新", english: "Check for Updates"), systemImage: "arrow.clockwise") {
                        viewModel.loadPlugins(forceRefresh: true)
                    }
                    .disabled(viewModel.isLoading)
                    Button(
                        model.localized("让安装助手处理", english: "Ask Installation Assistant"),
                        systemImage: "bubble.left.and.text.bubble.right"
                    ) {
                        model.selection = .contact(WorkspaceSystemContact.pluginInstallerID)
                    }
                    Button(model.localized("手动安装…", english: "Install Manually…"), systemImage: "plus") {
                        showsCustomInstaller = true
                    }
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
            refreshBrowserExtensionStatus(automaticallyGuideAfterRefresh: true)
            refreshBrowserExtensionPairingStatuses()
        }
        .onChange(of: viewModel.plugins) { _, _ in
            refreshBrowserExtensionStatus(automaticallyGuideAfterRefresh: true)
            refreshBrowserExtensionPairingStatuses()
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                refreshBrowserExtensionStatus()
                refreshBrowserExtensionPairingStatuses()
            }
        }
        .onDisappear {
            browserExtensionStatusTask?.cancel()
            browserExtensionStatusTask = nil
        }
        .sheet(isPresented: $showsCustomInstaller) {
            CustomPluginInstallerSheet(viewModel: viewModel)
                .environmentObject(model)
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
            subtitle: pluginVersionSubtitle(plugin),
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
                    if plugin.requiresReinstall {
                        Label(
                            model.localized("需要重新安装", english: "Reinstall required"),
                            systemImage: "exclamationmark.shield.fill"
                        )
                        .appFont(.caption2.weight(.semibold))
                        .foregroundStyle(.orange)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 3)
                        .background(.orange.opacity(0.12), in: Capsule())
                    } else if plugin.installed, !plugin.permissions.isEmpty {
                        permissionHealthBadge(plugin.permissions)
                    }
                    if isOperating {
                        ProgressView()
                            .controlSize(.small)
                    }
                    Spacer()
                    if plugin.installed, !plugin.requiresReinstall {
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
                if let issue = plugin.installationIssue {
                    Label(installationIssueMessage(issue), systemImage: "exclamationmark.triangle.fill")
                        .appFont(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if plugin.installed, !plugin.requiresReinstall {
                    permissionSection(plugin, isOperating: isOperating)
                }
                if plugin.installed,
                   !plugin.requiresReinstall,
                   BrowserExtensionGuide.isBrowserPlugin(plugin) {
                    browserExtensionSection(plugin, isOperating: isOperating)
                }
                if let operationError = viewModel.pluginErrorMessages[plugin.id] {
                    Label(operationError, systemImage: "exclamationmark.triangle.fill")
                        .appFont(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
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
                        if plugin.requiresReinstall, plugin.installAvailable {
                            Button(model.localized("安全重装", english: "Secure Reinstall")) {
                                installPlugin(plugin)
                            }
                                .buttonStyle(.borderedProminent)
                        } else if plugin.updateAvailable {
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

    private func pluginVersionSubtitle(_ plugin: LocalConnectorPlugin) -> String {
        guard let installedVersion = plugin.installedVersion else {
            return "\(plugin.publisher) · \(plugin.latestVersion)"
        }
        if plugin.updateAvailable {
            return model.localized(
                "\(plugin.publisher) · 已安装 \(installedVersion) · 最新 \(plugin.latestVersion)",
                english: "\(plugin.publisher) · Installed \(installedVersion) · Latest \(plugin.latestVersion)"
            )
        }
        return model.localized(
            "\(plugin.publisher) · 已安装 \(installedVersion)",
            english: "\(plugin.publisher) · Installed \(installedVersion)"
        )
    }

    private func installationIssueMessage(
        _ issue: LocalConnectorPluginInstallationIssue
    ) -> String {
        switch issue {
        case .missingIntegrityRecord:
            return model.localized(
                "此 Plugin 来自旧版安装，缺少可信的逐文件完整性记录。它已停止提供给任务使用，请从 Marketplace 安全重装。",
                english: "This plugin is from an older installation and lacks a trusted per-file integrity record. It is unavailable to tasks until securely reinstalled from Marketplace."
            )
        case .integrityCheckFailed:
            return model.localized(
                "此 Plugin 的安装文件未通过完整性校验。它已停止提供给任务使用，请从 Marketplace 安全重装。",
                english: "This plugin installation failed its integrity check. It is unavailable to tasks until securely reinstalled from Marketplace."
            )
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
            if BrowserExtensionGuide.isBrowserPlugin(plugin) {
                refreshBrowserExtensionStatus { _ in
                    startBrowserExtensionGuide(plugin, onlyIfStorePromptNeeded: true)
                }
            } else {
                refreshBrowserExtensionStatus()
            }
        }
    }

    private func startBrowserExtensionGuide(
        _ plugin: LocalConnectorPlugin,
        onlyIfStorePromptNeeded: Bool = false
    ) {
        if onlyIfStorePromptNeeded,
           !BrowserExtensionGuide.shouldAutomaticallyGuide(
                pluginVersion: plugin.latestVersion,
                extensionInstalled: browserExtensionInstalled
           ) {
            return
        }
        viewModel.startBrowserExtensionGuide(pluginID: plugin.id) {
            viewModel.refreshBrowserExtensionPairingStatus(pluginID: plugin.id)
            refreshBrowserExtensionStatus { installed in
                if installed {
                    BrowserExtensionGuide.openOnboarding()
                } else if onlyIfStorePromptNeeded {
                    BrowserExtensionGuide.openWebStoreAfterInstallIfNeeded(
                        pluginVersion: plugin.latestVersion,
                        extensionInstalled: false
                    )
                } else {
                    BrowserExtensionGuide.openWebStore()
                }
            }
        }
    }

    private func refreshBrowserExtensionStatus(
        automaticallyGuideAfterRefresh: Bool = false,
        completion: ((Bool) -> Void)? = nil
    ) {
        browserExtensionStatusTask?.cancel()
        browserExtensionStatusTask = Task {
            let detectionTask = Task.detached(priority: .utility) {
                BrowserExtensionGuide.isExtensionInstalled()
            }
            let installed = await withTaskCancellationHandler {
                await detectionTask.value
            } onCancel: {
                detectionTask.cancel()
            }
            guard !Task.isCancelled else { return }
            browserExtensionInstalled = installed
            if automaticallyGuideAfterRefresh {
                automaticallyGuideBrowserExtensionIfNeeded(extensionInstalled: installed)
            }
            completion?(installed)
        }
    }

    private func refreshBrowserExtensionPairingStatuses() {
        for plugin in viewModel.plugins where plugin.installed
            && BrowserExtensionGuide.isBrowserPlugin(plugin) {
            viewModel.refreshBrowserExtensionPairingStatus(pluginID: plugin.id)
        }
    }

    private func automaticallyGuideBrowserExtensionIfNeeded(extensionInstalled: Bool) {
        guard let plugin = viewModel.plugins.first(where: {
            $0.installed && BrowserExtensionGuide.isBrowserPlugin($0)
        }), BrowserExtensionGuide.shouldAutomaticallyGuide(
            pluginVersion: plugin.latestVersion,
            extensionInstalled: extensionInstalled
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

    private func pluginPermissionSummary(_ permission: LocalConnectorPluginPermission) -> String {
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

enum LocalConnectorCustomMCPExamples {
    static let rea = LocalConnectorCustomMCPDraft(
        displayName: "Reverse Engineer Anything",
        description: "Reverse engineering tools exposed through REA MCP.",
        componentKey: "reverse-engineer-anything",
        packageName: "rea-agents",
        packageVersion: "6.5.0",
        binaryName: "rea",
        arguments: ["mcp"],
        accessProfile: .developerAnalysis
    )
}

private struct CustomPluginInstallerSheet: View {
    private enum InstallKind: String, CaseIterable, Identifiable {
        case npmMCP
        case pluginFolder
        case gitPlugin
        case npmPlugin
        var id: String { rawValue }
    }

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var kind: InstallKind = .npmMCP
    @State private var displayName = ""
    @State private var description = ""
    @State private var componentKey = ""
    @State private var packageName = ""
    @State private var packageVersion = ""
    @State private var binaryName = ""
    @State private var argumentsText = ""
    @State private var accessProfile: LocalConnectorCustomMCPDraft.AccessProfile = .restricted
    @State private var folderPath = ""
    @State private var gitReference = ""
    @State private var subdirectory = ""
    @State private var inspection: LocalConnectorPluginInspection?
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.localized("安装自定义 MCP / Plugin", english: "Install Custom MCP / Plugin"))
                        .font(.title2.weight(.semibold))
                    Text(model.localized(
                        "先隔离预检工具清单，再确认写入本机安装记录。",
                        english: "Inspect the tool list in isolation before saving the installation locally."
                    ))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button(
                    model.localized("交给安装助手", english: "Use Installation Assistant"),
                    systemImage: "bubble.left.and.text.bubble.right"
                ) {
                    dismiss()
                    model.selection = .contact(WorkspaceSystemContact.pluginInstallerID)
                }
                Button(model.localized("关闭", english: "Close")) { dismiss() }
            }
            .padding(20)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Picker("", selection: $kind) {
                        Text(model.localized("从 npm 安装 MCP", english: "MCP from npm"))
                            .tag(InstallKind.npmMCP)
                        Text(model.localized("导入插件 / MCP 目录", english: "Plugin / MCP Folder"))
                            .tag(InstallKind.pluginFolder)
                        Text(model.localized("Git 插件", english: "Git Plugin"))
                            .tag(InstallKind.gitPlugin)
                        Text(model.localized("npm 插件", english: "npm Plugin"))
                            .tag(InstallKind.npmPlugin)
                    }
                    .pickerStyle(.segmented)

                    if kind == .npmMCP {
                        npmForm
                    } else if kind == .pluginFolder {
                        folderForm
                    } else {
                        remotePluginForm
                    }

                    Label(
                        preflightSafetyText,
                        systemImage: "lock.shield"
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)

                    if let errorMessage {
                        Text(errorMessage)
                            .font(.caption)
                            .foregroundStyle(.red)
                            .textSelection(.enabled)
                    }

                    if let inspection {
                        inspectionView(inspection)
                    }
                }
                .padding(20)
            }

            Divider()
            HStack {
                if viewModel.isInspectingCustomPlugin || viewModel.isInstallingCustomPlugin {
                    ProgressView().controlSize(.small)
                }
                Spacer()
                Button(model.localized("预检工具", english: "Inspect Tools")) { inspect() }
                    .disabled(isBusy || !canInspect)
                Button(model.localized("确认安装", english: "Confirm Install")) { install() }
                    .buttonStyle(.borderedProminent)
                    .disabled(isBusy || inspection == nil)
            }
            .padding(16)
        }
        .frame(minWidth: 680, idealWidth: 760, minHeight: 620, idealHeight: 720)
        .onChange(of: inputSignature) { _, _ in
            inspection = nil
            errorMessage = nil
        }
    }

    private var npmForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(model.localized("固定版本 npm MCP", english: "Pinned npm MCP"))
                    .font(.headline)
                Spacer()
                Button(model.localized("填入 REA 示例", english: "Use REA Example")) {
                    let example = LocalConnectorCustomMCPExamples.rea
                    displayName = example.displayName
                    description = example.description
                    componentKey = example.componentKey
                    packageName = example.packageName
                    packageVersion = example.packageVersion
                    binaryName = example.binaryName
                    argumentsText = example.arguments.joined(separator: "\n")
                    accessProfile = example.accessProfile
                }
            }
            HStack {
                field(model.localized("显示名称", english: "Display Name"), text: $displayName)
                field(model.localized("组件 Key", english: "Component Key"), text: $componentKey)
            }
            HStack {
                field(model.localized("npm 包名", english: "npm Package"), text: $packageName)
                field(model.localized("精确版本", english: "Exact Version"), text: $packageVersion)
                field(model.localized("Bin 名称", english: "Bin Name"), text: $binaryName)
            }
            field(model.localized("说明", english: "Description"), text: $description)
            VStack(alignment: .leading, spacing: 5) {
                Text(model.localized("访问级别", english: "Access Profile"))
                    .font(.caption.weight(.medium))
                Picker("", selection: $accessProfile) {
                    Text(model.localized("受限（仅本地）", english: "Restricted (Local Only)"))
                        .tag(LocalConnectorCustomMCPDraft.AccessProfile.restricted)
                    Text(model.localized("网络客户端", english: "Network Client"))
                        .tag(LocalConnectorCustomMCPDraft.AccessProfile.networkClient)
                    Text(model.localized("开发与逆向分析", english: "Developer Analysis"))
                        .tag(LocalConnectorCustomMCPDraft.AccessProfile.developerAnalysis)
                }
                .pickerStyle(.segmented)
                Text(accessProfileDescription)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 5) {
                Text(model.localized("参数（每行一个，不经过 shell）", english: "Arguments (one per line, no shell parsing)"))
                    .font(.caption.weight(.medium))
                TextEditor(text: $argumentsText)
                    .font(.system(.body, design: .monospaced))
                    .frame(height: 82)
                    .padding(5)
                    .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 7))
            }
        }
    }

    private var folderForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(model.localized(
                "选择插件或 MCP 的根目录",
                english: "Choose the plugin or MCP root folder"
            ))
                .font(.headline)
            Text(model.localized(
                "ChatOS 会自动识别 ChatOS Plugin、Agent Plugin、Codex、Claude、独立 Skill 和主流应用的 MCP JSON 配置。请选择清单文件所在的这一层。",
                english: "ChatOS recognizes ChatOS, portable Agent, Codex and Claude Plugins, standalone Skills, and MCP JSON configurations from mainstream agent apps. Choose the folder that contains the manifest."
            ))
                .font(.caption)
                .foregroundStyle(.secondary)

            VStack(alignment: .leading, spacing: 6) {
                Label(
                    model.localized("支持自动识别", english: "Recognized automatically"),
                    systemImage: "checkmark.circle.fill"
                )
                .font(.caption.weight(.semibold))
                .foregroundStyle(.green)
                Text("SKILL.md  ·  plugin.json  ·  chatos.plugin.json  ·  mcp.json  ·  .mcp.json\n.codex-plugin  ·  .claude-plugin  ·  .cursor/mcp.json  ·  .vscode/mcp.json")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 9))

            HStack {
                TextField(
                    model.localized(
                        "例如：/Users/me/Downloads/my-plugin",
                        english: "Example: /Users/me/Downloads/my-plugin"
                    ),
                    text: $folderPath
                )
                    .textFieldStyle(.roundedBorder)
                Button(
                    model.localized("选择根目录…", english: "Choose Root Folder…")
                ) { chooseFolder() }
            }
        }
    }

    private var remotePluginForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(kind == .gitPlugin
                ? model.localized("从 Git 仓库安装 Agent Plugin", english: "Agent Plugin from Git")
                : model.localized("从 npm 安装完整 Agent Plugin", english: "Complete Agent Plugin from npm"))
                .font(.headline)
            Text(model.localized(
                "适用于包含 Plugin 清单、SKILL.md 或 MCP 配置的完整包；如果 npm 包只是一个 MCP 命令，请改用“从 npm 安装 MCP”。",
                english: "Use this for a complete package containing a plugin manifest, SKILL.md, or MCP configuration. If the npm package only exposes an MCP command, use “MCP from npm” instead."
            ))
                .font(.caption)
                .foregroundStyle(.secondary)
            if kind == .gitPlugin {
                field(model.localized("HTTPS Git 仓库 URL", english: "HTTPS Git Repository URL"), text: $folderPath)
                HStack {
                    field(model.localized("分支 / Tag / Commit（可选）", english: "Branch / Tag / Commit (Optional)"), text: $gitReference)
                    field(model.localized("插件子目录（可选）", english: "Plugin Subdirectory (Optional)"), text: $subdirectory)
                }
            } else {
                HStack {
                    field(model.localized("npm 包名", english: "npm Package"), text: $packageName)
                    field(model.localized("精确版本", english: "Exact Version"), text: $packageVersion)
                }
                field(model.localized("插件子目录（可选）", english: "Plugin Subdirectory (Optional)"), text: $subdirectory)
            }
        }
    }

    private var preflightSafetyText: String {
        if kind == .npmMCP {
            return model.localized(
                "预检不会运行 npm install scripts；命令和参数不会作为 shell 字符串解析。",
                english: "Preflight never runs npm install scripts. Commands and arguments are never parsed as a shell string."
            )
        }
        return kind == .pluginFolder
            ? model.localized(
                "导入前会复制到隔离目录并检查清单、路径和实际 MCP 工具；原目录不会被修改。",
                english: "Before import, ChatOS copies the package into isolation and verifies its manifest, paths, and actual MCP tools. The original folder is not modified."
            )
            : model.localized(
                "ChatOS 不运行 npm install scripts 或仓库脚本；Git/npm 内容会在隔离目录预检，并在确认安装时重新获取和核对指纹。",
                english: "ChatOS never runs npm install scripts or repository scripts. Git/npm content is inspected in isolation, then reacquired and fingerprinted again at install time."
            )
    }

    private func field(_ title: String, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).font(.caption.weight(.medium))
            TextField(title, text: text).textFieldStyle(.roundedBorder)
        }
    }

    private func inspectionView(_ value: LocalConnectorPluginInspection) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(
                model.localized("预检通过", english: "Preflight Passed"),
                systemImage: "checkmark.shield.fill"
            )
            .font(.headline)
            .foregroundStyle(.green)
            Text("\(value.displayName) · \(value.version)")
                .font(.subheadline.weight(.semibold))
            Text(value.sourceSummary)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            ForEach(value.components) { component in
                VStack(alignment: .leading, spacing: 5) {
                    Text(component.kind == .skill
                        ? "\(component.displayName) · Skill"
                        : "\(component.displayName) · \(component.toolNames.count) tools")
                        .font(.caption.weight(.semibold))
                    if component.kind == .mcp {
                        Text(component.toolNames.prefix(24).joined(separator: ", "))
                            .font(.caption.monospaced())
                            .foregroundStyle(.tertiary)
                            .textSelection(.enabled)
                        if component.toolNames.count > 24 {
                            Text("+\(component.toolNames.count - 24) more")
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
            }
            Text("SHA-256 \(value.fingerprint)")
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
                .textSelection(.enabled)
        }
        .padding(14)
        .background(.green.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
    }

    private var isBusy: Bool {
        viewModel.isInspectingCustomPlugin || viewModel.isInstallingCustomPlugin
    }

    private var canInspect: Bool {
        if kind == .pluginFolder || kind == .gitPlugin {
            return !folderPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
        if kind == .npmPlugin {
            return [packageName, packageVersion]
                .allSatisfy { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        }
        return [displayName, componentKey, packageName, packageVersion, binaryName]
            .allSatisfy { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    private var draft: LocalConnectorCustomMCPDraft {
        .init(
            displayName: displayName,
            description: description,
            componentKey: componentKey,
            packageName: packageName,
            packageVersion: packageVersion,
            binaryName: binaryName,
            arguments: argumentsText.split(whereSeparator: \.isNewline).map(String.init),
            environment: [:],
            accessProfile: accessProfile
        )
    }

    private var inputSignature: String {
        [kind.rawValue, displayName, description, componentKey, packageName, packageVersion,
         binaryName, argumentsText, accessProfile.rawValue, folderPath, gitReference,
         subdirectory].joined(separator: "\u{0}")
    }

    private var accessProfileDescription: String {
        switch accessProfile {
        case .restricted:
            model.localized(
                "只启动 MCP，不开放外网、开发工具、浏览器或项目文件。",
                english: "Starts the MCP without network, developer-tool, browser, or project access."
            )
        case .networkClient:
            model.localized(
                "允许 MCP 连接外部 API；仍不能读取项目或启动浏览器。",
                english: "Allows outbound API access, but not project reads or browser launches."
            )
        case .developerAnalysis:
            model.localized(
                "用于开发与逆向工具：可读项目、调用开发工具、启动受管浏览器并保存证据。",
                english: "For development and reverse engineering: project reads, developer tools, managed browser launches, and evidence artifacts."
            )
        }
    }

    private var sourceDraft: LocalConnectorPluginSourceDraft {
        switch kind {
        case .pluginFolder:
            .init(kind: .localFolder, location: folderPath, subdirectory: nil)
        case .gitPlugin:
            .init(
                kind: .gitRepository,
                location: folderPath,
                gitReference: gitReference,
                subdirectory: subdirectory
            )
        case .npmPlugin:
            .init(
                kind: .npmPackage,
                location: packageName,
                exactVersion: packageVersion,
                subdirectory: subdirectory
            )
        case .npmMCP:
            .init(kind: .npmPackage, location: packageName, exactVersion: packageVersion)
        }
    }

    private func inspect() {
        inspection = nil
        errorMessage = nil
        if kind == .npmMCP {
            viewModel.inspectCustomMCP(draft) { result in handleInspection(result) }
        } else {
            viewModel.inspectPluginSource(sourceDraft) { result in handleInspection(result) }
        }
    }

    private func handleInspection(_ result: Result<LocalConnectorPluginInspection, Error>) {
        switch result {
        case let .success(value): inspection = value
        case let .failure(error): errorMessage = error.localizedDescription
        }
    }

    private func install() {
        guard let inspection else { return }
        errorMessage = nil
        let completion: @MainActor (Result<Void, Error>) -> Void = { result in
            switch result {
            case .success: dismiss()
            case let .failure(error): errorMessage = error.localizedDescription
            }
        }
        if kind == .npmMCP {
            viewModel.installCustomMCP(
                draft,
                expectedFingerprint: inspection.fingerprint,
                completion: completion
            )
        } else {
            viewModel.installPluginSource(
                sourceDraft,
                expectedFingerprint: inspection.fingerprint,
                completion: completion
            )
        }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = model.localized("选择", english: "Choose")
        if panel.runModal() == .OK { folderPath = panel.url?.path ?? folderPath }
    }
}
