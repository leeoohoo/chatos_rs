import ChatOSCore
import Foundation

struct NativeCustomPluginRuntimeHealth: Sendable, Equatable {
    var status: LocalConnectorPluginRuntimeStatus
    var toolCount: Int?
    var checkedAt: Date
    var error: String?
}

extension NativeLocalConnectorService {
    public func inspectCustomMCP(
        _ draft: LocalConnectorCustomMCPDraft
    ) async throws -> LocalConnectorPluginInspection {
        let prepared = try await pluginInstaller.prepareCustomMCP(draft)
        defer { pluginInstaller.discardCustomPlugin(prepared) }
        return try await inspectPreparedCustomPlugin(prepared)
    }

    public func installCustomMCP(
        _ draft: LocalConnectorCustomMCPDraft,
        expectedFingerprint: String
    ) async throws {
        let prepared = try await pluginInstaller.prepareCustomMCP(draft)
        var shouldDiscard = true
        defer {
            if shouldDiscard { pluginInstaller.discardCustomPlugin(prepared) }
        }
        let inspection = try await inspectPreparedCustomPlugin(prepared)
        guard inspection.fingerprint == expectedFingerprint else {
            throw NativeConnectorError.pluginInstallation(
                "MCP package 或工具清单在确认后发生变化，请重新预检并确认"
            )
        }
        let record = try pluginInstaller.commitCustomPlugin(prepared)
        shouldDiscard = false
        try await persistCustomPlugin(record)
    }

    public func inspectLocalPlugin(atPath path: String) async throws -> LocalConnectorPluginInspection {
        let prepared = try pluginInstaller.prepareLocalPlugin(atPath: path)
        defer { pluginInstaller.discardCustomPlugin(prepared) }
        return try await inspectPreparedCustomPlugin(prepared)
    }

    public func installLocalPlugin(
        atPath path: String,
        expectedFingerprint: String
    ) async throws {
        let prepared = try pluginInstaller.prepareLocalPlugin(atPath: path)
        var shouldDiscard = true
        defer {
            if shouldDiscard { pluginInstaller.discardCustomPlugin(prepared) }
        }
        let inspection = try await inspectPreparedCustomPlugin(prepared)
        guard inspection.fingerprint == expectedFingerprint else {
            throw NativeConnectorError.pluginInstallation(
                "Plugin 文件或工具清单在确认后发生变化，请重新预检并确认"
            )
        }
        let record = try pluginInstaller.commitCustomPlugin(prepared)
        shouldDiscard = false
        try await persistCustomPlugin(record)
    }

    public func inspectPluginSource(
        _ source: LocalConnectorPluginSourceDraft
    ) async throws -> LocalConnectorPluginInspection {
        let prepared = try await pluginInstaller.preparePluginSource(source)
        defer { pluginInstaller.discardCustomPlugin(prepared) }
        return try await inspectPreparedCustomPlugin(prepared)
    }

    public func installPluginSource(
        _ source: LocalConnectorPluginSourceDraft,
        expectedFingerprint: String
    ) async throws {
        let prepared = try await pluginInstaller.preparePluginSource(source)
        var shouldDiscard = true
        defer {
            if shouldDiscard { pluginInstaller.discardCustomPlugin(prepared) }
        }
        let inspection = try await inspectPreparedCustomPlugin(prepared)
        guard inspection.fingerprint == expectedFingerprint else {
            throw NativeConnectorError.pluginInstallation(
                "Plugin 来源或组件清单在确认后发生变化，请重新预检并确认"
            )
        }
        let record = try pluginInstaller.commitCustomPlugin(prepared)
        shouldDiscard = false
        try await persistCustomPlugin(record)
    }

    private func inspectPreparedCustomPlugin(
        _ prepared: NativePreparedCustomPlugin
    ) async throws -> LocalConnectorPluginInspection {
        try await inspectCustomPlugin(
            record: prepared.record,
            sourceSummary: prepared.sourceSummary
        )
    }

