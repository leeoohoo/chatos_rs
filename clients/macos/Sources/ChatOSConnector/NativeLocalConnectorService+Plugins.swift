import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
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
                installed: installed,
                updateAvailable: installedRecord.map { $0.version != source.release.version } ?? false,
                installAvailable: source.release.artifactSHA256 != nil
                    && source.release.npmPackage != nil,
                enabled: state.pluginPreferences[id] ?? source.preference?.enabled ?? true,
                permissions: permissions
            )
        }
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
        try stateStore.save(state)
        try? await publishPluginInstallationStatus()
    }

    public func uninstallPlugin(id: String) async throws {
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
            guard !currentIDs.contains(storedID) else { continue }
            let artifact = record.artifactSHA256.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !artifact.isEmpty,
                  let matches = sourcesByArtifact[artifact],
                  matches.count == 1,
                  let source = matches.first,
                  source.release.version?.trimmingCharacters(in: .whitespacesAndNewlines)
                    == record.version.trimmingCharacters(in: .whitespacesAndNewlines) else {
                continue
            }

            let currentID = source.catalog.id
            var migrated = record
            migrated.pluginID = currentID
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
}
