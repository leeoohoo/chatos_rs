import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    public func fetchPluginApplications() async throws -> [LocalConnectorPluginApplication] {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let installations = try await pluginStateStore.installations(ownerUserID: ownerUserID)
        return try installations.values
            .filter(\.enabled)
            .flatMap { installation -> [LocalConnectorPluginApplication] in
                let record = installation.record
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
        componentKey: String,
        context: LocalConnectorPluginApplicationContext?,
        expectedOwnerUserID: String
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        let ownerUserID = try activeClientStorageOwnerUserID()
        guard ownerUserID == expectedOwnerUserID else { throw CancellationError() }
        let installation = try await pluginStateStore.installations(ownerUserID: ownerUserID)[pluginID]
        guard installation?.enabled == true else {
            throw NativeConnectorError.pluginInstallation("Plugin 已停用")
        }
        guard let record = installation?.record else {
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
        guard let deviceID = state.deviceID else {
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
        do {
            return try await fetchPluginsWithCurrentPairing()
        } catch NativeConnectorError.notPaired {
            // Switching between local and deployed gateways can leave only the connector token
            // stale. Re-pair it once through the still-valid primary ChatOS session.
            _ = try await pairWithCurrentChatOSSession(deviceName: Host.current().localizedName)
            return try await fetchPluginsWithCurrentPairing()
        }
    }

    private func fetchPluginsWithCurrentPairing() async throws -> [LocalConnectorPlugin] {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let installations = try await pluginStateStore.installations(ownerUserID: ownerUserID)
        let token = try requireAccessToken()
        let sources = try await gateway.pluginSources(token: token)
        return sources.items.map { source in
            let id = source.catalog.id
            let installation = installations[id]
            let installedRecord = installation?.record
            let installed = installedRecord != nil
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
                enabled: installation?.enabled ?? source.preference?.enabled ?? true,
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

    func enabledPluginRecord(pluginID: String) async throws -> NativeInstalledPluginRecord {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let installation = try await pluginStateStore.installations(
            ownerUserID: ownerUserID
        )[pluginID]
        guard installation?.enabled == true, let record = installation?.record else {
            throw NativeConnectorError.pluginInstallation("Plugin 未安装或已停用")
        }
        return record
    }

    public func installPlugin(id: String) async throws {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let token = try requireAccessToken()
        let sources = try await gateway.pluginSources(token: token)
        guard let source = sources.items.first(where: { $0.catalog.id == id }) else {
            throw NativeConnectorError.pluginInstallation("Marketplace 中没有找到这个 Plugin")
        }
        let record = try await pluginInstaller.install(source: source, token: token, gateway: gateway)
        do {
            _ = try await pluginStateStore.put(ownerUserID: ownerUserID, record: record)
        } catch {
            try? pluginInstaller.uninstall(pluginID: id)
            throw error
        }
        try? await publishPluginInstallationStatus()
    }

    public func startBrowserExtensionPairing(pluginID: String) async throws {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let installation = try await pluginStateStore.installations(ownerUserID: ownerUserID)[pluginID]
        guard installation?.enabled == true,
              let record = installation?.record,
              let deviceID = state.deviceID else {
            throw NativeConnectorError.browserExtensionPairing("Browser CDP 尚未安装或设备尚未配对")
        }
        let manifest = try installedPluginManifest(record: record)
        guard manifest.name == NativeBrowserPluginIdentity.packageName,
              manifest.mcpServers[NativeBrowserPluginIdentity.componentKey] != nil else {
            throw NativeConnectorError.browserExtensionPairing("当前 Plugin 不是可连接 Chrome 的 Browser CDP")
        }
        let launch = try NativePluginManifestLoader.prepare(
            record: record,
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
        let ownerUserID = try activeClientStorageOwnerUserID()
        guard let record = try await pluginStateStore.record(
            ownerUserID: ownerUserID,
            pluginID: pluginID
        ),
              let deviceID = state.deviceID else {
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
        let ownerUserID = try activeClientStorageOwnerUserID()
        await browserExtensionPairingRuntime.stop()
        await pluginApplicationRuntime.stop(pluginID: id)
        try await pluginStateStore.remove(ownerUserID: ownerUserID, pluginID: id)
        try pluginInstaller.uninstall(pluginID: id)
        try? await publishPluginInstallationStatus()
    }

    public func updatePluginEnabled(id: String, enabled: Bool) async throws {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let token = try requireAccessToken()
        guard let deviceID = state.deviceID else { throw NativeConnectorError.notPaired }
        try await gateway.updatePluginPreference(
            token: token,
            pluginID: id,
            deviceID: deviceID,
            enabled: enabled
        )
        try await pluginStateStore.setEnabled(
            ownerUserID: ownerUserID,
            pluginID: id,
            enabled: enabled
        )
        if !enabled {
            await browserExtensionPairingRuntime.stop()
            await pluginApplicationRuntime.stop(pluginID: id)
        }
        try? await publishPluginInstallationStatus()
    }

    public func requestPluginPermission(pluginID: String, permissionID: String) async throws {
        let ownerUserID = try activeClientStorageOwnerUserID()
        guard let record = try await pluginStateStore.record(
            ownerUserID: ownerUserID,
            pluginID: pluginID
        ) else {
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
                : nil,
            bridgeCapabilities: contribution.bridgeCapabilities
        )
    }

}
