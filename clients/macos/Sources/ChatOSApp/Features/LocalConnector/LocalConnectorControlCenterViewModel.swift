import ChatOSCore
import Foundation

@MainActor
final class LocalConnectorControlCenterViewModel: ObservableObject {
    @Published var selectedTab: LocalConnectorControlTab = .connection {
        didSet {
            if oldValue == .approvals, selectedTab != .approvals {
                cancelApprovalLoad()
            }
        }
    }
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
    @Published var plugins: [LocalConnectorPlugin] = []
    @Published var browserExtensionPairedPluginIDs: Set<String> = []
    @Published private(set) var isStarting = false
    @Published var isLoading = false
    @Published private(set) var isPerformingAction = false
    @Published private(set) var isLoadingApprovals = false
    @Published private(set) var approvalLoadError: String?
    @Published private(set) var approvalOperationIDs: Set<String> = []
    @Published private(set) var approvalErrorMessages: [String: String] = [:]
    @Published var pluginOperationIDs: Set<String> = []
    @Published var pluginErrorMessages: [String: String] = [:]
    @Published var isInspectingCustomPlugin = false
    @Published var isInstallingCustomPlugin = false
    @Published var errorMessage: String?
    @Published var notice: String?

    let service: any LocalConnectorControlServicing
    private let approvalUseCase: LocalConnectorApprovalUseCase
    private var refreshGeneration: Int64 = 0
    var pluginRefreshGeneration: Int64 = 0
    private var approvalMonitorTask: Task<Void, Never>?
    private var approvalStreamTask: Task<Void, Never>?
    private var approvalEventStreamTask: Task<Void, Never>?
    private var approvalMonitoringState = LocalConnectorApprovalMonitoringState()
    var browserExtensionPairingTasks: [String: Task<Void, Never>] = [:]
    var browserExtensionPairingRequestIDs: [String: UUID] = [:]
    var selectedTabLoadTask: Task<Void, Never>?
    var selectedTabLoadGeneration: Int64 = 0
    private var statusRefreshTask: Task<Void, Never>?
    var pluginRefreshTask: Task<Void, Never>?
    private var actionTask: Task<Void, Never>?
    private var actionGeneration: Int64 = 0
    private var approvalActionState = LocalConnectorApprovalActionState()
    private var approvalActionTasks: [String: Task<Void, Never>] = [:]
    private var approvalLoadState = LocalConnectorApprovalLoadState()
    private var approvalLoadTask: Task<Void, Never>?
    var pluginActionTasks: [String: Task<Void, Never>] = [:]
    var pluginActionRequestIDs: [String: UUID] = [:]
    var customPluginTask: Task<Void, Never>?
    var lifecycleGeneration: UInt64 = 0
    private var signedOutSuspensionTask: Task<Void, Never>?
    private var signedOutSuspensionGeneration: UInt64 = 0
    private var servicePreparationTask: Task<Void, Never>?
    private var modelConfigurationChanged: (@MainActor @Sendable () -> Void)?
    var pluginConfigurationChanged: (@MainActor @Sendable () -> Void)?

