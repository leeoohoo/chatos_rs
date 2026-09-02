import AppKit
import ChatOSCore
import SwiftUI
import WebKit

struct PluginApplicationsView: View {
    @EnvironmentObject private var model: AppModel

    private let columns = [
        GridItem(.adaptive(minimum: 210, maximum: 280), spacing: 16),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(model.localized("应用", english: "Applications"))
                            .font(.system(size: 28, weight: .semibold, design: .rounded))
                        Text(model.localized(
                            "打开已安装并启用的插件应用。",
                            english: "Open installed and enabled plugin applications."
                        ))
                        .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(
                        model.localized("刷新", english: "Refresh"),
                        systemImage: "arrow.clockwise",
                        action: model.refreshPluginApplications
                    )
                    .disabled(model.isPluginApplicationsLoading)
                }

                if model.isPluginApplicationsLoading && model.pluginApplications.isEmpty {
                    HStack(spacing: 10) {
                        ProgressView()
                        Text(model.localized("正在读取插件应用…", english: "Loading plugin apps…"))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, minHeight: 220)
                } else if let error = model.pluginApplicationsError,
                          model.pluginApplications.isEmpty {
                    ContentUnavailableView(
                        model.localized("应用加载失败", english: "Applications Failed to Load"),
                        systemImage: "exclamationmark.triangle",
                        description: Text(error)
                    )
                } else if model.pluginApplications.isEmpty {
                    ContentUnavailableView {
                        Label(
                            model.localized("还没有插件应用", english: "No Plugin Applications"),
                            systemImage: "square.grid.2x2"
                        )
                    } description: {
                        Text(model.localized(
                            "请先在插件市场安装一个带页面的插件，并保持它处于启用状态。",
                            english: "Install a plugin with a workbench page from the marketplace and keep it enabled."
                        ))
                    } actions: {
                        Button(model.localized("打开插件管理", english: "Open Plugin Management")) {
                            model.openGlobalSearchSettings(tab: .plugins)
                        }
                    }
                    .frame(maxWidth: .infinity, minHeight: 320)
                } else {
                    LazyVGrid(columns: columns, alignment: .leading, spacing: 16) {
                        ForEach(model.pluginApplications) { application in
                            applicationCard(application)
                        }
                    }
                }
            }
            .padding(28)
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .task {
            if model.pluginApplications.isEmpty {
                model.refreshPluginApplications()
            }
        }
    }

    private func applicationCard(_ application: LocalConnectorPluginApplication) -> some View {
        Button {
            model.selection = .pluginApplication(application.pluginID, application.componentKey)
        } label: {
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .top) {
                    PluginApplicationIcon(application: application, size: 54)
                    Spacer()
                    Image(systemName: "arrow.up.right")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.tertiary)
                }

                VStack(alignment: .leading, spacing: 6) {
                    Text(application.displayName)
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                    Text(application.description.isEmpty
                         ? model.localized("插件应用", english: "Plugin application")
                         : application.description)
                        .font(.system(size: 12.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(3)
                        .frame(minHeight: 47, alignment: .topLeading)
                }

                Label(
                    application.requiresLocalRuntime
                        ? model.localized("本地服务", english: "Local service")
                        : model.localized("内嵌页面", english: "Embedded page"),
                    systemImage: application.requiresLocalRuntime ? "bolt.horizontal.circle" : "doc.richtext"
                )
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
            }
            .padding(17)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 18, style: .continuous)
                    .stroke(Color.primary.opacity(0.09), lineWidth: 1)
            }
            .contentShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .buttonStyle(.plain)
    }
}

struct PluginApplicationHostView: View {
    @EnvironmentObject private var model: AppModel
    let application: LocalConnectorPluginApplication

    @State private var launch: LocalConnectorPluginApplicationLaunch?
    @State private var errorMessage: String?
    @State private var reloadToken = 0
    @State private var contextChosen = false
    @State private var selectedProjectID: String?
    @State private var launchContext: LocalConnectorPluginApplicationContext?

