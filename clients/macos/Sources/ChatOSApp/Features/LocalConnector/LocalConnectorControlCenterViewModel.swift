import ChatOSCore
import Foundation

enum LocalConnectorApprovalMonitoringPolicy {
    static func consistencyCheckInterval(hasStreamingService: Bool) -> Duration {
        hasStreamingService ? .seconds(60) : .seconds(2)
    }
}

@MainActor
final class LocalConnectorControlCenterViewModel: ObservableObject {
    @Published var selectedTab: LocalConnectorControlTab = .connection
    @Published private(set) var status: LocalConnectorStatus?
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
    @Published private(set) var browserExtensionPairedPluginIDs: Set<String> = []
    @Published private(set) var isStarting = false
    @Published private(set) var isLoading = false
    @Published private(set) var isPerformingAction = false
    @Published private(set) var pluginOperationIDs: Set<String> = []
    @Published private(set) var pluginErrorMessages: [String: String] = [:]
    @Published private(set) var errorMessage: String?
    @Published private(set) var notice: String?

    private let service: any LocalConnectorControlServicing
    private var refreshGeneration: Int64 = 0
    private var pluginRefreshGeneration: Int64 = 0
    private var approvalMonitorTask: Task<Void, Never>?
    private var approvalStreamTask: Task<Void, Never>?
    private var approvalEventStreamTask: Task<Void, Never>?
    private var browserExtensionPairingTasks: [String: Task<Void, Never>] = [:]
    private var browserExtensionPairingRequestIDs: [String: UUID] = [:]
    private var selectedTabLoadTask: Task<Void, Never>?
    private var selectedTabLoadGeneration: Int64 = 0
    private var statusRefreshTask: Task<Void, Never>?
    private var pluginRefreshTask: Task<Void, Never>?
    private var actionTask: Task<Void, Never>?
    private var actionGeneration: Int64 = 0
    private var pluginActionTasks: [String: Task<Void, Never>] = [:]
    private var pluginActionRequestIDs: [String: UUID] = [:]
    private var lifecycleGeneration: UInt64 = 0
    private var signedOutSuspensionTask: Task<Void, Never>?
    private var signedOutSuspensionGeneration: UInt64 = 0
    private var servicePreparationTask: Task<Void, Never>?

    init(
        service: any LocalConnectorControlServicing
    ) {
        self.service = service
    }

    deinit {
        approvalStreamTask?.cancel()
        approvalEventStreamTask?.cancel()
        approvalMonitorTask?.cancel()
        browserExtensionPairingTasks.values.forEach { $0.cancel() }
        selectedTabLoadTask?.cancel()
        statusRefreshTask?.cancel()
        pluginRefreshTask?.cancel()
        actionTask?.cancel()
        pluginActionTasks.values.forEach { $0.cancel() }
    }

    func activate(
        pairIfNeeded: Bool,
        expectedOwnerUserID: String? = nil,
        onReady: (@MainActor (LocalConnectorStatus) -> Void)? = nil
    ) {
        startApprovalMonitoring()
        isStarting = true
        refreshStatus(
            pairIfNeeded: pairIfNeeded,
            expectedOwnerUserID: expectedOwnerUserID,
            onReady: onReady
        )
    }

    func setServicePreparationTask(_ task: Task<Void, Never>) {
        servicePreparationTask = task
    }

