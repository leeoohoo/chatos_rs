import AppKit
import ChatOSAPI
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
    @State private var launchTask: Task<Void, Never>?
    @State private var taskWorkspace: PluginTaskWorkspaceContext?

    private var requiresContextSelection: Bool {
        application.contextScope == "project" || application.contextScope == "workspace"
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Button {
                    close()
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
                        launchTask?.cancel()
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
                        launch: launch,
                        projectContext: launchContext,
                        reloadToken: reloadToken,
                        model: model,
                        onOpenWorkspace: { taskWorkspace = $0 }
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
                            close()
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
        .onDisappear { launchTask?.cancel() }
        .sheet(item: $taskWorkspace) { workspace in
            PluginTaskWorkspaceView(
                workspace: workspace,
                service: model.taskRunnerHostService
            )
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
        launchTask?.cancel()
        launch = nil
        errorMessage = nil
        launchTask = Task {
            do {
                let result = try await model.launchPluginApplication(application, context: context)
                guard !Task.isCancelled else { return }
                launch = result
            } catch {
                guard !Task.isCancelled else { return }
                errorMessage = error.localizedDescription
            }
        }
    }

    private func close() {
        launchTask?.cancel()
        model.selection = .applications
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
    var launch: LocalConnectorPluginApplicationLaunch
    var projectContext: LocalConnectorPluginApplicationContext?
    var reloadToken: Int
    let model: AppModel
    let onOpenWorkspace: @MainActor (PluginTaskWorkspaceContext) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(
            launch: launch,
            projectContext: projectContext,
            model: model,
            onOpenWorkspace: onOpenWorkspace
        )
    }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = launch.websiteDataStoreID.map(WKWebsiteDataStore.init(forIdentifier:))
            ?? .nonPersistent()
        configuration.preferences.isElementFullscreenEnabled = true
        configuration.userContentController.addUserScript(WKUserScript(
            source: Coordinator.bootstrapScript,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        ))
        configuration.userContentController.add(context.coordinator, name: Coordinator.handlerName)
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        context.coordinator.load(launch.url, in: webView, reloadToken: reloadToken)
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        context.coordinator.update(
            launch: launch,
            projectContext: projectContext,
            onOpenWorkspace: onOpenWorkspace
        )
        context.coordinator.load(launch.url, in: webView, reloadToken: reloadToken)
    }

    static func dismantleNSView(_ webView: WKWebView, coordinator: Coordinator) {
        webView.configuration.userContentController.removeScriptMessageHandler(forName: Coordinator.handlerName)
        webView.navigationDelegate = nil
    }

    @MainActor final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
        static let handlerName = "chatosPluginBridge"
        static let bootstrapScript = #"""
        (() => {
          const pending = new Map();
          let ready = null;
          window.__chatosReceiveHostMessage = message => {
            if (!message || typeof message !== 'object') return;
            if (message.type === 'chatos.plugin_ui.ready') {
              ready = message;
              window.dispatchEvent(new CustomEvent('chatos:host-ready', { detail: message }));
              return;
            }
            if (message.type !== 'chatos.plugin_ui.response') return;
            const callback = pending.get(message.request_id);
            if (!callback) return;
            pending.delete(message.request_id);
            message.ok ? callback.resolve(message.result) : callback.reject(Object.assign(new Error(message.error_message || 'Host request failed'), { code: message.error_code }));
          };
          window.chatosHost = Object.freeze({
            capabilities: () => ready ? [...ready.capabilities] : [],
            request: (method, payload = {}) => new Promise((resolve, reject) => {
              if (!ready) return reject(new Error('ChatOS host bridge is not ready'));
              if (!ready.capabilities.includes(method)) return reject(new Error(`Host capability is not granted: ${method}`));
              const request_id = crypto.randomUUID();
              pending.set(request_id, { resolve, reject });
              window.webkit.messageHandlers.chatosPluginBridge.postMessage({
                type: 'chatos.plugin_ui.request', protocol_version: 1,
                adapter_session_id: ready.adapter_session_id,
                host_session_nonce: ready.host_session_nonce,
                request_id, method, payload
              });
            })
          });
        })();
        """#

        private(set) var launch: LocalConnectorPluginApplicationLaunch
        private var projectContext: LocalConnectorPluginApplicationContext?
        private let model: AppModel
        private var onOpenWorkspace: @MainActor (PluginTaskWorkspaceContext) -> Void
        var allowedURL: URL
        private var loadedURL: URL?
        private var loadedReloadToken: Int?
        private let adapterSessionID = UUID().uuidString.lowercased()
        private let hostSessionNonce = UUID().uuidString.lowercased() + UUID().uuidString.lowercased()
        private var webView: WKWebView?

        init(
            launch: LocalConnectorPluginApplicationLaunch,
            projectContext: LocalConnectorPluginApplicationContext?,
            model: AppModel,
            onOpenWorkspace: @escaping @MainActor (PluginTaskWorkspaceContext) -> Void
        ) {
            self.launch = launch
            self.projectContext = projectContext
            self.model = model
            self.onOpenWorkspace = onOpenWorkspace
            self.allowedURL = launch.url
        }

        func update(
            launch: LocalConnectorPluginApplicationLaunch,
            projectContext: LocalConnectorPluginApplicationContext?,
            onOpenWorkspace: @escaping @MainActor (PluginTaskWorkspaceContext) -> Void
        ) {
            self.launch = launch
            self.projectContext = projectContext
            self.onOpenWorkspace = onOpenWorkspace
            allowedURL = launch.url
        }

        func load(_ url: URL, in webView: WKWebView, reloadToken: Int) {
            guard loadedURL != url || loadedReloadToken != reloadToken else { return }
            loadedURL = url
            loadedReloadToken = reloadToken
            self.webView = webView
            if url.isFileURL {
                webView.loadFileURL(url, allowingReadAccessTo: url.deletingLastPathComponent())
            } else {
                webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData))
            }
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            sendReady(to: webView)
        }

        nonisolated func userContentController(
            _ userContentController: WKUserContentController,
            didReceive message: WKScriptMessage
        ) {
            Task { @MainActor [weak self] in self?.receive(message) }
        }

        private func receive(_ message: WKScriptMessage) {
            guard message.name == Self.handlerName,
                  message.frameInfo.isMainFrame,
                  message.webView === webView,
                  let currentURL = message.webView?.url,
                  allows(currentURL),
                  let value = message.body as? [String: Any],
                  let data = try? JSONSerialization.data(withJSONObject: value),
                  data.count <= 256 * 1_024,
                  value["type"] as? String == "chatos.plugin_ui.request",
                  (value["protocol_version"] as? NSNumber)?.intValue == 1,
                  value["adapter_session_id"] as? String == adapterSessionID,
                  value["host_session_nonce"] as? String == hostSessionNonce,
                  let requestID = validIdentifier(value["request_id"] as? String),
                  let method = value["method"] as? String,
                  launch.application.bridgeCapabilities.contains(method),
                  let payload = value["payload"] as? [String: Any] else {
                return
            }
            Task {
                do {
                    let result = try await handle(method: method, payload: payload)
                    respond(requestID: requestID, ok: true, result: result)
                } catch {
                    respond(
                        requestID: requestID,
                        ok: false,
                        result: [:],
                        errorCode: "host_request_failed",
                        errorMessage: error.localizedDescription
                    )
                }
            }
        }

        private func handle(method: String, payload: [String: Any]) async throws -> Any {
            switch method {
            case "host.context.read":
                return [
                    "projectId": projectContext?.projectID as Any,
                    "projectName": projectContext?.projectName as Any,
                    "capabilities": launch.application.bridgeCapabilities,
                ]
            case "task.batch.prepare":
                guard let projectContext else { throw ChatOSAPIError.invalidRequest("插件未绑定项目") }
                let data = try JSONSerialization.data(withJSONObject: payload)
                let request = try JSONDecoder().decode(PluginHostTaskBatchRequest.self, from: data)
                return try jsonObject(await model.preparePluginTaskBatch(request, launch: launch, context: projectContext))
            case "task.batch.status":
                guard let projectContext,
                      let taskIDs = payload["taskIds"] as? [String] else {
                    throw ChatOSAPIError.invalidRequest("任务状态请求无效")
                }
                return try jsonObject(await model.pluginTaskStatuses(taskIDs: taskIDs, context: projectContext))
            case "task.workspace.open":
                guard let projectContext, let projectID = projectContext.projectID,
                      let batchID = validIdentifier(payload["batchId"] as? String),
                      let taskIDs = payload["taskIds"] as? [String],
                      !taskIDs.isEmpty else {
                    throw ChatOSAPIError.invalidRequest(String(localized: "任务工作区请求无效"))
                }
                _ = try await model.pluginTaskStatuses(taskIDs: taskIDs, context: projectContext)
                onOpenWorkspace(.init(batchID: batchID, taskIDs: taskIDs, projectID: projectID))
                return ["opened": true]
            default:
                throw ChatOSAPIError.invalidRequest("宿主能力未实现")
            }
        }

        private func sendReady(to webView: WKWebView) {
            deliver([
                "type": "chatos.plugin_ui.ready",
                "protocol_version": 1,
                "adapter_session_id": adapterSessionID,
                "host_session_nonce": hostSessionNonce,
                "capabilities": launch.application.bridgeCapabilities,
            ], to: webView)
        }

        private func respond(
            requestID: String,
            ok: Bool,
            result: Any,
            errorCode: String? = nil,
            errorMessage: String? = nil
        ) {
            var response: [String: Any] = [
                "type": "chatos.plugin_ui.response", "protocol_version": 1,
                "adapter_session_id": adapterSessionID, "host_session_nonce": hostSessionNonce,
                "request_id": requestID, "ok": ok, "result": result,
            ]
            if let errorCode { response["error_code"] = errorCode }
            if let errorMessage { response["error_message"] = errorMessage }
            if let webView { deliver(response, to: webView) }
        }

        private func deliver(_ value: [String: Any], to webView: WKWebView) {
            guard let data = try? JSONSerialization.data(withJSONObject: value),
                  let json = String(data: data, encoding: .utf8) else { return }
            webView.evaluateJavaScript("window.__chatosReceiveHostMessage(\(json))")
        }

        private func jsonObject<T: Encodable>(_ value: T) throws -> Any {
            try JSONSerialization.jsonObject(with: JSONEncoder().encode(value))
        }

        private func validIdentifier(_ value: String?) -> String? {
            guard let value, !value.isEmpty, value.utf8.count <= 256,
                  value.unicodeScalars.allSatisfy({ !CharacterSet.controlCharacters.contains($0) }) else { return nil }
            return value
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

struct PluginTaskWorkspaceContext: Identifiable, Equatable {
    let batchID: String
    let taskIDs: [String]
    let projectID: String
    var id: String { batchID }
}

private struct PluginTaskWorkspaceView: View {
    let workspace: PluginTaskWorkspaceContext
    let service: ChatOSTaskRunnerHostService
    @Environment(\.dismiss) private var dismiss
    @State private var tasks: [PluginHostTaskReference] = []
    @State private var isLoading = true
    @State private var isStarting = false
    @State private var errorMessage: String?
    @State private var showStartConfirmation = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(.tint)
                VStack(alignment: .leading, spacing: 2) {
                    Text("任务工作区").font(.headline)
                    Text("Task Runner 是运行状态的唯一来源").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("刷新", systemImage: "arrow.clockwise") { Task { await load() } }
                    .disabled(isLoading || isStarting)
                Button("开始执行", systemImage: "play.fill") { showStartConfirmation = true }
                    .buttonStyle(.borderedProminent)
                    .disabled(tasks.isEmpty || isLoading || isStarting || !tasks.contains(where: { $0.status == "ready" }))
                Button("关闭", action: dismiss.callAsFunction)
            }
            .padding(16)
            Divider()
            if isLoading && tasks.isEmpty {
                ProgressView("正在读取真实任务状态…").frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if tasks.isEmpty {
                ContentUnavailableView(
                    "任务不可用",
                    systemImage: "exclamationmark.triangle",
                    description: Text(errorMessage ?? String(localized: "Task Runner 没有返回任务"))
                )
            } else {
                List(tasks) { task in
                    HStack(spacing: 12) {
                        Circle().fill(statusColor(task.status)).frame(width: 9, height: 9)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(task.title).font(.system(size: 13, weight: .medium))
                            Text(task.taskID).font(.caption2.monospaced()).foregroundStyle(.tertiary)
                        }
                        Spacer()
                        Text(statusTitle(task.status)).font(.caption.weight(.medium))
                            .foregroundStyle(statusColor(task.status))
                            .padding(.horizontal, 8).padding(.vertical, 4)
                            .background(statusColor(task.status).opacity(0.1), in: Capsule())
                    }
                    .padding(.vertical, 5)
                }
                .listStyle(.inset)
                if let errorMessage {
                    Text(errorMessage).font(.caption).foregroundStyle(.red).padding(10)
                }
            }
        }
        .frame(minWidth: 760, minHeight: 520)
        .task { await load() }
        .confirmationDialog("开始执行这个任务批次？", isPresented: $showStartConfirmation, titleVisibility: .visible) {
            Button("开始执行", role: .none) { Task { await start() } }
            Button("取消", role: .cancel) {}
        } message: {
            Text("Task Runner 会按依赖关系调度已就绪任务。关闭项目管理插件不会停止任务。")
        }
    }

    private func load() async {
        isLoading = true; errorMessage = nil
        do { tasks = try await service.taskStatuses(taskIDs: workspace.taskIDs, projectID: workspace.projectID) }
        catch { errorMessage = error.localizedDescription }
        isLoading = false
    }

    private func start() async {
        isStarting = true; errorMessage = nil
        do {
            let results = try await service.startBatch(taskIDs: workspace.taskIDs, projectID: workspace.projectID)
            let failures = results.filter { !$0.ok }
            if !failures.isEmpty { errorMessage = failures.compactMap(\.message).joined(separator: "；") }
            await load()
        } catch { errorMessage = error.localizedDescription }
        isStarting = false
    }

    private func statusTitle(_ status: String) -> String {
        switch status {
        case "draft": String(localized: "草稿")
        case "ready": String(localized: "待执行")
        case "queued": String(localized: "排队中")
        case "running": String(localized: "运行中")
        case "succeeded": String(localized: "已完成")
        case "failed": String(localized: "失败")
        case "blocked": String(localized: "阻塞")
        case "cancelled": String(localized: "已取消")
        case "archived": String(localized: "已归档")
        default: status
        }
    }

    private func statusColor(_ status: String) -> Color {
        switch status { case "succeeded": .green; case "failed", "cancelled": .red; case "blocked": .orange; case "running", "queued": .blue; default: .secondary }
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