    init(
        service: any LocalConnectorControlServicing
    ) {
        self.service = service
        self.approvalUseCase = LocalConnectorApprovalUseCase(service: service)
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
        approvalActionTasks.values.forEach { $0.cancel() }
        approvalLoadTask?.cancel()
        pluginActionTasks.values.forEach { $0.cancel() }
        customPluginTask?.cancel()
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

    func setModelConfigurationChangedHandler(_ handler: (@MainActor @Sendable () -> Void)?) { modelConfigurationChanged = handler }
    func setPluginConfigurationChangedHandler(_ handler: (@MainActor @Sendable () -> Void)?) { pluginConfigurationChanged = handler }
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
        let pendingApprovalActions = Array(approvalActionTasks.values)
        approvalActionTasks.values.forEach { $0.cancel() }
        approvalActionTasks = [:]
        approvalActionState.reset()
        approvalOperationIDs = []
        approvalErrorMessages = [:]
        let pendingApprovalLoad = approvalLoadTask
        cancelApprovalLoad()
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
        startSignedOutSuspension(
            after: [pendingStatusRefresh, pendingAction].compactMap { $0 }
                + pendingApprovalActions
                + [pendingApprovalLoad].compactMap { $0 }
        )
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
        guard selectedTab == .approvals else { return }
        selectedTabLoadGeneration += 1
        selectedTabLoadTask?.cancel()
        selectedTabLoadTask = nil
        pluginRefreshGeneration += 1
        pluginRefreshTask?.cancel()
        pluginRefreshTask = nil
        isLoading = false

        approvalLoadTask?.cancel()
        let token = approvalLoadState.begin()
        let lifecycle = lifecycleGeneration
        let approvalUseCase = approvalUseCase
        let pendingServicePreparation = servicePreparationTask
        let pendingSignedOutSuspension = signedOutSuspensionTask
        isLoadingApprovals = true
        approvalLoadError = nil
        approvalLoadTask = Task { [weak self] in
            var snapshot: LocalConnectorApprovalSnapshot?
            var failure: Error?
            do {
                await pendingServicePreparation?.value
                try Task.checkCancellation()
                await pendingSignedOutSuspension?.value
                try Task.checkCancellation()
                snapshot = try await approvalUseCase.loadSnapshot()
                try Task.checkCancellation()
            } catch is CancellationError {
                if Task.isCancelled { return }
                failure = CancellationError()
            } catch {
                failure = error
            }
            guard let self,
                  lifecycle == lifecycleGeneration,
                  selectedTab == .approvals,
                  approvalLoadState.accepts(token) else { return }
            if let snapshot {
                if approvalSettings != snapshot.settings {
                    approvalSettings = snapshot.settings
                }
                applyPendingApprovalsIfChanged(snapshot.pendingApprovals)
                approvalLoadError = nil
            } else if let failure {
                // Keep the last trusted settings and queue visible while the
                // native connector restarts or a transient IPC request fails.
                approvalLoadError = failure.localizedDescription
            }
            finishApprovalLoad(token)
        }
    }

