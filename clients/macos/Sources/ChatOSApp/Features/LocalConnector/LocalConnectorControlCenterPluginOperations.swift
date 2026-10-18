import ChatOSCore
import Foundation

@MainActor
extension LocalConnectorControlCenterViewModel {
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
            onSuccess: onSuccess,
            capabilitiesChanged: true
        ) {
            try await self.service.installPlugin(id: id)
        }
    }

    func inspectCustomMCP(
        _ draft: LocalConnectorCustomMCPDraft,
        completion: @escaping @MainActor (Result<LocalConnectorPluginInspection, Error>) -> Void
    ) {
        runCustomPluginInspection(completion: completion) {
            try await self.service.inspectCustomMCP(draft)
        }
    }

    func inspectLocalPlugin(
        atPath path: String,
        completion: @escaping @MainActor (Result<LocalConnectorPluginInspection, Error>) -> Void
    ) {
        runCustomPluginInspection(completion: completion) {
            try await self.service.inspectLocalPlugin(atPath: path)
        }
    }

    func inspectPluginSource(
        _ source: LocalConnectorPluginSourceDraft,
        completion: @escaping @MainActor (Result<LocalConnectorPluginInspection, Error>) -> Void
    ) {
        runCustomPluginInspection(completion: completion) {
            try await self.service.inspectPluginSource(source)
        }
    }

    func installCustomMCP(
        _ draft: LocalConnectorCustomMCPDraft,
        expectedFingerprint: String,
        completion: @escaping @MainActor (Result<Void, Error>) -> Void
    ) {
        runCustomPluginInstall(completion: completion) {
            try await self.service.installCustomMCP(
                draft,
                expectedFingerprint: expectedFingerprint
            )
        }
    }

    func installLocalPlugin(
        atPath path: String,
        expectedFingerprint: String,
        completion: @escaping @MainActor (Result<Void, Error>) -> Void
    ) {
        runCustomPluginInstall(completion: completion) {
            try await self.service.installLocalPlugin(
                atPath: path,
                expectedFingerprint: expectedFingerprint
            )
        }
    }

    func installPluginSource(
        _ source: LocalConnectorPluginSourceDraft,
        expectedFingerprint: String,
        completion: @escaping @MainActor (Result<Void, Error>) -> Void
    ) {
        runCustomPluginInstall(completion: completion) {
            try await self.service.installPluginSource(
                source,
                expectedFingerprint: expectedFingerprint
            )
        }
    }

    private func runCustomPluginInspection(
        completion: @escaping @MainActor (Result<LocalConnectorPluginInspection, Error>) -> Void,
        operation: @escaping @Sendable () async throws -> LocalConnectorPluginInspection
    ) {
        customPluginTask?.cancel()
        isInspectingCustomPlugin = true
        customPluginTask = Task { [weak self] in
            do {
                let inspection = try await operation()
                try Task.checkCancellation()
                self?.isInspectingCustomPlugin = false
                completion(.success(inspection))
            } catch is CancellationError {
                self?.isInspectingCustomPlugin = false
            } catch {
                self?.isInspectingCustomPlugin = false
                completion(.failure(error))
            }
        }
    }

    private func runCustomPluginInstall(
        completion: @escaping @MainActor (Result<Void, Error>) -> Void,
        operation: @escaping @Sendable () async throws -> Void
    ) {
        customPluginTask?.cancel()
        isInstallingCustomPlugin = true
        customPluginTask = Task { [weak self] in
            do {
                try await operation()
                try Task.checkCancellation()
                guard let self else { return }
                self.isInstallingCustomPlugin = false
                self.notice = "自定义 Plugin 已完成复检并安装到本机。"
                self.loadPlugins(forceRefresh: true)
                self.pluginConfigurationChanged?()
                completion(.success(()))
            } catch is CancellationError {
                self?.isInstallingCustomPlugin = false
            } catch {
                self?.isInstallingCustomPlugin = false
                completion(.failure(error))
            }
        }
    }

    func uninstallPlugin(id: String) {
        performPluginAction(
            id: id,
            successNotice: "Plugin 已卸载。",
            capabilitiesChanged: true
        ) {
            try await self.service.uninstallPlugin(id: id)
        }
    }

    func setPluginEnabled(id: String, enabled: Bool) {
        performAction(successNotice: enabled ? "Plugin 已启用。" : "Plugin 已停用。") { [weak self] in
            guard let self else { return }
            try await self.service.updatePluginEnabled(id: id, enabled: enabled)
            try Task.checkCancellation()
            let nextPlugins = try await self.service.fetchPlugins()
            try Task.checkCancellation()
            self.plugins = nextPlugins
            self.pluginConfigurationChanged?()
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

    private func performPluginAction(
        id: String,
        successNotice: String,
        onSuccess: (@MainActor () -> Void)? = nil,
        forceRefreshPluginsAfterOperation: Bool = false,
        capabilitiesChanged: Bool = false,
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
                if capabilitiesChanged {
                    pluginConfigurationChanged?()
                }
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