    func refreshStatus(
        pairIfNeeded: Bool = false,
        expectedOwnerUserID: String? = nil,
        onReady: (@MainActor (LocalConnectorStatus) -> Void)? = nil
    ) {
        statusRefreshTask?.cancel()
        refreshGeneration += 1
        let generation = refreshGeneration
        isLoading = true
        errorMessage = nil
        let service = service
        let pendingServicePreparation = servicePreparationTask
        let pendingSignedOutSuspension = signedOutSuspensionTask
        statusRefreshTask = Task { [weak self] in
            do {
                await pendingServicePreparation?.value
                try Task.checkCancellation()
                await pendingSignedOutSuspension?.value
                try Task.checkCancellation()
                // A restored ChatOS login does not guarantee that the independent Connector
                // credential is still accepted by the gateway. Refresh it on every authenticated
                // activation; the native service reuses the existing device/workspace when the
                // account and deployment are unchanged.
                let nextStatus = if pairIfNeeded {
                    try await service.pairWithCurrentChatOSSession(
                        deviceName: Host.current().localizedName
                    )
                } else {
                    try await Self.fetchStatusWithStartupRetry(service: service)
                }
                guard !Task.isCancelled,
                      let self,
                      generation == refreshGeneration else { return }
                let ownerMismatch = expectedOwnerUserID.map {
                    nextStatus.user?.id != $0
                } ?? false
                guard !ownerMismatch else {
                    throw CancellationError()
                }
                status = nextStatus
                onReady?(nextStatus)
            } catch is CancellationError {
                // A newer refresh or account transition owns the visible state.
            } catch {
                guard let self,
                      generation == refreshGeneration else { return }
                errorMessage = error.localizedDescription
            }
            guard let self,
                  generation == refreshGeneration else { return }
            isStarting = false
            isLoading = false
            statusRefreshTask = nil
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
            loadSystemPermissions()
        case .sandbox:
            loadSandbox()
        }
    }

    func disconnect() {
        performAction(successNotice: "已阻断服务端到本机的调用，本机数据和配置均已保留。") {
            let nextStatus = try await self.service.disconnect()
            try Task.checkCancellation()
            self.status = nextStatus
        }
    }

    func resetForSignedOut() {
        lifecycleGeneration &+= 1
        stopApprovalMonitoring()
        let pendingStatusRefresh = statusRefreshTask
        statusRefreshTask?.cancel()
        statusRefreshTask = nil
        pluginRefreshTask?.cancel()
        pluginRefreshTask = nil
        actionGeneration += 1
        let pendingAction = actionTask
        actionTask?.cancel()
        actionTask = nil
        pluginActionTasks.values.forEach { $0.cancel() }
        pluginActionTasks = [:]
        pluginActionRequestIDs = [:]
        refreshGeneration += 1
        pluginRefreshGeneration += 1
        browserExtensionPairingTasks.values.forEach { $0.cancel() }
        browserExtensionPairingTasks = [:]
        browserExtensionPairingRequestIDs = [:]
        selectedTabLoadGeneration += 1
        selectedTabLoadTask?.cancel()
        selectedTabLoadTask = nil
        isStarting = false
        isLoading = false
        isPerformingAction = false
        pluginOperationIDs = []
        pluginErrorMessages = [:]
        errorMessage = nil
        notice = nil
        status = nil
        approvalSettings = nil
        pendingApprovals = []
        latestApprovalEvent = nil
        plugins = []
        browserExtensionPairedPluginIDs = []
        startSignedOutSuspension(after: [pendingStatusRefresh, pendingAction].compactMap { $0 })
    }

    private func startSignedOutSuspension(after pendingTasks: [Task<Void, Never>]) {
        signedOutSuspensionGeneration &+= 1
        let generation = signedOutSuspensionGeneration
        let previousSuspension = signedOutSuspensionTask
        let service = service
        signedOutSuspensionTask = Task { [weak self] in
            await previousSuspension?.value
            for task in pendingTasks {
                await task.value
            }
            // A missing/expired login session is not an explicit request to erase this
            // Mac's persisted project and workspace access state.
            await service.suspendForSignedOut()
            guard let self,
                  signedOutSuspensionGeneration == generation else { return }
            signedOutSuspensionTask = nil
        }
    }

    func reconnect() {
        performAction(successNotice: "服务端到本机的调用通道已恢复。") {
            let nextStatus: LocalConnectorStatus
            if self.status?.configured == true {
                nextStatus = try await self.service.resumeServerAccess()
            } else {
                nextStatus = try await self.service.pairWithCurrentChatOSSession(
                    deviceName: Host.current().localizedName
                )
            }
            try Task.checkCancellation()
            self.status = nextStatus
        }
    }

    func runTerminal(commandLine: String, workspaceID: String, cwd: String?) {
        guard !commandLine.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        performAction(successNotice: nil) {
            let nextResult = try await self.service.executeTerminal(
                workspaceID: workspaceID,
                commandLine: commandLine,
                cwd: cwd
            )
            try Task.checkCancellation()
            let nextHistory = try await self.service.fetchCommandHistory(limit: 50)
            try Task.checkCancellation()
            self.terminalResult = nextResult
            self.commandHistory = nextHistory
        }
    }

