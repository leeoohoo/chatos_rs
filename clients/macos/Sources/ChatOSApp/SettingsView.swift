import ChatOSCore
import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var selection: SettingsSection = .general

    var body: some View {
        AuthenticationGateView(authentication: model.authentication) {
            authenticatedSettings
        }
    }

    private var authenticatedSettings: some View {
        NavigationSplitView {
            List(selection: $selection) {
                Section("ChatOS") {
                    settingsRow(.general)
                    settingsRow(.pet)
                    settingsRow(.globalUtilities)
                    settingsRow(.cloudAI)
                }
                Section(model.localized("本机连接器", english: "Native Connector")) {
                    settingsRow(.connection)
                    settingsRow(.plugins)
                    settingsRow(.approvals)
                    settingsRow(.runtime)
                    settingsRow(.sandbox)
                }
            }
            .listStyle(.sidebar)
            .navigationTitle(model.localized("设置", english: "Settings"))
            .navigationSplitViewColumnWidth(min: 190, ideal: 220, max: 260)
        } detail: {
            detail
        }
        .task {
            applyRequestedSectionIfNeeded()
            activate(selection)
        }
        .onChange(of: selection) { _, next in
            model.localConnectorControl.clearMessages()
            activate(next)
        }
        .onChange(of: model.authentication.phase) { _, phase in
            if case .signedOut = phase {
                selection = .general
            }
        }
        .onChange(of: model.requestedConnectorSettingsTab) { _, _ in
            applyRequestedSectionIfNeeded()
        }
    }

    private func settingsRow(_ item: SettingsSection) -> some View {
        Label(item.title(language: model.interfaceLanguage), systemImage: item.systemImage)
            .tag(item)
    }

    @ViewBuilder
    private var detail: some View {
        VStack(spacing: 0) {
            if selection.connectorTab != nil {
                if let error = model.localConnectorControl.errorMessage {
                    banner(error, image: "exclamationmark.triangle.fill", tint: .orange)
                } else if let notice = model.localConnectorControl.notice {
                    banner(notice, image: "checkmark.circle.fill", tint: .green)
                }
            }
            selectedDetail
                .workspaceFill(alignment: .topLeading)
        }
        .workspaceFill(alignment: .topLeading)
        .background(Color(nsColor: .windowBackgroundColor))
        .navigationTitle(selection.title(language: model.interfaceLanguage))
    }

    @ViewBuilder
    private var selectedDetail: some View {
        switch selection {
        case .general:
            generalSettings
        case .pet:
            PetSettingsView(preferences: model.petPreferences)
        case .globalUtilities:
            GlobalUtilitiesSettingsView(
                preferences: model.globalUtilityPreferences,
                hotKeys: model.globalUtilityCoordinator.hotKeys
            )
        default:
            connectorDetail
        }
    }

    private var generalSettings: some View {
        SettingsGroupedPage {
            languageCard
            displayCard
            powerCard
            accountCard
        }
    }

    private var languageCard: some View {
        LocalConnectorCard(
            model.localized("语言", english: "Language"),
            subtitle: model.localized(
                "分别控制客户端界面和 AI 内部上下文。",
                english: "Control the client interface and AI internal context independently."
            ),
            systemImage: "globe"
        ) {
            VStack(spacing: 0) {
                languagePreferenceRow(
                    title: model.localized("界面语言", english: "Interface language"),
                    detail: model.localized(
                        "修改后客户端界面会立即切换。",
                        english: "The client interface switches immediately."
                    ),
                    selection: $model.interfaceLanguage
                )
                Divider().padding(.vertical, 12)
                languagePreferenceRow(
                    title: model.localized("内部上下文语言", english: "Internal context language"),
                    detail: model.localized(
                        "用于后续 AI system prompt、内置 MCP prompt 和任务过程。",
                        english: "Used for subsequent AI system prompts, built-in MCP prompts, and task progress."
                    ),
                    selection: $model.contextLanguage
                )

                if model.isLanguagePreferencesLoading || model.isLanguagePreferencesSaving {
                    Divider().padding(.vertical, 12)
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text(model.localized(
                            model.isLanguagePreferencesLoading ? "正在读取账号设置…" : "正在保存账号设置…",
                            english: model.isLanguagePreferencesLoading
                                ? "Loading account settings…"
                                : "Saving account settings…"
                        ))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        Spacer()
                    }
                }
                if let error = model.languagePreferencesError {
                    Divider().padding(.vertical, 12)
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .appFont(.caption)
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    private func languagePreferenceRow(
        title: String,
        detail: String,
        selection: Binding<ChatOSLanguage>
    ) -> some View {
        HStack(alignment: .center, spacing: 24) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).appFont(.headline)
                Text(detail)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 20)
            Picker(title, selection: selection) {
                Text(model.localized("中文", english: "Chinese"))
                    .tag(ChatOSLanguage.simplifiedChinese)
                Text("English").tag(ChatOSLanguage.english)
            }
            .labelsHidden()
            .frame(width: 150)
        }
    }

    private var displayCard: some View {
        LocalConnectorCard(
            model.localized("显示", english: "Display"),
            subtitle: model.localized(
                "调整 ChatOS 界面的整体阅读密度。",
                english: "Adjust the overall reading density of ChatOS."
            ),
            systemImage: "textformat.size"
        ) {
            HStack(spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.localized("字体大小", english: "Font size"))
                        .appFont(.headline)
                    Text(model.localized(
                        "标题、正文和辅助信息会保持相同的层级比例。",
                        english: "Titles, body text, and supporting text keep the same visual hierarchy."
                    ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                }
                Spacer(minLength: 20)
                Image(systemName: "textformat.size.smaller")
                    .foregroundStyle(.secondary)
                Slider(value: $model.interfaceFontSize, in: 12...18, step: 1)
                    .frame(width: 220)
                Image(systemName: "textformat.size.larger")
                    .foregroundStyle(.secondary)
                Text("\(Int(model.interfaceFontSize)) pt")
                    .appFont(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 42, alignment: .trailing)
                Button(model.localized("恢复默认", english: "Reset")) {
                    model.interfaceFontSize = 14
                }
            }
        }
    }

    private var powerCard: some View {
        LocalConnectorCard(
            model.localized("运行与电源", english: "Runtime & Power"),
            subtitle: model.localized(
                "控制长时间任务运行时的系统行为。",
                english: "Control system behavior while long-running tasks are active."
            ),
            systemImage: "bolt.fill"
        ) {
            HStack(alignment: .center, spacing: 20) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.localized(
                        "程序运行时保持 Mac 唤醒",
                        english: "Keep Mac awake while ChatOS is running"
                    ))
                    .appFont(.headline)
                    Text(model.localized(
                        "阻止空闲系统睡眠；屏幕仍可熄灭，也不会阻止合盖、关机或用户主动睡眠。",
                        english: "Prevents idle system sleep. The display may still turn off, and closing the lid, shutting down, or manually sleeping still works."
                    ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 20)
                Toggle("", isOn: $model.preventsIdleSystemSleep)
                    .labelsHidden()
                    .toggleStyle(.switch)
            }
        }
    }

    private var accountCard: some View {
        LocalConnectorCard(
            model.localized("账号", english: "Account"),
            subtitle: model.localized(
                "当前登录的 ChatOS 平台身份。",
                english: "The ChatOS platform identity currently signed in."
            ),
            systemImage: "person.crop.circle"
        ) {
            if case let .authenticated(session) = model.authentication.phase {
                HStack(spacing: 14) {
                    Image(systemName: "person.crop.circle.fill")
                        .appFont(.system(size: 34))
                        .foregroundStyle(.tint)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(session.user.displayName ?? session.user.username)
                            .appFont(.headline)
                        if let displayName = session.user.displayName,
                           displayName != session.user.username {
                            Text(session.user.username)
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Spacer()
                    Text(session.user.role)
                        .appFont(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 9)
                        .padding(.vertical, 4)
                        .background(.quaternary, in: Capsule())
                    Button(model.localized("退出登录", english: "Sign out"), role: .destructive) {
                        model.authentication.logout()
                    }
                }
            } else {
                Label(
                    model.localized(
                        "登录状态已失效，请在 ChatOS 主窗口重新登录。",
                        english: "Your session has expired. Sign in again from the main ChatOS window."
                    ),
                    systemImage: "person.crop.circle.badge.exclamationmark"
                )
                .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var connectorDetail: some View {
        switch selection {
        case .cloudAI: LocalConnectorModelsView(viewModel: model.localConnectorControl)
        case .connection: LocalConnectorConnectionView(viewModel: model.localConnectorControl)
        case .plugins: LocalConnectorPluginsView(viewModel: model.localConnectorControl)
        case .approvals: LocalConnectorApprovalsView(viewModel: model.localConnectorControl)
        case .runtime: LocalConnectorRuntimePermissionsView(viewModel: model.localConnectorControl)
        case .sandbox: LocalConnectorSandboxView(viewModel: model.localConnectorControl)
        case .general, .pet, .globalUtilities: EmptyView()
        }
    }

    private func activate(_ section: SettingsSection) {
        guard let tab = section.connectorTab else { return }
        model.localConnectorControl.selectedTab = tab
        model.localConnectorControl.refreshSelectedTab()
    }

    private func applyRequestedSectionIfNeeded() {
        guard let tab = model.requestedConnectorSettingsTab,
              let section = SettingsSection(connectorTab: tab) else { return }
        selection = section
        model.consumeConnectorSettingsRequest()
    }

    private func banner(_ text: String, image: String, tint: Color) -> some View {
        HStack(spacing: 9) {
            Image(systemName: image).foregroundStyle(tint)
            Text(text).appFont(.callout).textSelection(.enabled)
            Spacer()
            Button(
                model.localized("关闭", english: "Dismiss"),
                systemImage: "xmark",
                action: model.localConnectorControl.clearMessages
            )
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(tint.opacity(0.08))
        .overlay(alignment: .bottom) { Divider() }
    }
}

private enum SettingsSection: String, CaseIterable, Hashable {
    case general, pet, globalUtilities, cloudAI, connection, plugins, approvals, runtime, sandbox

    func title(language: ChatOSLanguage) -> String {
        if language == .english {
            switch self {
            case .general: return "General & Account"
            case .pet: return "Pet"
            case .globalUtilities: return "Global Utilities"
            case .cloudAI: return "AI Models"
            case .connection: return "Device & Gateway"
            case .plugins: return "Plugins & Skills"
            case .approvals: return "Command Approvals"
            case .runtime: return "Runtime & Permissions"
            case .sandbox: return "Access Control"
            }
        }
        return switch self {
        case .general: "常规与账号"
        case .pet: "宠物"
        case .globalUtilities: "全局工具"
        case .cloudAI: "AI 模型配置"
        case .connection: "设备与网关"
        case .plugins: "插件与 Skills"
        case .approvals: "命令审批"
        case .runtime: "运行与系统权限"
        case .sandbox: "权限控制"
        }
    }

    var systemImage: String {
        return switch self {
        case .general: "gearshape"
        case .pet: "pawprint.fill"
        case .globalUtilities: "command.square.fill"
        case .cloudAI: "sparkles"
        case .connection: "network"
        case .plugins: "puzzlepiece.extension"
        case .approvals: "checkmark.shield"
        case .runtime: "lock.open.display"
        case .sandbox: "shield.lefthalf.filled"
        }
    }

    var eyebrow: String {
        switch self {
        case .pet: "PET"
        case .globalUtilities: "GLOBAL UTILITIES"
        case .cloudAI: "CLOUD AI"
        case .connection: "NATIVE CONNECTOR"
        case .plugins: "PLUGINS & SKILLS"
        case .approvals: "APPROVAL"
        case .runtime: "SYSTEM ACCESS"
        case .sandbox: "PERMISSION POLICY"
        case .general: "SETTINGS"
        }
    }

    func description(language: ChatOSLanguage) -> String {
        if language == .english {
            switch self {
            case .general: return "Manage interface language and the current account."
            case .pet: return "Manage the global pet and event notifications outside the main window."
            case .globalUtilities: return "Manage global shortcuts, capture, clipboard history, and quick search."
            case .cloudAI: return "Manage ChatOS cloud models and the local approval model."
            case .connection: return "Manage the Swift Native Connector device identity and gateway connection."
            case .plugins: return "Manage plugins and skills running on this Mac."
            case .approvals: return "Configure approval rules for sensitive commands and Computer Use."
            case .runtime: return "Inspect client runtime health and macOS permissions."
            case .sandbox: return "Configure local boundaries for files, network, and processes."
            }
        }
        return switch self {
        case .general: "管理界面语言与当前账号。"
        case .pet: "管理脱离主窗口显示的全局宠物与事件提醒。"
        case .globalUtilities: "管理全局快捷键、截屏录屏、剪贴板历史和快速搜索。"
        case .cloudAI: "管理 ChatOS 云端模型，并选择本机审批模型。"
        case .connection: "管理 Swift Native Connector 的设备身份与网关长连接。"
        case .plugins: "管理运行在这台 Mac 上的 Plugin 与 Skills。"
        case .approvals: "配置敏感命令与 Computer Use 的审批策略。"
        case .runtime: "检查客户端运行状态与 macOS 系统授权。"
        case .sandbox: "配置文件、网络和进程的本机安全边界。"
        }
    }

    var connectorTab: LocalConnectorControlTab? {
        switch self {
        case .general, .pet, .globalUtilities: nil
        case .cloudAI: .models
        case .connection: .connection
        case .plugins: .plugins
        case .approvals: .approvals
        case .runtime: .runtime
        case .sandbox: .sandbox
        }
    }

    init?(connectorTab: LocalConnectorControlTab) {
        switch connectorTab {
        case .connection: self = .connection
        case .plugins: self = .plugins
        case .terminal: return nil
        case .models: self = .cloudAI
        case .approvals: self = .approvals
        case .runtime: self = .runtime
        case .sandbox: self = .sandbox
        }
    }
}
