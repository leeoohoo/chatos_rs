import ChatOSCore
import Foundation

@MainActor
final class LocalConnectorControlCenterViewModel: ObservableObject {
    @Published var selectedTab: LocalConnectorControlTab = .connection
    @Published private(set) var status: LocalConnectorStatus?
    @Published private(set) var runtimeSettings: LocalConnectorRuntimeSettings?
    @Published private(set) var systemPermissions: LocalConnectorSystemPermissions?
    @Published private(set) var commandHistory: [LocalConnectorCommandHistoryEntry] = []
    @Published private(set) var terminalResult: LocalConnectorTerminalResult?
    @Published private(set) var approvalSettings: LocalConnectorApprovalSettings?
    @Published private(set) var pendingApprovals: [LocalConnectorPendingApproval] = []
    @Published private(set) var latestApprovalEvent: LocalConnectorApprovalEvent?
    @Published private(set) var modelCatalog: LocalConnectorModelCatalog?
    @Published private(set) var modelProviders: [LocalConnectorModelProvider] = []
    @Published private(set) var sandboxBackends: [LocalConnectorSandboxBackend] = []
    @Published private(set) var sandboxSettings: LocalConnectorSandboxSettings?
    @Published private(set) var plugins: [LocalConnectorPlugin] = []
    @Published private(set) var isStarting = false
    @Published private(set) var isLoading = false
    @Published private(set) var isPerformingAction = false
    @Published private(set) var pluginOperationIDs: Set<String> = []
    @Published private(set) var errorMessage: String?
    @Published private(set) var notice: String?

    private let service: any LocalConnectorControlServicing
    private var refreshGeneration: Int64 = 0
    private var approvalMonitorTask: Task<Void, Never>?
    private var approvalStreamTask: Task<Void, Never>?
    private var approvalEventStreamTask: Task<Void, Never>?

    init(
        service: any LocalConnectorControlServicing
    ) {
        self.service = service
    }

    func activate(pairIfNeeded: Bool) {
        startApprovalMonitoring()
        isStarting = true
        refreshStatus(pairIfNeeded: pairIfNeeded)
    }

    func refreshStatus(pairIfNeeded: Bool = false) {
        refreshGeneration += 1
        let generation = refreshGeneration
        isLoading = true
        errorMessage = nil
        Task {
            do {
                let nextStatus = try await fetchStatusWithStartupRetry()
                guard generation == refreshGeneration else { return }
                if pairIfNeeded && !nextStatus.configured {
                    status = try await service.pairWithCurrentChatOSSession(
                        deviceName: Host.current().localizedName
                    )
                } else {
                    status = nextStatus
                }
            } catch {
                guard generation == refreshGeneration else { return }
                errorMessage = error.localizedDescription
            }
            guard generation == refreshGeneration else { return }
            isStarting = false
            isLoading = false
        }
    }

    func refreshSelectedTab() {
        switch selectedTab {
        case .connection:
            refreshStatus()
        case .plugins:
            loadPlugins()
        case .terminal:
            loadCommandHistory()
        case .models:
            loadModels(refresh: false)
        case .approvals:
            loadApprovals()
        case .runtime:
            loadRuntimeAndPermissions()
        case .sandbox:
            loadSandbox()
        }
    }

    func disconnect() {
        performAction(successNotice: "已断开这台设备与网关的配对。") {
            self.status = try await self.service.disconnect()
        }
    }

    func resetForSignedOut() {
        stopApprovalMonitoring()
        refreshGeneration += 1
        isStarting = false
        isLoading = false
        isPerformingAction = false
        pluginOperationIDs = []
        errorMessage = nil
        notice = nil
        status = nil
        approvalSettings = nil
        pendingApprovals = []
        latestApprovalEvent = nil
        plugins = []
        Task {
            _ = try? await service.disconnect()
        }
    }

    func reconnect() {
        performAction(successNotice: "设备已重新配对。") {
            self.status = try await self.service.pairWithCurrentChatOSSession(
                deviceName: Host.current().localizedName
            )
        }
    }

    func runTerminal(commandLine: String, workspaceID: String, cwd: String?) {
        guard !commandLine.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        performAction(successNotice: nil) {
            self.terminalResult = try await self.service.executeTerminal(
                workspaceID: workspaceID,
                commandLine: commandLine,
                cwd: cwd
            )
            self.commandHistory = try await self.service.fetchCommandHistory(limit: 50)
        }
    }

    func loadCommandHistory() {
        load {
            self.commandHistory = try await self.service.fetchCommandHistory(limit: 50)
        }
    }

