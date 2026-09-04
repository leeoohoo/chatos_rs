import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    public func fetchPluginApplications() async throws -> [LocalConnectorPluginApplication] {
        let records = state.installedPluginRecords ?? [:]
        return try records.values
            .filter { state.pluginPreferences[$0.pluginID] ?? true }
            .flatMap { record -> [LocalConnectorPluginApplication] in
                let manifest = try installedPluginManifest(record: record)
                return manifest.ui.compactMap { contribution in
                    guard contribution.surface == "workbench" else { return nil }
                    return pluginApplication(
                        record: record,
                        manifest: manifest,
                        contribution: contribution
                    )
                }
            }
            .sorted {
                if $0.displayName != $1.displayName {
                    return $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
                }
                return $0.id < $1.id
            }
    }

    public func launchPluginApplication(
        pluginID: String,
        componentKey: String
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        try await launchPluginApplication(
            pluginID: pluginID,
            componentKey: componentKey,
            context: nil
        )
    }

    public func launchPluginApplication(
        pluginID: String,
        componentKey: String,
        context: LocalConnectorPluginApplicationContext?
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        guard state.pluginPreferences[pluginID] ?? true else {
            throw NativeConnectorError.pluginInstallation("Plugin 已停用")
        }
        guard let record = state.installedPluginRecords?[pluginID] else {
            throw NativeConnectorError.pluginInstallation("Plugin 尚未安装")
        }
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
        let token = try requireAccessToken()
        let sources = try await gateway.pluginSources(token: token)
        if reconcileInstalledPluginIdentities(with: sources.items) {
            try stateStore.save(state)
            try? await sendPluginInstallationStatus()
        }
        return sources.items.map { source in
            let id = source.catalog.id
            let installedRecord = state.installedPluginRecords?[id]
            let installed = installedRecord != nil || state.installedPluginIDs.contains(id)
            let installedManifest: NativePluginManifest?
            let permissions: [LocalConnectorPluginPermission]
            if let installedRecord,
               let manifest = try? installedPluginManifest(record: installedRecord) {
                installedManifest = manifest
                permissions = NativePluginPermissionInspector.permissions(
                    record: installedRecord,
                    manifest: manifest
                )
            } else {
                installedManifest = nil
                permissions = []
            }
            return .init(
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
                permissions: permissions
            )
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
        let token = try requireAccessToken()
        let sources = try await gateway.pluginSources(token: token)
        guard let source = sources.items.first(where: { $0.catalog.id == id }) else {
            throw NativeConnectorError.pluginInstallation("Marketplace 中没有找到这个 Plugin")
        }
        let record = try await pluginInstaller.install(source: source, token: token, gateway: gateway)
        var records = state.installedPluginRecords ?? [:]
        records[id] = record
        state.installedPluginRecords = records
        state.installedPluginIDs.insert(id)
        _ = reconcileInstalledPluginIdentities(with: sources.items)
        try stateStore.save(state)
        try? await publishPluginInstallationStatus()
    }

    public func startBrowserExtensionPairing(pluginID: String) async throws {
        guard state.pluginPreferences[pluginID] ?? true,
              let record = state.installedPluginRecords?[pluginID],
              let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            throw NativeConnectorError.pluginInstallation("Browser CDP 尚未安装或设备尚未配对")
        }
        let manifest = try installedPluginManifest(record: record)
        guard manifest.name == "chatos-browser-cdp",
              manifest.mcpServers["browser-cdp"] != nil else {
            throw NativeConnectorError.pluginInstallation("当前 Plugin 不是可连接 Chrome 的 Browser CDP")
        }
        let launch = try NativePluginManifestLoader.prepare(
            record: record,
            componentKey: "browser-cdp",
            serverKey: "browser-cdp",
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
        let manifest = try installedPluginManifest(record: record)
        guard manifest.name == "chatos-browser-cdp",
              manifest.mcpServers["browser-cdp"] != nil else {
            return false
        }
        let runtimeContext = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: "browser-cdp",
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
        state.installedPluginIDs.remove(id)
        state.installedPluginRecords?[id] = nil
        try stateStore.save(state)
        try? await publishPluginInstallationStatus()
    }

    public func updatePluginEnabled(id: String, enabled: Bool) async throws {
        let token = try requireAccessToken()
        guard let deviceID = state.deviceID else { throw NativeConnectorError.notPaired }
        try await gateway.updatePluginPreference(
            token: token,
            pluginID: id,
            deviceID: deviceID,
            enabled: enabled
        )
        state.pluginPreferences[id] = enabled
        if !enabled {
            await browserExtensionPairingRuntime.stop()
            await pluginApplicationRuntime.stop(pluginID: id)
        }
        try stateStore.save(state)
        try? await publishPluginInstallationStatus()
    }

    public func requestPluginPermission(pluginID: String, permissionID: String) async throws {
        guard let record = state.installedPluginRecords?[pluginID] else {
            throw NativeConnectorError.pluginInstallation("Plugin 尚未安装")
        }
        let manifest = try installedPluginManifest(record: record)
        if try NativePluginPermissionInspector.request(
            record: record,
            manifest: manifest,
            permissionID: permissionID
        ) {
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
    }

    private func installedPluginManifest(
        record: NativeInstalledPluginRecord
    ) throws -> NativePluginManifest {
        let url = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .appendingPathComponent("chatos.plugin.json")
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: url, options: .mappedIfSafe)
        )
        guard manifest.name.isEmpty == false,
              manifest.version == record.version else {
            throw NativeConnectorError.pluginInstallation("Plugin 权限清单与安装记录不一致")
        }
        return manifest
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
                : nil
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
        guard let data = try? Data(contentsOf: manifestURL, options: .mappedIfSafe),
              let manifest = try? JSONDecoder().decode(NativePluginManifest.self, from: data) else {
            return nil
        }
        let name = manifest.name.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? nil : name
    }
}