    private func inspectCustomPlugin(
        record: NativeInstalledPluginRecord,
        sourceSummary: String
    ) async throws -> LocalConnectorPluginInspection {
        let manifest = try Self.loadCustomPluginManifest(record: record)
        var permissionSnapshot = Set(manifest.permissions.map(\.permission))
        if !manifest.mcpServers.isEmpty || !manifest.hooks.isEmpty {
            permissionSnapshot.insert("process.spawn")
        }
        var components: [LocalConnectorPluginInspectionComponent] = []
        var fingerprintParts = [record.artifactSHA256, sourceSummary]
        for componentKey in manifest.mcpServers.keys.sorted() {
            let launch = try NativePluginManifestLoader.prepare(
                record: record,
                manifest: manifest,
                componentKey: componentKey,
                serverKey: nil,
                adapterSessionID: "custom-plugin-inspection-\(UUID().uuidString)",
                ownerUserID: state.user?.id ?? "local-user",
                deviceID: state.deviceID ?? "local-device",
                workspaceRoot: nil,
                permissionSnapshot: permissionSnapshot,
                runtimeRootURL: pluginRuntimeRootURL
            )
            let client = NativePluginStdioClient(launch: launch)
            do {
                try await client.start()
                let initialized = try await client.initialize()
                await client.terminate()
                let tools = try Self.validatedAgentPluginTools(initialized.tools)
                let toolNames = tools.compactMap { $0.jsonObject?["name"]?.jsonString }.sorted()
                guard toolNames.count == tools.count,
                      Set(toolNames).count == toolNames.count else {
                    throw NativePluginRuntimeError.invalidMCPResponse("MCP 工具名称缺失或重复")
                }
                components.append(.init(
                    componentKey: componentKey,
                    displayName: launch.displayName,
                    kind: .mcp,
                    toolNames: toolNames
                ))
                fingerprintParts.append(componentKey)
                fingerprintParts.append(contentsOf: tools.map(\.canonicalJSONString).sorted())
            } catch {
                await client.terminate()
                throw error
            }
        }
        let skillSnapshot = try PluginSkillSnapshot.load(
            installationRoot: URL(
                fileURLWithPath: record.installationPath,
                isDirectory: true
            ),
            relativeSkillDirectories: manifest.skills.map(\.path)
        )
        for skillName in skillSnapshot.skillNames.sorted() {
            components.append(.init(
                componentKey: skillName,
                displayName: skillName,
                kind: .skill,
                toolNames: []
            ))
            fingerprintParts.append("skill:\(skillName)")
        }
        let installationRoot = URL(
            fileURLWithPath: record.installationPath,
            isDirectory: true
        )
        for hook in manifest.hooks.sorted(by: { $0.componentKey < $1.componentKey }) {
            let bundle = try NativeCodexHookBundleParser.load(
                contribution: hook,
                installationRoot: installationRoot
            )
            let runtimes = try bundle.requiredRuntimes.map { name in
                let executable = try NativePluginInstaller.resolveExecutable(named: name)
                return "\(name): \(executable.path)"
            }
            components.append(.init(
                componentKey: hook.componentKey,
                displayName: hook.componentKey,
                kind: .hook,
                toolNames: [],
                hookDialect: hook.dialect,
                hookEvents: bundle.events,
                hookCommands: bundle.commands,
                hookRuntimes: runtimes
            ))
            fingerprintParts.append(contentsOf: [
                "hook:\(hook.componentKey)",
                hook.dialect,
                hook.path,
                bundle.contentSHA256,
                bundle.normalizedSHA256,
            ])
        }
        guard !components.isEmpty else {
            throw NativeConnectorError.pluginInstallation("Plugin 没有可安装的 Skill、MCP 或 Hook 组件")
        }
        let fingerprint = NativePluginInstaller.sha256Hex(
            fingerprintParts.joined(separator: "\n")
        )
        return .init(
            fingerprint: fingerprint,
            displayName: manifest.interface?.displayName ?? manifest.name,
            version: manifest.version,
            sourceSummary: sourceSummary,
            components: components,
            requestedPermissions: Array(permissionSnapshot).sorted()
        )
    }

    private func persistCustomPlugin(_ record: NativeInstalledPluginRecord) async throws {
        var records = state.installedPluginRecords ?? [:]
        records[record.pluginID] = record
        state.installedPluginRecords = records
        state.installedPluginIDs.insert(record.pluginID)
        state.pluginPreferences[record.pluginID] = true
        try stateStore.save(state)
        await pluginPermissionSnapshots.invalidateAll()
        await pluginIntegritySnapshots.invalidateAll()
        await pluginConfigurationChangedHandler?()
    }

