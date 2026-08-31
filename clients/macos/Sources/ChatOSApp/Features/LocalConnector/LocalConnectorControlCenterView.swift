import SwiftUI

struct LocalConnectorControlCenterView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel

    var body: some View {
        HSplitView {
            navigation
                .frame(minWidth: 190, idealWidth: 210, maxWidth: 240)
            content
                .appFont(.body)
                .frame(minWidth: 620, maxWidth: .infinity, maxHeight: .infinity)
        }
        .workspaceFill()
        .navigationTitle(model.localized("本机控制中心", english: "Native Control Center"))
        .task {
            viewModel.refreshSelectedTab()
        }
        .onChange(of: viewModel.selectedTab) {
            viewModel.clearMessages()
            viewModel.refreshSelectedTab()
        }
    }

    private var navigation: some View {
        List(selection: $viewModel.selectedTab) {
            Section(model.localized("控制中心", english: "Control Center")) {
                ForEach(LocalConnectorControlTab.allCases) { tab in
                    Label(tab.title(language: model.interfaceLanguage), systemImage: tab.systemImage)
                        .tag(tab)
                }
            }
        }
        .workspaceFill()
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            HStack(spacing: 8) {
                Circle()
                    .fill(connectionColor)
                    .frame(width: 8, height: 8)
                VStack(alignment: .leading, spacing: 1) {
                    Text("SWIFT CONNECTOR")
                        .appFont(.caption2.weight(.semibold))
                        .foregroundStyle(.tertiary)
                    Text(connectionLabel)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(12)
            .background(.bar)
        }
    }

    private var content: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if let error = viewModel.errorMessage {
                messageBanner(error, systemImage: "exclamationmark.triangle.fill", tint: .orange)
            } else if let notice = viewModel.notice {
                messageBanner(notice, systemImage: "checkmark.circle.fill", tint: .green)
            }
            Group {
                switch viewModel.selectedTab {
                case .connection:
                    LocalConnectorConnectionView(viewModel: viewModel)
                case .plugins:
                    LocalConnectorPluginsView(viewModel: viewModel)
                case .terminal:
                    LocalConnectorTerminalView(viewModel: viewModel)
                case .models:
                    LocalConnectorModelsView(viewModel: viewModel)
                case .approvals:
                    LocalConnectorApprovalsView(viewModel: viewModel)
                case .runtime:
                    LocalConnectorRuntimePermissionsView(viewModel: viewModel)
                case .sandbox:
                    LocalConnectorSandboxView(viewModel: viewModel)
                }
            }
            .workspaceFill()
        }
        .workspaceFill()
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var header: some View {
        HStack(spacing: 14) {
            Image(systemName: viewModel.selectedTab.systemImage)
                .appFont(.system(size: 21, weight: .medium))
                .foregroundStyle(.tint)
                .frame(width: 42, height: 42)
                .background(.tint.opacity(0.1), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 3) {
                Text(viewModel.selectedTab.eyebrow)
                    .appFont(.caption2.weight(.semibold))
                    .tracking(0.8)
                    .foregroundStyle(.secondary)
                Text(viewModel.selectedTab.title(language: model.interfaceLanguage))
                    .appFont(.title2.weight(.semibold))
                Text(viewModel.selectedTab.description(language: model.interfaceLanguage))
                    .appFont(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if viewModel.isLoading || viewModel.isPerformingAction || viewModel.isStarting {
                ProgressView()
                    .controlSize(.small)
            }
            Button(model.localized("刷新", english: "Refresh"), systemImage: "arrow.clockwise") {
                viewModel.refreshSelectedTab()
            }
            .labelStyle(.iconOnly)
            .help(model.localized("刷新当前页面", english: "Refresh this page"))
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 18)
    }

    private func messageBanner(_ text: String, systemImage: String, tint: Color) -> some View {
        HStack(spacing: 9) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
            Text(text)
                .appFont(.callout)
                .textSelection(.enabled)
            Spacer()
            Button(model.localized("关闭", english: "Dismiss"), systemImage: "xmark", action: viewModel.clearMessages)
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(tint.opacity(0.08))
        .overlay(alignment: .bottom) { Divider() }
    }

    private var connectionColor: Color {
        guard let status = viewModel.status else { return .secondary }
        return status.connectorRunning ? .green : status.configured ? .orange : .secondary
    }

    private var connectionLabel: String {
        guard let status = viewModel.status else {
            return model.localized("正在启动", english: "Starting")
        }
        if status.connectorRunning {
            return model.localized("连接正常", english: "Connected")
        }
        return status.configured
            ? model.localized("等待网关", english: "Waiting for gateway")
            : model.localized("等待配对", english: "Waiting for pairing")
    }
}

struct LocalConnectorCard<Content: View>: View {
    // This component is shared by the Settings window and the standalone
    // connector control center. Keep its structure container-neutral: turning
    // it into Form/Section changes the semantics of every dynamic TextField
    // page and can create an AttributeGraph update loop.
    @Environment(\.localConnectorCardPresentation) private var presentation
    var title: String
    var subtitle: String?
    var systemImage: String
    @ViewBuilder var content: Content

    init(
        _ title: String,
        subtitle: String? = nil,
        systemImage: String,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.systemImage = systemImage
        self.content = content()
    }

    @ViewBuilder
    var body: some View {
        if presentation == .settingsGrouped {
            settingsGroupedBody
        } else {
            cardBody
        }
    }

    private var cardBody: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                Image(systemName: systemImage)
                    .foregroundStyle(.tint)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .appFont(.headline)
                    if let subtitle {
                        Text(subtitle)
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
            }
            content
        }
        .padding(18)
        .background(.background, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .stroke(.separator.opacity(0.65), lineWidth: 1)
        }
    }

    private var settingsGroupedBody: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(title, systemImage: systemImage)
                .appFont(.headline)
                .padding(.horizontal, 10)
            VStack(alignment: .leading, spacing: 14) {
                content
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
            if let subtitle {
                Text(subtitle)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 10)
            }
        }
    }
}

enum LocalConnectorCardPresentation {
    case card
    case settingsGrouped
}

private struct LocalConnectorCardPresentationKey: EnvironmentKey {
    static let defaultValue = LocalConnectorCardPresentation.card
}

extension EnvironmentValues {
    var localConnectorCardPresentation: LocalConnectorCardPresentation {
        get { self[LocalConnectorCardPresentationKey.self] }
        set { self[LocalConnectorCardPresentationKey.self] = newValue }
    }
}

struct SettingsGroupedPage<Content: View>: View {
    @ViewBuilder var content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        // Reproduce the old macOS grouped-settings appearance with stable
        // ScrollView/VStack layout. Do not replace this with Form globally:
        // model and plugin pages contain dynamic grids and editable controls.
        ScrollView {
            VStack(spacing: 22) {
                content
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .topLeading)
        }
        .background(AppPalette.canvas)
        .environment(\.localConnectorCardPresentation, .settingsGrouped)
    }
}

struct LocalConnectorKeyValueRow: View {
    var label: String
    var value: String
    var monospaced = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 16) {
            Text(label)
                .appFont(.callout)
                .foregroundStyle(.secondary)
            Spacer(minLength: 20)
            Text(value)
                .appFont(monospaced ? .system(.callout, design: .monospaced) : .callout)
                .multilineTextAlignment(.trailing)
                .textSelection(.enabled)
        }
    }
}
