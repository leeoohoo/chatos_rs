import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    private static var pluginSourceCacheTTL: TimeInterval { 5 }

    public func fetchPluginApplications() async throws -> [LocalConnectorPluginApplication] {
        let records = state.installedPluginRecords ?? [:]
        var applications: [LocalConnectorPluginApplication] = []
        for record in records.values where state.pluginPreferences[record.pluginID] ?? true {
            guard await pluginIntegritySnapshots.status(record: record) == .verified,
                  let manifest = try? installedPluginManifest(record: record) else { continue }
            applications.append(contentsOf: manifest.ui.compactMap { contribution in
                guard contribution.surface == "workbench" else { return nil }
                return pluginApplication(
                    record: record,
                    manifest: manifest,
                    contribution: contribution
                )
            })
        }
        return applications.sorted {
            if $0.displayName != $1.displayName {
                return $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
            }
            return $0.id < $1.id
        }
    }

    public func launchPluginApplication(
        pluginID: String,
        componentKey: String,
        context: LocalConnectorPluginApplicationContext?,
        expectedOwnerUserID: String
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        guard state.user?.id == expectedOwnerUserID else { throw CancellationError() }
        guard state.pluginPreferences[pluginID] ?? true else {
            throw NativeConnectorError.pluginInstallation("Plugin 已停用")
        }
        guard let record = state.installedPluginRecords?[pluginID] else {
            throw NativeConnectorError.pluginInstallation("Plugin 尚未安装")
        }
        try Self.verifyTrustedPluginInstallation(record)
        let manifest = try installedPluginManifest(record: record)
        guard let contribution = manifest.ui.first(where: {
            $0.componentKey == componentKey && $0.surface == "workbench"
        }) else {
            throw NativeConnectorError.pluginInstallation("Plugin 没有这个应用页面")
        }
        let application = pluginApplication(
            record: record,
            manifest: manifest,
            contribution: contribution
        )
        let resolvedPath: NativeResolvedProjectPath?
        if let root = context?.projectRoot?.pluginContextValue {
            resolvedPath = try resolveProjectPath(root)
        } else {
            resolvedPath = nil
        }
        guard let ownerUserID = state.user?.id, let deviceID = state.deviceID else {
            throw NativeConnectorError.notPaired
        }
        return try await pluginApplicationRuntime.launch(
            record: record,
            manifest: manifest,
            contribution: contribution,
            runtimeRootURL: pluginRuntimeRootURL,
            application: application,
            hostContext: .init(
                ownerUserID: ownerUserID,
                deviceID: deviceID,
                workspaceID: resolvedPath?.workspace.id,
                workspaceRoot: resolvedPath?.absoluteURL,
                projectID: context?.projectID?.pluginContextValue,
                projectName: context?.projectName?.pluginContextValue
            )
        )
    }

    public func fetchPlugins() async throws -> [LocalConnectorPlugin] {
        try await fetchPlugins(refresh: false)
    }

    public func fetchPlugins(refresh: Bool) async throws -> [LocalConnectorPlugin] {
        do {
            return try await fetchPluginsWithCurrentPairing(forceRefresh: refresh)
        } catch NativeConnectorError.notPaired {
            // Switching between local and deployed gateways can leave only the connector token
            // stale. Re-pair it once through the still-valid primary ChatOS session.
            do {
                _ = try await pairWithCurrentChatOSSession(deviceName: Host.current().localizedName)
                return try await fetchPluginsWithCurrentPairing(forceRefresh: refresh)
            } catch {
                let installed = await locallyInstalledPlugins(forceRefresh: refresh)
                if !installed.isEmpty { return installed }
                throw error
            }
        } catch {
            let installed = await locallyInstalledPlugins(forceRefresh: refresh)
            if !installed.isEmpty { return installed }
            throw error
        }
    }

    /// Keep the installed catalog usable while the Marketplace is temporarily unavailable.
    /// Previously the presence of one Custom MCP made the error fallback return only custom
    /// records, which made every installed Marketplace Plugin appear to have been deleted.
    private func locallyInstalledPlugins(
        forceRefresh: Bool
    ) async -> [LocalConnectorPlugin] {
        let marketplace = await locallyInstalledMarketplacePlugins(forceRefresh: forceRefresh)
        let custom = await customInstalledPlugins(forceRefresh: forceRefresh)
        return (marketplace + custom).sorted {
            let comparison = $0.displayName.localizedStandardCompare($1.displayName)
            return comparison == .orderedSame ? $0.pluginID < $1.pluginID : comparison == .orderedAscending
        }
    }

    private func locallyInstalledMarketplacePlugins(
        forceRefresh: Bool
    ) async -> [LocalConnectorPlugin] {
        let records = (state.installedPluginRecords ?? [:]).values
            .filter { $0.source == nil }
            .sorted { $0.pluginID < $1.pluginID }
        var result: [LocalConnectorPlugin] = []
        result.reserveCapacity(records.count)
        for record in records {
            var issue = await pluginInstallationIssue(
                installed: true,
                record: record,
                forceRefresh: forceRefresh
            )
            var manifest: NativePluginManifest?
            if issue == nil {
                do {
                    manifest = try installedPluginManifest(record: record)
                } catch {
                    issue = .integrityCheckFailed
                }
            }
            let permissions: [LocalConnectorPluginPermission]
            if let manifest {
                permissions = (try? await pluginPermissionSnapshots.permissions(
                    record: record,
                    manifest: manifest,
                    forceRefresh: forceRefresh
                )) ?? []
            } else {
                permissions = []
            }
            let packageName = manifest?.name
                ?? record.pluginKey?.split(separator: "@", maxSplits: 1).first.map(String.init)
            result.append(.init(
                pluginID: record.pluginID,
                packageName: packageName,
                pluginKey: record.pluginKey,
                displayName: manifest?.interface?.displayName ?? packageName ?? record.pluginID,
                description: manifest?.description ?? "",
                category: "Plugin",
                publisher: "ChatOS",
                latestVersion: record.version,
                installedVersion: record.version,
                installed: true,
                updateAvailable: false,
                installAvailable: false,
                enabled: state.pluginPreferences[record.pluginID] ?? true,
                hasUI: manifest.map { !$0.ui.isEmpty },
                permissions: permissions,
                installationIssue: issue,
                source: .marketplace
            ))
        }
        return result
    }

    public func fetchPluginHookHistory(
        pluginID: String,
        beforeUpdatedAtUnixMs: Int64?,
        beforeInvocationID: String?,
        limit: Int
    ) async throws -> LocalConnectorPluginHookHistoryPage {
        guard let ownerUserID = state.user?.id,
              let host = localAgentHostClient else {
            throw NativeConnectorError.notPaired
        }
        guard (1...100).contains(limit),
              (beforeUpdatedAtUnixMs == nil) == (beforeInvocationID == nil) else {
            throw NativeConnectorError.invalidResponse("Hook 运行历史查询参数无效")
        }
        return try await NativeLocalAgentHookBundleClient(host: host).listInvocations(
            ownerUserID: ownerUserID,
            pluginID: pluginID,
            beforeUpdatedAtUnixMs: beforeUpdatedAtUnixMs,
            beforeInvocationID: beforeInvocationID,
            limit: limit
        )
    }

    private func fetchPluginsWithCurrentPairing(
        forceRefresh: Bool
    ) async throws -> [LocalConnectorPlugin] {
        let sources = try await pluginSources(forceRefresh: forceRefresh)
        var stateChanged = reconcileInstalledPluginIdentities(with: sources.items)
        await repairLegacyPluginIntegrityRecordsIfNeeded(sources: sources.items)
        if reconcileInstalledPluginIdentities(with: sources.items) {
            stateChanged = true
        }
        if stateChanged {
            try stateStore.save(state)
            try? await sendPluginInstallationStatus()
        }
        var plugins: [LocalConnectorPlugin] = []
        plugins.reserveCapacity(sources.items.count)
        for source in sources.items {
            let id = source.catalog.id
            let installedRecord = state.installedPluginRecords?[id]
            let installed = installedRecord != nil || state.installedPluginIDs.contains(id)
            var installationIssue = await pluginInstallationIssue(
                installed: installed,
                record: installedRecord,
                forceRefresh: forceRefresh
            )
            var installedManifest: NativePluginManifest?
            var permissions: [LocalConnectorPluginPermission] = []
            if let installedRecord, installationIssue == nil {
                do {
                    installedManifest = try installedPluginManifest(record: installedRecord)
                } catch {
                    installedManifest = nil
                    installationIssue = .integrityCheckFailed
                }
                if let installedManifest {
                    permissions = try await pluginPermissionSnapshots.permissions(
                        record: installedRecord,
                        manifest: installedManifest,
                        forceRefresh: forceRefresh
                    )
                }
            }
            plugins.append(.init(
                pluginID: id,
                packageName: source.catalog.name,
                pluginKey: source.catalog.pluginKey,
                displayName: installedManifest?.interface?.displayName
                    ?? source.catalog.displayName
                    ?? source.catalog.name
                    ?? id,
                description: source.catalog.description ?? "",
                category: source.catalog.interface?.category ?? "Plugin",
                publisher: source.catalog.publisher?.name
                    ?? source.catalog.interface?.developerName
                    ?? "ChatOS",
                latestVersion: source.release.version ?? source.release.id,
                installedVersion: installedRecord?.version,
                installed: installed,
                updateAvailable: Self.pluginUpdateAvailable(
                    installed: installedRecord,
                    release: source.release
                ),
                installAvailable: source.release.artifactSHA256 != nil
                    && source.release.npmPackage != nil,
                enabled: state.pluginPreferences[id] ?? source.preference?.enabled ?? true,
                hasUI: source.catalog.hasUI ?? installedManifest.map { !$0.ui.isEmpty },
                permissions: permissions,
                installationIssue: installationIssue,
                source: .marketplace
            ))
        }
        plugins.append(contentsOf: await customInstalledPlugins(forceRefresh: forceRefresh))
        return plugins
    }

    func repairLegacyPluginIntegrityRecordsIfNeeded(
        sources: [GatewayPluginSourceDTO]? = nil
    ) async {
        guard (state.installedPluginRecords ?? [:]).values.contains(where: {
            $0.source == nil && $0.packageFileSHA256?.isEmpty != false
        }) else { return }
        if let pluginIntegrityRepairTask {
            await pluginIntegrityRepairTask.value
            return
        }
        let task = Task { [weak self] in
            guard let self else { return }
            await self.repairLegacyPluginIntegrityRecords(sources: sources)
        }
        pluginIntegrityRepairTask = task
        await task.value
        pluginIntegrityRepairTask = nil
    }

    private func repairLegacyPluginIntegrityRecords(
        sources suppliedSources: [GatewayPluginSourceDTO]?
    ) async {
        do {
            let sources: [GatewayPluginSourceDTO] = if let suppliedSources {
                suppliedSources
            } else {
                try await pluginSources().items
            }
            let sourcesByID: [String: GatewayPluginSourceDTO] = Dictionary(
                uniqueKeysWithValues: sources.map {
                    ($0.catalog.id, $0)
                }
            )
            let token = try requireAccessToken()
            let candidates = (state.installedPluginRecords ?? [:]).values
                .filter { $0.source == nil && $0.packageFileSHA256?.isEmpty != false }
                .sorted { $0.pluginID < $1.pluginID }
            var changed = false
            for record in candidates {
                guard let source = sourcesByID[record.pluginID] else { continue }
                do {
                    let attested = try await pluginInstaller.attestLegacyInstallation(
                        record: record,
                        source: source,
                        token: token,
                        gateway: gateway
                    )
                    guard state.installedPluginRecords?[record.pluginID] == record else { continue }
                    state.installedPluginRecords?[record.pluginID] = attested
                    changed = true
                } catch {
                    continue
                }
            }
            guard changed else { return }
            try stateStore.save(state)
            await pluginPermissionSnapshots.invalidateAll()
            await pluginIntegritySnapshots.invalidateAll()
            try? await publishPluginInstallationStatus()
        } catch {
            return
        }
    }

    private func pluginInstallationIssue(
        installed: Bool,
        record: NativeInstalledPluginRecord?,
        forceRefresh: Bool
    ) async -> LocalConnectorPluginInstallationIssue? {
        guard installed else { return nil }
        guard let record else { return .missingIntegrityRecord }
        switch await pluginIntegritySnapshots.status(record: record, forceRefresh: forceRefresh) {
        case .verified:
            return nil
        case .missingRecord:
            return .missingIntegrityRecord
        case .failed:
            return .integrityCheckFailed
        }
    }

    static func pluginUpdateAvailable(
        installed: NativeInstalledPluginRecord?,
        release: GatewayPluginReleaseDTO
    ) -> Bool {
        guard let installed else { return false }
        guard installed.releaseID != release.id else { return false }
        guard let latestVersion = normalizedPluginVersion(release.version) else { return false }
        let latestArtifact = release.artifactSHA256?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        return normalizedPluginVersion(installed.version) != latestVersion
            || latestArtifact.map { installed.artifactSHA256 != $0 } == true
    }

    private static func normalizedPluginVersion(_ value: String?) -> String? {
        let normalized = value?.trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized?.isEmpty == false ? normalized : nil
    }

    public func installPlugin(id: String) async throws {
        let sources = try await pluginSources(forceRefresh: true)
        guard let source = sources.items.first(where: { $0.catalog.id == id }) else {
            throw NativeConnectorError.pluginInstallation("Marketplace 中没有找到这个 Plugin")
        }
        let token = try requireAccessToken()
        let record = try await pluginInstaller.install(source: source, token: token, gateway: gateway)
        await pluginPermissionSnapshots.invalidateAll()
        await pluginIntegritySnapshots.invalidateAll()
        var records = state.installedPluginRecords ?? [:]
        records[id] = record
        state.installedPluginRecords = records
        state.installedPluginIDs.insert(id)
        _ = reconcileInstalledPluginIdentities(with: sources.items)
        try stateStore.save(state)
        try? await publishPluginInstallationStatus()
    }

    func pluginSources(
        forceRefresh: Bool = false
    ) async throws -> GatewayPluginSourceListDTO {
        try Task.checkCancellation()
        let now = Date()
        if !forceRefresh,
           let cache = pluginSourceCache,
           Self.pluginSourceCacheIsUsable(
               cacheGeneration: cache.generation,
               currentGeneration: pluginSourceGeneration,
               expiresAt: cache.expiresAt,
               now: now
           ) {
            return cache.value
        }

        if let refresh = pluginSourceRefresh,
           refresh.generation == pluginSourceGeneration {
            do {
                let response = try await refresh.task.value
                return try finalizePluginSourceRefresh(
                    response,
                    generation: refresh.generation
                )
            } catch {
                if pluginSourceRefresh?.generation == refresh.generation {
                    pluginSourceRefresh = nil
                }
                guard Self.pluginSourceRefreshIsCurrent(
                    expectedGeneration: refresh.generation,
                    currentGeneration: pluginSourceGeneration,
                    isCancelled: Task.isCancelled
                ) else {
                    throw CancellationError()
                }
                throw error
            }
        }

        if forceRefresh {
            pluginSourceGeneration &+= 1
        }
        let generation = pluginSourceGeneration
        let token = try requireAccessToken()
        let gateway = gateway
        let task = Task { try await gateway.pluginSources(token: token) }
        pluginSourceRefresh = .init(generation: generation, task: task)
        do {
            let response = try await task.value
            return try finalizePluginSourceRefresh(response, generation: generation)
        } catch {
            if pluginSourceRefresh?.generation == generation {
                pluginSourceRefresh = nil
            }
            guard Self.pluginSourceRefreshIsCurrent(
                expectedGeneration: generation,
                currentGeneration: pluginSourceGeneration,
                isCancelled: Task.isCancelled
            ) else {
                throw CancellationError()
            }
            throw error
        }
    }

    func invalidatePluginSources() {
        pluginSourceGeneration &+= 1
        pluginSourceRefresh?.task.cancel()
        pluginSourceRefresh = nil
        pluginSourceCache = nil
    }

    private func finalizePluginSourceRefresh(
        _ response: GatewayPluginSourceListDTO,
        generation: Int
    ) throws -> GatewayPluginSourceListDTO {
        guard Self.pluginSourceRefreshIsCurrent(
            expectedGeneration: generation,
            currentGeneration: pluginSourceGeneration,
            isCancelled: Task.isCancelled
        ) else {
            throw CancellationError()
        }
        let now = Date()
        if let cache = pluginSourceCache,
           Self.pluginSourceCacheIsUsable(
               cacheGeneration: cache.generation,
               currentGeneration: generation,
               expiresAt: cache.expiresAt,
               now: now
           ) {
            if pluginSourceRefresh?.generation == generation {
                pluginSourceRefresh = nil
            }
            return cache.value
        }
        pluginSourceCache = .init(
            generation: generation,
            value: response,
            expiresAt: now.addingTimeInterval(Self.pluginSourceCacheTTL)
        )
        if pluginSourceRefresh?.generation == generation {
            pluginSourceRefresh = nil
        }
        return response
    }

    static func pluginSourceCacheIsUsable(
        cacheGeneration: Int,
        currentGeneration: Int,
        expiresAt: Date,
        now: Date
    ) -> Bool {
        cacheGeneration == currentGeneration && expiresAt > now
    }

    static func pluginSourceRefreshIsCurrent(
        expectedGeneration: Int,
        currentGeneration: Int,
        isCancelled: Bool
    ) -> Bool {
        !isCancelled && expectedGeneration == currentGeneration
    }

    public func startBrowserExtensionPairing(pluginID: String) async throws {
        guard state.pluginPreferences[pluginID] ?? true,
              let record = state.installedPluginRecords?[pluginID],
              let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            throw NativeConnectorError.browserExtensionPairing("Browser CDP 尚未安装或设备尚未配对")
        }
        try Self.verifyTrustedPluginInstallation(record)
        let manifest = try installedPluginManifest(record: record)
        guard manifest.name == NativeBrowserPluginIdentity.packageName,
              manifest.mcpServers[NativeBrowserPluginIdentity.componentKey] != nil else {
            throw NativeConnectorError.browserExtensionPairing("当前 Plugin 不是可连接 Chrome 的 Browser CDP")
        }
        let launch = try NativePluginManifestLoader.prepare(
            record: record,
            manifest: manifest,
            componentKey: NativeBrowserPluginIdentity.componentKey,
            serverKey: NativeBrowserPluginIdentity.componentKey,
            adapterSessionID: "browser-extension-pairing-\(UUID().uuidString.lowercased())",
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            workspaceRoot: nil,
            permissionSnapshot: Set(manifest.permissions.map(\.permission)),
            runtimeRootURL: pluginRuntimeRootURL
        )
        try await browserExtensionPairingRuntime.start(launch: launch)
    }

    public func isBrowserExtensionPaired(pluginID: String) async throws -> Bool {
        guard let record = state.installedPluginRecords?[pluginID],
              let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            return false
        }
        guard await pluginIntegritySnapshots.status(record: record) == .verified else {
            return false
        }
        let manifest = try installedPluginManifest(record: record)
        guard manifest.name == NativeBrowserPluginIdentity.packageName,
              manifest.mcpServers[NativeBrowserPluginIdentity.componentKey] != nil else {
            return false
        }
        let runtimeContext = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: NativeBrowserPluginIdentity.componentKey,
            runtimeRootURL: pluginRuntimeRootURL,
            pluginID: record.pluginID,
            host: .init(
                ownerUserID: ownerUserID,
                deviceID: deviceID,
                workspaceID: nil,
                workspaceRoot: nil,
                projectID: nil,
                projectName: nil
            )
        )
        return NativeBrowserExtensionPairingStatus.isPaired(
            at: runtimeContext.dataURL
                .appendingPathComponent("browser-bridge", isDirectory: true)
                .appendingPathComponent("extension-pairing.json")
        )
    }

    public func uninstallPlugin(id: String) async throws {
        await browserExtensionPairingRuntime.stop()
        await pluginApplicationRuntime.stop(pluginID: id)
        try pluginInstaller.uninstall(pluginID: id)
        await pluginPermissionSnapshots.invalidateAll()
        await pluginIntegritySnapshots.invalidateAll()
        state.installedPluginIDs.remove(id)
        state.installedPluginRecords?[id] = nil
        state.pluginHookTrustRecords = (state.pluginHookTrustRecords ?? [:]).filter {
            $0.value.pluginID != id
        }
        customPluginRuntimeHealth[id] = nil
        try stateStore.save(state)
        await syncTrustedPluginHookBundles()
        try? await publishPluginInstallationStatus()
    }

    public func updatePluginEnabled(id: String, enabled: Bool) async throws {
        if state.installedPluginRecords?[id]?.source == nil {
            let token = try requireAccessToken()
            guard let deviceID = state.deviceID else { throw NativeConnectorError.notPaired }
            try await gateway.updatePluginPreference(
                token: token,
                pluginID: id,
                deviceID: deviceID,
                enabled: enabled
            )
        }
        state.pluginPreferences[id] = enabled
        if !enabled {
            await browserExtensionPairingRuntime.stop()
            await pluginApplicationRuntime.stop(pluginID: id)
        }
        try stateStore.save(state)
        await syncTrustedPluginHookBundles()
        try? await publishPluginInstallationStatus()
    }

    public func requestPluginPermission(pluginID: String, permissionID: String) async throws {
        guard let record = state.installedPluginRecords?[pluginID] else {
            throw NativeConnectorError.pluginInstallation("Plugin 尚未安装")
        }
        try Self.verifyTrustedPluginInstallation(record)
        let manifest = try installedPluginManifest(record: record)
        if try await NativePluginPermissionInspector.request(
            record: record,
            manifest: manifest,
            permissionID: permissionID
        ) {
            await pluginPermissionSnapshots.invalidate(record: record)
            return
        }
        let nativePermissionID: String
        switch permissionID {
        case "computer.accessibility": nativePermissionID = "accessibility"
        case "computer.screen-recording": nativePermissionID = "screen_recording"
        default:
            throw NativeConnectorError.pluginInstallation("这个权限不需要系统设置")
        }
        await MainActor.run { NativeSystemPermissions.request(nativePermissionID) }
        await pluginPermissionSnapshots.invalidate(record: record)
    }

    private func installedPluginManifest(
        record: NativeInstalledPluginRecord
    ) throws -> NativePluginManifest {
        let url = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .appendingPathComponent("chatos.plugin.json")
        let manifest = try NativePluginManifestLoader.loadManifest(from: url)
        guard manifest.name.isEmpty == false,
              manifest.version == record.version else {
            throw NativeConnectorError.pluginInstallation("Plugin 权限清单与安装记录不一致")
        }
        return manifest
    }

    static func verifyTrustedPluginInstallation(
        _ record: NativeInstalledPluginRecord
    ) throws {
        switch NativePluginInstallationIntegrity.status(record: record) {
        case .verified:
            return
        case .missingRecord:
            throw NativeConnectorError.pluginInstallation(
                "Plugin 安装缺少可信完整性记录，请从 Marketplace 重新安装"
            )
        case .failed:
            throw NativeConnectorError.pluginInstallation(
                "Plugin 安装未通过完整性校验，请从 Marketplace 重新安装"
            )
        }
    }

    private func pluginApplication(
        record: NativeInstalledPluginRecord,
        manifest: NativePluginManifest,
        contribution: NativePluginManifest.UIContribution
    ) -> LocalConnectorPluginApplication {
        let installationURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .standardizedFileURL
        let iconPath = manifest.interface?.logo?.path
        let iconURL = iconPath.flatMap { path -> URL? in
            let normalized = path.hasPrefix("./") ? String(path.dropFirst(2)) : path
            guard !normalized.isEmpty,
                  !normalized.hasPrefix("/"),
                  !normalized.split(separator: "/").contains("..") else { return nil }
            let url = installationURL.appendingPathComponent(normalized).standardizedFileURL
            guard url.path.hasPrefix(installationURL.path + "/"),
                  FileManager.default.fileExists(atPath: url.path) else { return nil }
            return url
        }
        let contributionTitle = contribution.title?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let interfaceTitle = manifest.interface?.displayName?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return .init(
            pluginID: record.pluginID,
            componentKey: contribution.componentKey,
            displayName: contributionTitle.flatMap { $0.isEmpty ? nil : $0 }
                ?? interfaceTitle.flatMap { $0.isEmpty ? nil : $0 }
                ?? manifest.name,
            description: manifest.description,
            brandColor: manifest.interface?.brandColor,
            iconURL: iconURL,
            requiresLocalRuntime: contribution.runtime != nil,
            contextScope: manifest.runtimeContext?.applies(to: contribution.componentKey) == true
                ? manifest.runtimeContext?.scope
                : nil,
            missingContext: manifest.runtimeContext?.applies(to: contribution.componentKey) == true
                ? manifest.runtimeContext?.missingContext
                : nil,
            bridgeCapabilities: contribution.bridgeCapabilities
        )
    }

    @discardableResult
    func reconcileInstalledPluginIdentities(with sources: [GatewayPluginSourceDTO]) -> Bool {
        Self.reconcileInstalledPluginIdentities(state: &state, sources: sources)
    }

    @discardableResult
    static func reconcileInstalledPluginIdentities(
        state: inout NativeConnectorPersistentState,
        sources: [GatewayPluginSourceDTO]
    ) -> Bool {
        guard var records = state.installedPluginRecords, !records.isEmpty else { return false }
        let currentIDs = Set(sources.map(\.catalog.id))
        let sourcesByPluginKey = Dictionary(grouping: sources) {
            $0.catalog.pluginKey?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        }
        let sourcesByPackageName = Dictionary(grouping: sources) {
            ($0.catalog.name ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        }
        var sourcesByArtifact: [String: [GatewayPluginSourceDTO]] = [:]
        for source in sources {
            guard let artifact = source.release.artifactSHA256?
                .trimmingCharacters(in: CharacterSet.whitespacesAndNewlines),
                  !artifact.isEmpty else {
                continue
            }
            sourcesByArtifact[artifact, default: []].append(source)
        }
        var changed = false

        for (storedID, record) in records.sorted(by: { $0.key < $1.key }) {
            guard record.source == nil else { continue }
            if currentIDs.contains(storedID) {
                guard let source = sources.first(where: { $0.catalog.id == storedID }),
                      record.pluginKey != source.catalog.pluginKey else {
                    continue
                }
                var enriched = record
                enriched.pluginKey = source.catalog.pluginKey
                records[storedID] = enriched
                changed = true
                continue
            }
            let artifact = record.artifactSHA256.trimmingCharacters(in: .whitespacesAndNewlines)
            let pluginKey = record.pluginKey?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            let packageName = installedPluginPackageName(record: record)
            let identityMatches = !pluginKey.isEmpty
                ? sourcesByPluginKey[pluginKey]
                : packageName.flatMap { sourcesByPackageName[$0] }

            if let identityMatches,
               identityMatches.count == 1,
               let source = identityMatches.first,
               records[source.catalog.id] != nil {
                records[storedID] = nil
                state.installedPluginIDs.remove(storedID)
                state.pluginPreferences[storedID] = nil
                changed = true
                continue
            }

            let matches = identityMatches ?? sourcesByArtifact[artifact]
            guard !artifact.isEmpty,
                  let matches,
                  matches.count == 1,
                  let source = matches.first,
                  source.release.artifactSHA256?
                    .trimmingCharacters(in: .whitespacesAndNewlines) == artifact,
                  source.release.version?.trimmingCharacters(in: .whitespacesAndNewlines)
                    == record.version.trimmingCharacters(in: .whitespacesAndNewlines) else {
                continue
            }

            let currentID = source.catalog.id
            var migrated = record
            migrated.pluginID = currentID
            migrated.pluginKey = source.catalog.pluginKey
            migrated.releaseID = source.release.id
            records[storedID] = nil
            records[currentID] = migrated
            state.installedPluginIDs.remove(storedID)
            state.installedPluginIDs.insert(currentID)
            if let enabled = state.pluginPreferences.removeValue(forKey: storedID) {
                state.pluginPreferences[currentID] = enabled
            }
            changed = true
        }

        if changed {
            state.installedPluginRecords = records
        }
        return changed
    }

    private static func installedPluginPackageName(
        record: NativeInstalledPluginRecord
    ) -> String? {
        let manifestURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .appendingPathComponent("chatos.plugin.json")
        guard let manifest = try? NativePluginManifestLoader.loadManifest(from: manifestURL) else {
            return nil
        }
        let name = manifest.name.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? nil : name
    }
}

struct NativePluginSourceCache: Sendable {
    let generation: Int
    let value: GatewayPluginSourceListDTO
    let expiresAt: Date
}

struct NativePluginSourceRefresh: Sendable {
    let generation: Int
    let task: Task<GatewayPluginSourceListDTO, Error>
}