    private var requiresContextSelection: Bool {
        application.contextScope == "project" || application.contextScope == "workspace"
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Button {
                    model.selection = .applications
                } label: {
                    Image(systemName: "chevron.left")
                }
                .buttonStyle(.borderless)
                .help(model.localized("返回应用列表", english: "Back to Applications"))

                PluginApplicationIcon(application: application, size: 30)
                Text(application.displayName)
                    .font(.system(size: 14, weight: .semibold))
                if contextChosen, requiresContextSelection {
                    Text(selectedProjectName ?? model.localized("公共项目", english: "Shared Project"))
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(.quaternary, in: Capsule())
                    Button(model.localized("切换", english: "Switch")) {
                        launch = nil
                        errorMessage = nil
                        launchContext = nil
                        contextChosen = false
                    }
                    .buttonStyle(.borderless)
                    .font(.system(size: 11))
                }
                Spacer()
                if launch != nil {
                    Button {
                        reloadToken += 1
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.borderless)
                    .help(model.localized("重新载入", english: "Reload"))
                }
            }
            .padding(.horizontal, 14)
            .frame(height: 48)
            .background(.bar)
            .overlay(alignment: .bottom) { Divider() }

            Group {
                if requiresContextSelection && !contextChosen {
                    contextPicker
                } else if let launch {
                    RestrictedPluginWebView(
                        url: launch.url,
                        websiteDataStoreID: launch.websiteDataStoreID,
                        reloadToken: reloadToken
                    )
                } else if let errorMessage {
                    ContentUnavailableView {
                        Label(
                            model.localized("应用无法打开", english: "Application Could Not Open"),
                            systemImage: "exclamationmark.triangle"
                        )
                    } description: {
                        Text(errorMessage)
                    } actions: {
                        Button(model.localized("重试", english: "Try Again")) {
                            start(context: launchContext)
                        }
                        Button(model.localized("返回应用列表", english: "Back to Applications")) {
                            model.selection = .applications
                        }
                    }
                } else {
                    VStack(spacing: 14) {
                        ProgressView()
                            .controlSize(.large)
                        Text(model.localized("正在启动插件应用…", english: "Starting plugin application…"))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .task(id: application.id) {
            if !requiresContextSelection {
                contextChosen = true
                start(context: nil)
            }
        }
    }

    private var selectedProjectName: String? {
        guard let selectedProjectID else { return nil }
        return model.workspaceProjects.first(where: { $0.id == selectedProjectID })?.name
    }

    private var contextPicker: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 6) {
                Text(model.localized("选择应用项目", english: "Choose Application Project"))
                    .font(.system(size: 24, weight: .semibold, design: .rounded))
                Text(model.localized(
                    "插件会为每个 ChatOS 用户和项目使用独立的数据目录。",
                    english: "The plugin uses a separate data directory for every ChatOS user and project."
                ))
                .foregroundStyle(.secondary)
            }
            ScrollView {
                LazyVStack(spacing: 10) {
                    if application.missingContext == "device" {
                        contextButton(
                            title: model.localized("公共项目", english: "Shared Project"),
                            subtitle: model.localized(
                                "用于没有关联 ChatOS 项目的图形和数据",
                                english: "For diagrams and data not tied to a ChatOS project"
                            ),
                            systemImage: "person.crop.square"
                        ) {
                            selectedProjectID = nil
                            let context = LocalConnectorPluginApplicationContext.device
                            launchContext = context
                            contextChosen = true
                            start(context: context)
                        }
                    }
                    ForEach(model.workspaceProjects) { project in
                        contextButton(
                            title: project.name,
                            subtitle: project.displayRootPath ?? project.rootPath
                                ?? model.localized("ChatOS 项目", english: "ChatOS project"),
                            systemImage: "folder"
                        ) {
                            selectedProjectID = project.id
                            let context = LocalConnectorPluginApplicationContext(
                                projectID: project.id,
                                projectName: project.name,
                                projectRoot: project.rootPath
                            )
                            launchContext = context
                            contextChosen = true
                            start(context: context)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: 680, maxHeight: 640, alignment: .leading)
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func contextButton(
        title: String,
        subtitle: String,
        systemImage: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: systemImage)
                    .font(.system(size: 20, weight: .medium))
                    .frame(width: 42, height: 42)
                    .background(Color.accentColor.opacity(0.12), in: RoundedRectangle(cornerRadius: 11))
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).font(.system(size: 14, weight: .semibold))
                    Text(subtitle).font(.system(size: 11.5)).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer()
                Image(systemName: "chevron.right").foregroundStyle(.tertiary)
            }
            .padding(13)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
            .overlay { RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.08)) }
        }
        .buttonStyle(.plain)
    }

    private func start(context: LocalConnectorPluginApplicationContext? = nil) {
        launch = nil
        errorMessage = nil
        Task {
            do {
                launch = try await model.launchPluginApplication(application, context: context)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}

private struct PluginApplicationIcon: View {
    let application: LocalConnectorPluginApplication
    let size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.24, style: .continuous)
                .fill(application.brandColor.flatMap(Color.init(pluginHex:)) ?? Color.accentColor)
                .shadow(color: .black.opacity(0.12), radius: 5, y: 2)
            if let iconURL = application.iconURL,
               let image = NSImage(contentsOf: iconURL) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
                    .padding(size * 0.16)
            } else {
                Image(systemName: "square.stack.3d.up.fill")
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(.white)
                    .padding(size * 0.24)
            }
        }
        .frame(width: size, height: size)
    }
}