    func clearCommandHistory() {
        performAction(successNotice: "终端历史已清空。") {
            try await self.service.clearCommandHistory()
            self.commandHistory = []
        }
    }

    func loadApprovals() {
        load {
            async let settings = self.service.fetchApprovalSettings()
            async let pending = self.service.fetchPendingApprovals()
            self.approvalSettings = try await settings
            self.pendingApprovals = try await pending
        }
    }

    func startApprovalMonitoring() {
        if approvalStreamTask == nil,
           let streamingService = service as? any LocalConnectorApprovalStreaming {
            approvalStreamTask = Task { [weak self] in
                let stream = await streamingService.approvalSnapshots()
                for await approvals in stream {
                    guard let self, !Task.isCancelled else { return }
                    self.pendingApprovals = approvals
                }
            }
        }
        if approvalEventStreamTask == nil,
           let streamingService = service as? any LocalConnectorApprovalStreaming {
            approvalEventStreamTask = Task { [weak self] in
                let stream = await streamingService.approvalEvents()
                for await event in stream {
                    guard let self, !Task.isCancelled else { return }
                    self.latestApprovalEvent = event
                }
            }
        }
        guard approvalMonitorTask == nil else { return }
        approvalMonitorTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refreshPendingApprovalsSilently()
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    func stopApprovalMonitoring() {
        approvalStreamTask?.cancel()
        approvalStreamTask = nil
        approvalEventStreamTask?.cancel()
        approvalEventStreamTask = nil
        approvalMonitorTask?.cancel()
        approvalMonitorTask = nil
    }

    func updateApprovalMode(
        _ mode: LocalConnectorApprovalMode,
        riskAcknowledged: Bool
    ) {
        performAction(successNotice: "默认审批策略已更新。") {
            self.approvalSettings = try await self.service.updateDefaultApprovalMode(
                mode,
                riskAcknowledged: riskAcknowledged
            )
        }
    }

    func resolveApproval(id: String, decision: String) {
        performAction(successNotice: "审批已处理。") {
            try await self.service.resolveApproval(id: id, decision: decision)
            self.pendingApprovals = try await self.service.fetchPendingApprovals()
            self.approvalSettings = try await self.service.fetchApprovalSettings()
        }
    }

    func loadRuntimeAndPermissions() {
        load {
            async let settings = self.service.fetchRuntimeSettings()
            async let permissions = self.service.fetchSystemPermissions()
            self.runtimeSettings = try await settings
            self.systemPermissions = try await permissions
        }
    }

    func updateDeveloperMode(_ enabled: Bool) {
        performAction(successNotice: enabled ? "开发者模式已开启。" : "开发者模式已关闭。") {
            self.runtimeSettings = try await self.service.updateDeveloperMode(enabled)
            self.status = try await self.service.fetchStatus()
        }
    }

    func requestPermission(id: String) {
        performAction(successNotice: "系统授权引导已打开；完成后可重新检测状态。") {
            self.systemPermissions = try await self.service.requestSystemPermission(id: id)
        }
    }

    func refreshPermissions() {
        performAction(successNotice: "系统权限状态已重新检测。") {
            self.systemPermissions = try await self.service.fetchSystemPermissions()
        }
    }

    func loadModels(refresh: Bool) {
        load {
            async let catalog = self.service.fetchModelCatalog(refresh: refresh)
            async let providers = self.service.fetchModelProviders()
            self.modelCatalog = try await catalog
            self.modelProviders = try await providers
        }
    }

    func saveModelConfiguration(
        settings: LocalConnectorModelSettings,
        updates: [String: LocalConnectorModelConfigUpdate]
    ) {
        performAction(successNotice: "AI 模型配置已保存。") {
            for (id, update) in updates {
                try await self.service.updateModelConfig(id: id, update: update)
            }
            try await self.service.updateModelSettings(settings)
            self.modelCatalog = try await self.service.fetchModelCatalog(refresh: false)
        }
    }

    func createModelProvider(_ draft: LocalConnectorModelProviderDraft) {
        performAction(successNotice: "供应商已添加，正在同步模型目录。") {
            try await self.service.createModelProvider(draft)
            self.modelProviders = try await self.service.fetchModelProviders()
            self.modelCatalog = try await self.service.fetchModelCatalog(refresh: true)
        }
    }

    func updateModelProvider(id: String, draft: LocalConnectorModelProviderDraft) {
        performAction(successNotice: "供应商配置已更新。") {
            try await self.service.updateModelProvider(id: id, draft: draft)
            self.modelProviders = try await self.service.fetchModelProviders()
            self.modelCatalog = try await self.service.fetchModelCatalog(refresh: true)
        }
    }

    func refreshModelProvider(id: String) {
        performAction(successNotice: "供应商模型目录已刷新。") {
            try await self.service.refreshModelProvider(id: id)
            self.modelProviders = try await self.service.fetchModelProviders()
            self.modelCatalog = try await self.service.fetchModelCatalog(refresh: true)
        }
    }

    func deleteModelProvider(id: String) {
        performAction(successNotice: "供应商及其导入模型已删除。") {
            try await self.service.deleteModelProvider(id: id)
            self.modelProviders = try await self.service.fetchModelProviders()
            self.modelCatalog = try await self.service.fetchModelCatalog(refresh: false)
        }
    }

    func loadSandbox() {
        load {
            async let backends = self.service.fetchSandboxBackends()
            async let settings = self.service.fetchSandboxSettings()
            self.sandboxBackends = try await backends
            self.sandboxSettings = try await settings
        }
    }

    func updateSandbox(
        enabled: Bool? = nil,
        permissionProfileID: String? = nil,
        approvalPolicy: String? = nil,
        approvalReviewer: String? = nil,
        networkAccess: String? = nil
    ) {
        performAction(successNotice: "权限策略已更新。") {
            self.sandboxSettings = try await self.service.updateSandboxSettings(
                enabled: enabled,
                permissionProfileID: permissionProfileID,
                approvalPolicy: approvalPolicy,
                approvalReviewer: approvalReviewer,
                networkAccess: networkAccess
            )
        }
    }

    func loadPlugins() {
        load {
            self.plugins = try await self.service.fetchPlugins()
        }
    }

    func installPlugin(id: String) {
        performPluginAction(id: id, successNotice: "Plugin 已完成校验并安装到本机。") {
            try await self.service.installPlugin(id: id)
        }
    }

    func uninstallPlugin(id: String) {
        performPluginAction(id: id, successNotice: "Plugin 已卸载。") {
            try await self.service.uninstallPlugin(id: id)
        }
    }

    func setPluginEnabled(id: String, enabled: Bool) {
        performAction(successNotice: enabled ? "Plugin 已启用。" : "Plugin 已停用。") {
            try await self.service.updatePluginEnabled(id: id, enabled: enabled)
            self.plugins = try await self.service.fetchPlugins()
        }
    }

    func requestPluginPermission(pluginID: String, permissionID: String) {
        performPluginAction(
            id: pluginID,
            successNotice: "已打开该 Plugin 的系统授权引导；完成授权后请重新检测。"
        ) {
            try await self.service.requestPluginPermission(
                pluginID: pluginID,
                permissionID: permissionID
            )
        }
    }

    func refreshPluginPermissions(id: String) {
        performPluginAction(id: id, successNotice: "Plugin 权限状态已重新检测。") {}
    }

    func clearMessages() {
        errorMessage = nil
        notice = nil
    }

    private func fetchStatusWithStartupRetry() async throws -> LocalConnectorStatus {
        var lastError: Error?
        for attempt in 0..<20 {
            do {
                return try await service.fetchStatus()
            } catch {
                lastError = error
                if attempt < 19 {
                    try await Task.sleep(for: .milliseconds(150))
                }
            }
        }
        throw lastError ?? URLError(.cannotConnectToHost)
    }

    private func load(_ operation: @escaping @MainActor () async throws -> Void) {
        let requestedTab = selectedTab
        isLoading = true
        errorMessage = nil
        Task {
            do {
                try await operation()
            } catch {
                if selectedTab == requestedTab {
                    errorMessage = error.localizedDescription
                }
            }
            if selectedTab == requestedTab {
                isLoading = false
            }
        }
    }

    private func refreshPendingApprovalsSilently() async {
        do {
            pendingApprovals = try await service.fetchPendingApprovals()
        } catch {
            // The native connector may briefly restart while the app stays open.
            // Keep the last known queue and let the next polling cycle retry.
        }
    }

    private func performAction(
        successNotice: String?,
        _ operation: @escaping @MainActor () async throws -> Void
    ) {
        isPerformingAction = true
        errorMessage = nil
        notice = nil
        Task {
            do {
                try await operation()
                notice = successNotice
            } catch {
                errorMessage = error.localizedDescription
            }
            isPerformingAction = false
        }
    }

    private func performPluginAction(
        id: String,
        successNotice: String,
        _ operation: @escaping @MainActor () async throws -> Void
    ) {
        pluginOperationIDs.insert(id)
        errorMessage = nil
        notice = nil
        Task {
            do {
                try await operation()
                plugins = try await service.fetchPlugins()
                notice = successNotice
            } catch {
                errorMessage = error.localizedDescription
            }
            pluginOperationIDs.remove(id)
        }
    }
}