    func startApprovalMonitoring() {
        let streamingService = service as? any LocalConnectorApprovalStreaming
        let session = approvalMonitoringState.start(
            hasStreamingService: streamingService != nil
        )
        if approvalStreamTask == nil, let streamingService {
            approvalStreamTask = Task { [weak self] in
                let stream = await streamingService.approvalSnapshots()
                for await approvals in stream {
                    guard let self,
                          !Task.isCancelled,
                          approvalMonitoringState.accepts(session) else { return }
                    self.applyPendingApprovalsIfChanged(approvals)
                }
            }
        }
        if approvalEventStreamTask == nil, let streamingService {
            approvalEventStreamTask = Task { [weak self] in
                let stream = await streamingService.approvalEvents()
                for await event in stream {
                    guard let self,
                          !Task.isCancelled,
                          approvalMonitoringState.accepts(session) else { return }
                    if self.latestApprovalEvent != event {
                        self.latestApprovalEvent = event
                    }
                }
            }
        }
        guard approvalMonitorTask == nil else { return }
        approvalMonitorTask = Task { [weak self] in
            if session.shouldRefreshImmediately {
                await self?.refreshPendingApprovalsSilently(session: session)
            }
            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: session.consistencyCheckInterval)
                } catch {
                    return
                }
                guard !Task.isCancelled else { return }
                await self?.refreshPendingApprovalsSilently(session: session)
            }
        }
    }

    func stopApprovalMonitoring() {
        approvalMonitoringState.stop()
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
            let nextSettings = try await self.approvalUseCase.updateDefaultMode(
                mode,
                riskAcknowledged: riskAcknowledged
            )
            try Task.checkCancellation()
            self.approvalSettings = nextSettings
        }
    }

    func resolveApproval(
        _ approval: LocalConnectorPendingApproval,
        decision: LocalConnectorApprovalDecision
    ) {
        guard let token = approvalActionState.begin(approvalID: approval.id) else { return }
        approvalOperationIDs = approvalActionState.activeApprovalIDs
        approvalErrorMessages.removeValue(forKey: approval.id)
        notice = nil
        let lifecycle = lifecycleGeneration
        let pendingServicePreparation = servicePreparationTask
        let pendingSignedOutSuspension = signedOutSuspensionTask
        let approvalUseCase = approvalUseCase
        approvalActionTasks[approval.id] = Task { [weak self] in
            var snapshot: LocalConnectorApprovalSnapshot?
            var failure: Error?
            do {
                await pendingServicePreparation?.value
                try Task.checkCancellation()
                await pendingSignedOutSuspension?.value
                try Task.checkCancellation()
                snapshot = try await approvalUseCase.resolve(
                    approval: approval,
                    decision: decision
                )
                try Task.checkCancellation()
            } catch is CancellationError {
                return
            } catch {
                failure = error
            }
            guard let self,
                  lifecycle == lifecycleGeneration,
                  approvalActionState.accepts(token) else { return }
            if let snapshot {
                applyPendingApprovalsIfChanged(snapshot.pendingApprovals)
                if approvalSettings != snapshot.settings {
                    approvalSettings = snapshot.settings
                }
                approvalErrorMessages.removeValue(forKey: approval.id)
                notice = "审批已处理。"
            } else if let failure {
                approvalErrorMessages[approval.id] = failure.localizedDescription
            }
            finishApprovalAction(token)
        }
    }

    func isResolvingApproval(_ approvalID: String) -> Bool {
        approvalOperationIDs.contains(approvalID)
    }

    func approvalError(for approvalID: String) -> String? {
        approvalErrorMessages[approvalID]
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
        try await availableModels(scope: .taskCreation)
    }

    func availableModels(scope: LocalConnectorModelSelectionScope) async throws -> [LocalConnectorModelConfig] {
        if let modelCatalog {
            return LocalConnectorModelSelectionPolicy.models(from: modelCatalog.items, scope: scope)
        }
        let lifecycle = lifecycleGeneration
        let catalog = try await service.fetchModelCatalog(refresh: false)
        try Task.checkCancellation()
        guard lifecycle == lifecycleGeneration else { throw CancellationError() }
        modelCatalog = catalog
        return LocalConnectorModelSelectionPolicy.models(from: catalog.items, scope: scope)
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
            self.modelCatalog = catalog; self.modelConfigurationChanged?()
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
            self.modelCatalog = nextCatalog; self.modelConfigurationChanged?()
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
            self.modelCatalog = nextCatalog; self.modelConfigurationChanged?()
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
            self.modelCatalog = nextCatalog; self.modelConfigurationChanged?()
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
            self.modelCatalog = nextCatalog; self.modelConfigurationChanged?()
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

    func clearMessages() {
        errorMessage = nil
        notice = nil
    }

    private func cancelApprovalLoad() {
        approvalLoadState.cancel()
        approvalLoadTask?.cancel()
        approvalLoadTask = nil
        isLoadingApprovals = false
        approvalLoadError = nil
    }

    private func finishApprovalLoad(_ token: LocalConnectorApprovalLoadToken) {
        guard approvalLoadState.finish(token) else { return }
        approvalLoadTask = nil
        isLoadingApprovals = false
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

    private func refreshPendingApprovalsSilently(
        session: LocalConnectorApprovalMonitoringSession
    ) async {
        do {
            let nextPendingApprovals = try await approvalUseCase.refreshPendingApprovals()
            try Task.checkCancellation()
            guard approvalMonitoringState.accepts(session) else { return }
            applyPendingApprovalsIfChanged(nextPendingApprovals)
        } catch {
            // The native connector may briefly restart while the app stays open.
            // Keep the last known queue and let the next polling cycle retry.
        }
    }

    private func applyPendingApprovalsIfChanged(
        _ nextPendingApprovals: [LocalConnectorPendingApproval]
    ) {
        let nextIDs = Set(nextPendingApprovals.map(\.id))
        let nextErrors = approvalErrorMessages.filter { nextIDs.contains($0.key) }
        if approvalErrorMessages != nextErrors {
            approvalErrorMessages = nextErrors
        }
        guard pendingApprovals != nextPendingApprovals else { return }
        pendingApprovals = nextPendingApprovals
    }

    private func finishApprovalAction(_ token: LocalConnectorApprovalActionToken) {
        guard approvalActionState.finish(token) else { return }
        approvalActionTasks.removeValue(forKey: token.approvalID)
        approvalOperationIDs = approvalActionState.activeApprovalIDs
    }

    func performAction(
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

}
