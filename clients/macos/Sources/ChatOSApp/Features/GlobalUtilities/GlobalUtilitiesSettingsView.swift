import AppKit
import ChatOSCore
import SwiftUI

struct GlobalUtilitiesSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var preferences: GlobalUtilityPreferencesStore
    @ObservedObject var hotKeys: GlobalHotKeyService

    @State private var showsShortcutWarning = false

    var body: some View {
        SettingsGroupedPage {
            masterCard
            shortcutsCard
            privacyCard
        }
        .alert(
            model.localized("启用全局快捷键？", english: "Enable global shortcuts?"),
            isPresented: $showsShortcutWarning
        ) {
            Button(model.localized("取消", english: "Cancel"), role: .cancel) {}
            Button(model.localized("启用", english: "Enable")) {
                preferences.hasAcknowledgedShortcutConflicts = true
                preferences.isEnabled = true
            }
        } message: {
            Text(model.localized(
                "Control+A、Control+Q、Command+E 会覆盖部分应用的常用按键；Command+Space 通常被系统 Spotlight 占用，ChatOS 会在冲突时使用 Option+Space。你可以随时改键或关闭。",
                english: "Control+A, Control+Q, and Command+E may override common shortcuts in other apps. Command+Space is usually owned by Spotlight, so ChatOS uses Option+Space when needed. You can rebind or disable them at any time."
            ))
        }
    }

    private var masterCard: some View {
        LocalConnectorCard(
            model.localized("全局工具", english: "Global Utilities"),
            subtitle: model.localized(
                "主窗口关闭后仍可使用的本机效率入口。",
                english: "Local productivity tools that remain available after the main window closes."
            ),
            systemImage: "command.square.fill"
        ) {
            HStack(alignment: .center, spacing: 20) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.localized("启用全局快捷工具", english: "Enable global utilities"))
                        .appFont(.headline)
                    Text(model.localized(
                        "关闭后会注销全部全局快捷键，不会在后台拦截按键。",
                        english: "Turning this off unregisters every global shortcut."
                    ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                }
                Spacer(minLength: 20)
                Toggle("", isOn: masterBinding)
                    .labelsHidden()
                    .toggleStyle(.switch)
            }
        }
    }

    private var shortcutsCard: some View {
        LocalConnectorCard(
            model.localized("全局快捷键", english: "Global Shortcuts"),
            subtitle: model.localized(
                "点击快捷键按钮后直接按下新的组合键。",
                english: "Click a shortcut button, then press the new key combination."
            ),
            systemImage: "keyboard"
        ) {
            VStack(spacing: 0) {
                ForEach(Array(GlobalUtilityAction.allCases.enumerated()), id: \.element) { index, action in
                    if index > 0 {
                        Divider().padding(.vertical, 12)
                    }
                    shortcutRow(action)
                }

                Divider().padding(.vertical, 12)
                HStack {
                    Button(model.localized("恢复默认快捷键", english: "Restore Defaults")) {
                        preferences.restoreDefaults()
                    }
                    if usesQuickSearchFallback {
                        Button(model.localized(
                            "打开 Spotlight 快捷键设置",
                            english: "Open Spotlight Shortcut Settings"
                        )) {
                            openSpotlightShortcutSettings()
                        }
                    }
                    Spacer()
                }
            }
        }
    }

    private var privacyCard: some View {
        LocalConnectorCard(
            model.localized("隐私", english: "Privacy"),
            subtitle: model.localized(
                "截图、录屏、剪贴板和本地搜索默认只在这台 Mac 上处理。",
                english: "Capture, clipboard, and local search data stays on this Mac by default."
            ),
            systemImage: "hand.raised.fill"
        ) {
            Label(
                model.localized(
                    "未经你明确点击，截图、录屏和剪贴板内容不会上传到 ChatOS 服务端。",
                    english: "Screenshots, recordings, and clipboard content are never uploaded without an explicit action."
                ),
                systemImage: "lock.fill"
            )
            .appFont(.callout)
            .foregroundStyle(.secondary)
        }
    }

    private func shortcutRow(_ action: GlobalUtilityAction) -> some View {
        HStack(alignment: .center, spacing: 16) {
            Toggle("", isOn: actionEnabledBinding(action))
                .labelsHidden()
                .toggleStyle(.switch)
            Image(systemName: actionIcon(action))
                .frame(width: 22)
                .foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 4) {
                Text(actionTitle(action)).appFont(.headline)
                statusLabel(action)
            }
            Spacer(minLength: 20)
            HotKeyRecorderView(
                hotKey: preferences.hotKey(for: action),
                isEnglish: model.interfaceLanguage == .english
            ) { newValue in
                preferences.setHotKey(newValue, for: action)
            }
            .disabled(!preferences.isEnabled || !preferences.isActionEnabled(action))
        }
    }

    @ViewBuilder
    private func statusLabel(_ action: GlobalUtilityAction) -> some View {
        let presentation = statusPresentation(action)
        Label(presentation.text, systemImage: presentation.icon)
            .appFont(.caption)
            .foregroundStyle(presentation.color)
    }

    private var masterBinding: Binding<Bool> {
        Binding(
            get: { preferences.isEnabled },
            set: { enabled in
                if enabled,
                   !preferences.hasAcknowledgedShortcutConflicts {
                    showsShortcutWarning = true
                } else {
                    preferences.isEnabled = enabled
                }
            }
        )
    }

    private func actionEnabledBinding(_ action: GlobalUtilityAction) -> Binding<Bool> {
        Binding(
            get: { preferences.isActionEnabled(action) },
            set: { preferences.setActionEnabled($0, for: action) }
        )
    }

    private func statusPresentation(
        _ action: GlobalUtilityAction
    ) -> (text: String, icon: String, color: Color) {
        switch hotKeys.states[action] ?? .disabled {
        case let .registered(activeHotKey, usesFallback):
            if usesFallback {
                return (
                    model.localized(
                        "主快捷键被占用，当前使用 \(activeHotKey.displayName)",
                        english: "Primary shortcut is occupied; using \(activeHotKey.displayName)"
                    ),
                    "exclamationmark.triangle.fill",
                    .orange
                )
            }
            return (
                model.localized("已注册", english: "Registered"),
                "checkmark.circle.fill",
                .green
            )
        case let .conflict(requestedHotKey, _):
            return (
                model.localized(
                    "\(requestedHotKey.displayName) 已被其他应用占用",
                    english: "\(requestedHotKey.displayName) is used by another app"
                ),
                "xmark.circle.fill",
                .red
            )
        case let .unsupported(message):
            return (message, "exclamationmark.octagon.fill", .orange)
        case .disabled:
            return (
                model.localized("未注册", english: "Not registered"),
                "minus.circle",
                .secondary
            )
        }
    }

    private var usesQuickSearchFallback: Bool {
        guard case let .registered(_, usesFallback) = hotKeys.states[.quickSearch] else {
            return false
        }
        return usesFallback
    }

    private func actionTitle(_ action: GlobalUtilityAction) -> String {
        switch action {
        case .screenshot: model.localized("截屏", english: "Screenshot")
        case .screenRecording: model.localized("录屏", english: "Screen Recording")
        case .clipboardHistory: model.localized("剪贴板历史", english: "Clipboard History")
        case .quickSearch: model.localized("快速搜索", english: "Quick Search")
        }
    }

    private func actionIcon(_ action: GlobalUtilityAction) -> String {
        switch action {
        case .screenshot: "viewfinder"
        case .screenRecording: "record.circle"
        case .clipboardHistory: "clipboard"
        case .quickSearch: "magnifyingglass"
        }
    }

    private func openSpotlightShortcutSettings() {
        guard let url = URL(
            string: "x-apple.systempreferences:com.apple.Keyboard-Settings.extension?Shortcuts"
        ) else { return }
        NSWorkspace.shared.open(url)
    }
}