private struct RestrictedPluginWebView: NSViewRepresentable {
    var url: URL
    var websiteDataStoreID: UUID?
    var reloadToken: Int

    func makeCoordinator() -> Coordinator { Coordinator(allowedURL: url) }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = websiteDataStoreID.map(WKWebsiteDataStore.init(forIdentifier:))
            ?? .nonPersistent()
        configuration.preferences.isElementFullscreenEnabled = true
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        context.coordinator.load(url, in: webView, reloadToken: reloadToken)
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        context.coordinator.allowedURL = url
        context.coordinator.load(url, in: webView, reloadToken: reloadToken)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var allowedURL: URL
        private var loadedURL: URL?
        private var loadedReloadToken: Int?

        init(allowedURL: URL) {
            self.allowedURL = allowedURL
        }

        func load(_ url: URL, in webView: WKWebView, reloadToken: Int) {
            guard loadedURL != url || loadedReloadToken != reloadToken else { return }
            loadedURL = url
            loadedReloadToken = reloadToken
            if url.isFileURL {
                webView.loadFileURL(url, allowingReadAccessTo: url.deletingLastPathComponent())
            } else {
                webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData))
            }
        }

        @MainActor func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
        ) {
            guard let destination = navigationAction.request.url else {
                decisionHandler(.cancel)
                return
            }
            decisionHandler(allows(destination) ? .allow : .cancel)
        }

        private func allows(_ destination: URL) -> Bool {
            if destination.absoluteString == "about:blank" { return true }
            if allowedURL.isFileURL {
                let root = allowedURL.deletingLastPathComponent().standardizedFileURL.path + "/"
                return destination.isFileURL
                    && destination.standardizedFileURL.path.hasPrefix(root)
            }
            return destination.scheme == allowedURL.scheme
                && destination.host == allowedURL.host
                && destination.port == allowedURL.port
        }
    }
}

private extension Color {
    init?(pluginHex value: String) {
        let raw = value.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
        guard raw.count == 6, let number = UInt64(raw, radix: 16) else { return nil }
        self.init(
            red: Double((number >> 16) & 0xff) / 255,
            green: Double((number >> 8) & 0xff) / 255,
            blue: Double(number & 0xff) / 255
        )
    }
}