    func customInstalledPlugins(forceRefresh: Bool) async -> [LocalConnectorPlugin] {
        let records = (state.installedPluginRecords ?? [:]).values
            .filter {
                $0.source == .customMCP || $0.source == .localPlugin
                    || $0.source == .gitPlugin || $0.source == .npmPlugin
            }
            .sorted { $0.pluginID < $1.pluginID }
        var result: [LocalConnectorPlugin] = []
        for record in records {
            let issue = await customPluginInstallationIssue(record, forceRefresh: forceRefresh)
            let manifest = issue == nil ? try? Self.loadCustomPluginManifestUnchecked(record: record) : nil
            let enabled = state.pluginPreferences[record.pluginID] ?? true
            let hasMCP = manifest?.mcpServers.isEmpty == false
            if issue != nil || !hasMCP {
                customPluginRuntimeHealth.removeValue(forKey: record.pluginID)
            } else if !enabled {
                customPluginRuntimeHealth[record.pluginID] = .init(
                    status: .disabled, toolCount: nil, checkedAt: Date(), error: nil
                )
            } else if forceRefresh {
                do {
                    let inspection = try await inspectCustomPlugin(
                        record: record,
                        sourceSummary: record.sourceSummary ?? "installed custom plugin"
                    )
                    customPluginRuntimeHealth[record.pluginID] = .init(
                        status: .ready,
                        toolCount: inspection.components.reduce(0) { $0 + $1.toolNames.count },
                        checkedAt: Date(),
                        error: nil
                    )
                } catch {
                    customPluginRuntimeHealth[record.pluginID] = .init(
                        status: .unavailable, toolCount: nil, checkedAt: Date(),
                        error: error.localizedDescription
                    )
                }
            }
            let runtimeHealth = customPluginRuntimeHealth[record.pluginID]
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
            let category: String
            switch record.source {
            case .some(.customMCP): category = "Custom MCP"
            case .some(.gitPlugin): category = "Git Plugin"
            case .some(.npmPlugin): category = "npm Plugin"
            default: category = "Local Plugin"
            }
            result.append(.init(
                pluginID: record.pluginID,
                packageName: manifest?.name,
                pluginKey: record.pluginKey,
                displayName: manifest?.interface?.displayName ?? manifest?.name ?? record.pluginID,
                description: manifest?.description ?? record.sourceSummary ?? "",
                category: category,
                publisher: "Local",
                latestVersion: record.version,
                installedVersion: record.version,
                installed: true,
                updateAvailable: false,
                installAvailable: false,
                enabled: enabled,
                hasUI: manifest.map { !$0.ui.isEmpty },
                permissions: permissions,
                installationIssue: issue,
                source: record.source ?? .localPlugin,
                runtimeStatus: hasMCP ? (runtimeHealth?.status ?? .notChecked) : nil,
                runtimeToolCount: runtimeHealth?.toolCount,
                runtimeLastCheckedAt: runtimeHealth?.checkedAt,
                runtimeError: runtimeHealth?.error
            ))
        }
        return result
    }

    private func customPluginInstallationIssue(
        _ record: NativeInstalledPluginRecord,
        forceRefresh: Bool
    ) async -> LocalConnectorPluginInstallationIssue? {
        switch await pluginIntegritySnapshots.status(record: record, forceRefresh: forceRefresh) {
        case .verified: nil
        case .missingRecord: .missingIntegrityRecord
        case .failed: .integrityCheckFailed
        }
    }

    private static func loadCustomPluginManifest(
        record: NativeInstalledPluginRecord
    ) throws -> NativePluginManifest {
        let installationURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
        try NativePluginInstallationIntegrity.verify(record: record, installationURL: installationURL)
        return try NativePluginManifestLoader.loadManifest(
            from: installationURL.appendingPathComponent("chatos.plugin.json")
        )
    }

    private static func loadCustomPluginManifestUnchecked(
        record: NativeInstalledPluginRecord
    ) throws -> NativePluginManifest {
        let installationURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
        let manifest = try NativePluginManifestLoader.loadManifest(
            from: installationURL.appendingPathComponent("chatos.plugin.json")
        )
        guard manifest.name.isEmpty == false, manifest.version == record.version else {
            throw NativeConnectorError.pluginInstallation("Plugin 权限清单与安装记录不一致")
        }
        return manifest
    }
}