    func loadCommandHistory() {
        load {
            let history = try await self.service.fetchCommandHistory(limit: 50)
            try Task.checkCancellation()
            self.commandHistory = history
        }
    }

    func clearCommandHistory() {
        performAction(successNotice: "终端历史已清空。") {
            try await self.service.clearCommandHistory()
            try Task.checkCancellation()
            self.commandHistory = []
        }
    }

    func loadApprovals() {
        load {
            async let settings = self.service.fetchApprovalSettings()
            async let pending = self.service.fetchPendingApprovals()
            let nextSettings = try await settings
            let nextPendingApprovals = try await pending
            try Task.checkCancellation()
            if self.approvalSettings != nextSettings {
                self.approvalSettings = nextSettings
            }
            self.applyPendingApprovalsIfChanged(nextPendingApprovals)
        }
    }

    func startApprovalMonitoring() {
        let lifecycle = lifecycleGeneration
        let streamingService = service as? any LocalConnectorApprovalStreaming
        let hasStreamingService = streamingService != nil
        if approvalStreamTask == nil, let streamingService {
            approvalStreamTask = Task { [weak self] in
                let stream = await streamingService.approvalSnapshots()
                for await approvals in stream {
                    guard let self, !Task.isCancelled else { return }
                    self.applyPendingApprovalsIfChanged(approvals)
                }
            }
        }
        if approvalEventStreamTask == nil, let streamingService {
            approvalEventStreamTask = Task { [weak self] in
                let stream = await streamingService.approvalEvents()
                for await event in stream {
                    guard let self, !Task.isCancelled else { return }
                    if self.latestApprovalEvent != event {
                        self.latestApprovalEvent = event
                    }
                }
            }
        }
        guard approvalMonitorTask == nil else { return }
        let interval = LocalConnectorApprovalMonitoringPolicy.consistencyCheckInterval(
            hasStreamingService: hasStreamingService
        )
        approvalMonitorTask = Task { [weak self] in
            if !hasStreamingService {
                await self?.refreshPendingApprovalsSilently(lifecycle: lifecycle)
            }
            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: interval)
                } catch {
                    return
                }
                guard !Task.isCancelled else { return }
                await self?.refreshPendingApprovalsSilently(lifecycle: lifecycle)
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
            let nextSettings = try await self.service.updateDefaultApprovalMode(
                mode,
                riskAcknowledged: riskAcknowledged
            )
            try Task.checkCancellation()
            self.approvalSettings = nextSettings
        }
    }

    func resolveApproval(id: String, decision: String) {
        performAction(successNotice: "审批已处理。") {
            try await self.service.resolveApproval(id: id, decision: decision)
            try Task.checkCancellation()
            async let pending = self.service.fetchPendingApprovals()
            async let settings = self.service.fetchApprovalSettings()
            let nextPendingApprovals = try await pending
            let nextSettings = try await settings
            try Task.checkCancellation()
            self.applyPendingApprovalsIfChanged(nextPendingApprovals)
            if self.approvalSettings != nextSettings {
                self.approvalSettings = nextSettings
            }
        }
    }

    func loadSystemPermissions() {
        load {
            let permissions = try await self.service.fetchSystemPermissions()
            try Task.checkCancellation()
            self.systemPermissions = permissions
        }
    }

    func requestPermission(id: String) {
        performAction(successNotice: "系统授权引导已打开；完成后可重新检测状态。") {
            let permissions = try await self.service.requestSystemPermission(id: id)
            try Task.checkCancellation()
            self.systemPermissions = permissions
        }
    }

    func refreshPermissions() {
        performAction(successNotice: "系统权限状态已重新检测。") {
            let permissions = try await self.service.fetchSystemPermissions()
            try Task.checkCancellation()
            self.systemPermissions = permissions
        }
    }

    func loadModels(refresh: Bool) {
        load {
            async let catalog = self.service.fetchModelCatalog(refresh: refresh)
            async let providers = self.service.fetchModelProviders()
            let nextCatalog = try await catalog
            let nextProviders = try await providers
            try Task.checkCancellation()
            self.modelCatalog = nextCatalog
            self.modelProviders = nextProviders
        }
    }

    func availableTaskModels() async throws -> [LocalConnectorModelConfig] {
        if let modelCatalog {
            return modelCatalog.items.filter {
                $0.enabled && $0.taskEnabled && $0.hasAPIKey
            }
        }
        let lifecycle = lifecycleGeneration
        let catalog = try await service.fetchModelCatalog(refresh: false)
        try Task.checkCancellation()
        guard lifecycle == lifecycleGeneration else { throw CancellationError() }
        modelCatalog = catalog
        return catalog.items.filter {
            $0.enabled && $0.taskEnabled && $0.hasAPIKey
        }
    }

    func saveModelConfiguration(
        settings: LocalConnectorModelSettings,
        updates: [String: LocalConnectorModelConfigUpdate]
    ) {
        performAction(successNotice: "AI 模型配置已保存。") {
            for (id, update) in updates {
                try await self.service.updateModelConfig(id: id, update: update)
                try Task.checkCancellation()
            }
            try await self.service.updateModelSettings(settings)
            try Task.checkCancellation()
            let catalog = try await self.service.fetchModelCatalog(refresh: false)
            try Task.checkCancellation()
            self.modelCatalog = catalog
        }
    }

    func createModelProvider(_ draft: LocalConnectorModelProviderDraft) {
        performAction(successNotice: "供应商已添加，正在同步模型目录。") {
            try await self.service.createModelProvider(draft)
            try Task.checkCancellation()
            async let providers = self.service.fetchModelProviders()
            async let catalog = self.service.fetchModelCatalog(refresh: true)
            let nextProviders = try await providers
            let nextCatalog = try await catalog
            try Task.checkCancellation()
            self.modelProviders = nextProviders
            self.modelCatalog = nextCatalog
        }
    }

    func updateModelProvider(id: String, draft: LocalConnectorModelProviderDraft) {
        performAction(successNotice: "供应商配置已更新。") {
            try await self.service.updateModelProvider(id: id, draft: draft)
            try Task.checkCancellation()
            async let providers = self.service.fetchModelProviders()
            async let catalog = self.service.fetchModelCatalog(refresh: true)
            let nextProviders = try await providers
            let nextCatalog = try await catalog
            try Task.checkCancellation()
            self.modelProviders = nextProviders
            self.modelCatalog = nextCatalog
        }
    }

    func refreshModelProvider(id: String) {
        performAction(successNotice: "供应商模型目录已刷新。") {
            try await self.service.refreshModelProvider(id: id)
            try Task.checkCancellation()
            async let providers = self.service.fetchModelProviders()
            async let catalog = self.service.fetchModelCatalog(refresh: true)
            let nextProviders = try await providers
            let nextCatalog = try await catalog
            try Task.checkCancellation()
            self.modelProviders = nextProviders
            self.modelCatalog = nextCatalog
        }
    }

    func deleteModelProvider(id: String) {
        performAction(successNotice: "供应商及其导入模型已删除。") {
            try await self.service.deleteModelProvider(id: id)
            try Task.checkCancellation()
            async let providers = self.service.fetchModelProviders()
            async let catalog = self.service.fetchModelCatalog(refresh: false)
            let nextProviders = try await providers
            let nextCatalog = try await catalog
            try Task.checkCancellation()
            self.modelProviders = nextProviders
            self.modelCatalog = nextCatalog
        }
    }

    func loadSandbox() {
        load {
            async let backends = self.service.fetchSandboxBackends()
            async let settings = self.service.fetchSandboxSettings()
            let nextBackends = try await backends
            let nextSettings = try await settings
            try Task.checkCancellation()
            self.sandboxBackends = nextBackends
            self.sandboxSettings = nextSettings
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
            let settings = try await self.service.updateSandboxSettings(
                enabled: enabled,
                permissionProfileID: permissionProfileID,
                approvalPolicy: approvalPolicy,
                approvalReviewer: approvalReviewer,
                networkAccess: networkAccess
            )
            try Task.checkCancellation()
            self.sandboxSettings = settings
        }
    }

    func loadPlugins(forceRefresh: Bool = false) {
        selectedTabLoadGeneration += 1
        selectedTabLoadTask?.cancel()
        selectedTabLoadTask = nil
        pluginRefreshTask?.cancel()
        pluginRefreshGeneration += 1
        let generation = pluginRefreshGeneration
        let requestedTab = selectedTab
        isLoading = true
        errorMessage = nil
        let service = service
        pluginRefreshTask = Task { [weak self] in
            do {
                let nextPlugins = try await service.fetchPlugins(refresh: forceRefresh)
                guard !Task.isCancelled,
                      let self,
                      generation == pluginRefreshGeneration else { return }
                plugins = nextPlugins
            } catch is CancellationError {
                return
            } catch {
                guard let self,
                      generation == pluginRefreshGeneration else { return }
                if selectedTab == requestedTab {
                    errorMessage = error.localizedDescription
                }
            }
            guard let self,
                  generation == pluginRefreshGeneration else { return }
            if selectedTab == requestedTab {
                isLoading = false
            }
            pluginRefreshTask = nil
        }
    }

    func installPlugin(
        id: String,
        onSuccess: (@MainActor () -> Void)? = nil
    ) {
        performPluginAction(
            id: id,
            successNotice: "Plugin 已完成校验并安装到本机。",
            onSuccess: onSuccess
        ) {
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
            try Task.checkCancellation()
            let nextPlugins = try await self.service.fetchPlugins()
            try Task.checkCancellation()
            self.plugins = nextPlugins
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
        performPluginAction(
            id: id,
            successNotice: "Plugin 权限状态已重新检测。",
            forceRefreshPluginsAfterOperation: true
        ) {}
    }

    func startBrowserExtensionGuide(
        pluginID: String,
        onReady: @escaping @MainActor () -> Void
    ) {
        performPluginAction(
            id: pluginID,
            successNotice: "Chrome 一次性连接服务已启动。",
            onSuccess: onReady
        ) {
            try await self.service.startBrowserExtensionPairing(pluginID: pluginID)
        }
    }

    func refreshBrowserExtensionPairingStatus(pluginID: String) {
        browserExtensionPairingTasks[pluginID]?.cancel()
        let requestID = UUID()
        browserExtensionPairingRequestIDs[pluginID] = requestID
        let service = service
        browserExtensionPairingTasks[pluginID] = Task { [weak self] in
            do {
                let isPaired = try await service.isBrowserExtensionPaired(pluginID: pluginID)
                guard !Task.isCancelled,
                      let self,
                      browserExtensionPairingRequestIDs[pluginID] == requestID else { return }
                if isPaired {
                    browserExtensionPairedPluginIDs.insert(pluginID)
                } else {
                    browserExtensionPairedPluginIDs.remove(pluginID)
                }
            } catch {
                // A connector restart or transient IPC failure is not evidence that pairing
                // disappeared. Preserve the last trusted state and let the next refresh retry.
                guard !Task.isCancelled else { return }
            }
            guard let self,
                  browserExtensionPairingRequestIDs[pluginID] == requestID else { return }
            browserExtensionPairingTasks[pluginID] = nil
            browserExtensionPairingRequestIDs[pluginID] = nil
        }
    }

    func clearMessages() {
        errorMessage = nil
        notice = nil
    }

    nonisolated private static func fetchStatusWithStartupRetry(
        service: any LocalConnectorControlServicing
    ) async throws -> LocalConnectorStatus {
        var lastError: Error?
        for attempt in 0..<20 {
            do {
                return try await service.fetchStatus()
            } catch {
                try Task.checkCancellation()
                lastError = error
                if attempt < 19 {
                    try await Task.sleep(for: .milliseconds(150))
                }
            }
        }
        throw lastError ?? URLError(.cannotConnectToHost)
    }

    private func load(_ operation: @escaping @MainActor () async throws -> Void) {
        pluginRefreshGeneration += 1
        pluginRefreshTask?.cancel()
        pluginRefreshTask = nil
        selectedTabLoadGeneration += 1
        let generation = selectedTabLoadGeneration
        selectedTabLoadTask?.cancel()
        let requestedTab = selectedTab
        isLoading = true
        errorMessage = nil
        selectedTabLoadTask = Task { [weak self] in
            do {
                try await operation()
            } catch is CancellationError {
                return
            } catch {
                guard let self,
                      generation == selectedTabLoadGeneration else { return }
                if selectedTab == requestedTab {
                    errorMessage = error.localizedDescription
                }
            }
            guard let self,
                  generation == selectedTabLoadGeneration else { return }
            if selectedTab == requestedTab {
                isLoading = false
            }
            selectedTabLoadTask = nil
        }
    }

    private func refreshPendingApprovalsSilently(lifecycle: UInt64) async {
        do {
            let nextPendingApprovals = try await service.fetchPendingApprovals()
            try Task.checkCancellation()
            guard lifecycle == lifecycleGeneration else { return }
            applyPendingApprovalsIfChanged(nextPendingApprovals)
        } catch {
            // The native connector may briefly restart while the app stays open.
            // Keep the last known queue and let the next polling cycle retry.
        }
    }

    private func applyPendingApprovalsIfChanged(
        _ nextPendingApprovals: [LocalConnectorPendingApproval]
    ) {
        guard pendingApprovals != nextPendingApprovals else { return }
        pendingApprovals = nextPendingApprovals
    }

    private func performAction(
        successNotice: String?,
        _ operation: @escaping @MainActor () async throws -> Void
    ) {
        actionGeneration += 1
        let generation = actionGeneration
        let lifecycle = lifecycleGeneration
        actionTask?.cancel()
        isPerformingAction = true
        errorMessage = nil
        notice = nil
        let pendingServicePreparation = servicePreparationTask
        let pendingSignedOutSuspension = signedOutSuspensionTask
        actionTask = Task { [weak self] in
            do {
                await pendingServicePreparation?.value
                try Task.checkCancellation()
                await pendingSignedOutSuspension?.value
                try Task.checkCancellation()
                try await operation()
                try Task.checkCancellation()
                guard let self,
                      generation == actionGeneration,
                      lifecycle == lifecycleGeneration else { return }
                notice = successNotice
            } catch is CancellationError {
                return
            } catch {
                guard let self,
                      generation == actionGeneration,
                      lifecycle == lifecycleGeneration else { return }
                errorMessage = error.localizedDescription
            }
            guard let self,
                  generation == actionGeneration,
                  lifecycle == lifecycleGeneration else { return }
            isPerformingAction = false
            actionTask = nil
        }
    }

    private func performPluginAction(
        id: String,
        successNotice: String,
        onSuccess: (@MainActor () -> Void)? = nil,
        forceRefreshPluginsAfterOperation: Bool = false,
        _ operation: @escaping @MainActor () async throws -> Void
    ) {
        pluginActionTasks[id]?.cancel()
        let requestID = UUID()
        let lifecycle = lifecycleGeneration
        pluginActionRequestIDs[id] = requestID
        pluginOperationIDs.insert(id)
        pluginErrorMessages[id] = nil
        errorMessage = nil
        notice = nil
        let service = service
        pluginActionTasks[id] = Task { [weak self] in
            do {
                try await operation()
                try Task.checkCancellation()
                let nextPlugins = try await service.fetchPlugins(
                    refresh: forceRefreshPluginsAfterOperation
                )
                try Task.checkCancellation()
                guard let self,
                      lifecycle == lifecycleGeneration,
                      pluginActionRequestIDs[id] == requestID else { return }
                plugins = nextPlugins
                notice = successNotice
                onSuccess?()
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled,
                      let self,
                      lifecycle == lifecycleGeneration,
                      pluginActionRequestIDs[id] == requestID else { return }
                errorMessage = error.localizedDescription
                pluginErrorMessages[id] = error.localizedDescription
            }
            guard let self,
                  lifecycle == lifecycleGeneration,
                  pluginActionRequestIDs[id] == requestID else { return }
            pluginOperationIDs.remove(id)
            pluginActionTasks[id] = nil
            pluginActionRequestIDs[id] = nil
        }
    }
}
