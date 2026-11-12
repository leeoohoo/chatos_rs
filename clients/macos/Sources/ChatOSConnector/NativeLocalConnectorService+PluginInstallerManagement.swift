import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    public func resolveProgramRelease(repositoryURL: String, tag: String, assetName: String) async throws -> LocalConnectorProgramRelease {
        try await pluginInstaller.repositoryClient.resolveArchive(repositoryURL: repositoryURL, tag: tag, assetName: assetName)
    }
    func discoverInstallerRepository(url: String, documentPath: String?, reference: String?) async throws -> LocalAgentJSONValue {
        try await pluginInstaller.repositoryClient.discover(repositoryURL: url, documentPath: documentPath, reference: reference)
    }

    func resolveInstallerRelease(url: String, tag: String, asset: String) async throws -> LocalAgentJSONValue {
        try await pluginInstaller.repositoryClient.resolveRelease(repositoryURL: url, tag: tag, assetName: asset)
    }
    /// Reads local records without starting MCP servers or depending on Marketplace availability.
    func installerInstalledPlugins(query: String?) -> [LocalAgentJSONValue] {
        (state.installedPluginRecords ?? [:]).values.sorted { $0.pluginID < $1.pluginID }
            .compactMap { record in
                let info = installerPluginInfo(record)
                let displayName: String = if case let .string(value)? = info["display_name"] { value } else { "" }
                guard query == nil || Self.installerMatches(query!, fields: [
                    record.pluginID, record.pluginKey ?? "", record.sourceSummary ?? "",
                    displayName,
                ]) else { return nil }
                return .object(info)
            }
    }

    func installerPluginRecord(id: String) -> NativeInstalledPluginRecord? {
        state.installedPluginRecords?[id]
    }

    func installerPluginRecords() -> [String: NativeInstalledPluginRecord] {
        state.installedPluginRecords ?? [:]
    }

    func installerPluginInfo(_ record: NativeInstalledPluginRecord) -> [String: LocalAgentJSONValue] {
        let manifest = try? NativePluginManifestLoader.loadManifest(
            from: URL(fileURLWithPath: record.installationPath).appendingPathComponent("chatos.plugin.json")
        )
        var result: [String: LocalAgentJSONValue] = [
            "plugin_id": .string(record.pluginID),
            "display_name": .string(manifest?.interface?.displayName ?? manifest?.name ?? record.pluginID),
            "package_name": .string(manifest?.name ?? ""),
            "installed_version": .string(record.version),
            "release_id": .string(record.releaseID),
            "source": .string((record.source ?? .marketplace).rawValue),
            "enabled": .bool(state.pluginPreferences[record.pluginID] ?? true),
            "component_keys": .array((manifest?.mcpServers.keys.sorted() ?? []).map(LocalAgentJSONValue.string)),
        ]
        if let source = Self.installerSourceDraft(record) {
            result["resolved_source"] = NativePluginInstallerMetadata.source(source)
            if let version = source.exactVersion { result["package_version"] = .string(version) }
        }
        if let draft = record.customMCPDraft {
            result["resolved_install_request"] = NativePluginInstallerMetadata.npm(draft)
            result["package_version"] = .string(draft.packageVersion)
        } else if record.source == .customMCP, let manifest,
                  let summary = record.sourceSummary {
            // Legacy wrappers already recorded the exact bin/profile; arguments come from
            // the structured manifest, never by splitting a shell-like summary string.
            let parts = summary.components(separatedBy: " · ")
            if parts.count == 3, let profile = LocalConnectorCustomMCPDraft.AccessProfile(rawValue: parts[2]),
               let component = manifest.mcpServers.keys.sorted().first,
               manifest.mcpServers.count == 1, let server = manifest.mcpServers[component],
               let binary = parts[1].split(separator: " ").first {
                result["resolved_install_request"] = NativePluginInstallerMetadata.npm(.init(
                    displayName: manifest.interface?.displayName ?? manifest.name,
                    description: manifest.description, componentKey: component,
                    packageName: manifest.name, packageVersion: record.version,
                    binaryName: String(binary), arguments: server.args, accessProfile: profile
                ))
            }
        }
        return result
    }

    static func installerMatches(_ query: String, fields: [String]) -> Bool {
        let terms = query.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        return terms.allSatisfy { term in fields.contains { $0.localizedCaseInsensitiveContains(term) } }
    }

    static func installerSourceDraft(_ record: NativeInstalledPluginRecord) -> LocalConnectorPluginSourceDraft? {
        if let source = record.sourceDraft { return source }
        guard let summary = record.sourceSummary else { return nil }
        let parts = summary.components(separatedBy: " · ")
        if record.source == .localPlugin {
            if let index = parts.firstIndex(of: "Local folder"), parts.indices.contains(index + 1) {
                return .init(
                    kind: .localFolder, location: parts[index + 1],
                    subdirectory: parts.indices.contains(index + 2) ? parts[index + 2] : nil
                )
            }
            if let path = parts.last, path.hasPrefix("/") {
                return .init(kind: .localFolder, location: path)
            }
        }
        if record.source == .gitPlugin, let index = parts.firstIndex(of: "Git"),
           parts.indices.contains(index + 2) {
            return .init(
                kind: .gitRepository, location: parts[index + 1], gitReference: parts[index + 2],
                subdirectory: parts.indices.contains(index + 3) ? parts[index + 3] : nil
            )
        }
        // Older npm Agent Plugins recorded the exact scoped name/version in the summary.
        guard record.source == .npmPlugin else { return nil }
        guard let index = parts.firstIndex(of: "npm Agent Plugin"),
              parts.indices.contains(index + 1) else { return nil }
        let spec = parts[index + 1]
        guard let separator = spec.lastIndex(of: "@"), separator != spec.startIndex else { return nil }
        return .init(
            kind: .npmPackage, location: String(spec[..<separator]),
            exactVersion: String(spec[spec.index(after: separator)...]),
            subdirectory: parts.indices.contains(index + 2) ? parts[index + 2] : nil
        )
    }

    func installedCustomPluginReplacement(_ pluginID: String?) throws -> NativeInstalledPluginRecord? {
        guard let pluginID else { return nil }
        guard let record = state.installedPluginRecords?[pluginID] else {
            throw NativeConnectorError.pluginInstallation(
                "replacing_plugin_id 指向的插件尚未安装。新安装请删除 replacing_plugin_id 字段或传 null 后重新预检；不要填写 REPLACE 等占位符。更新请先调用 plugin_installer_list_installed，使用实际返回的 plugin_id。不要重复提交相同的无效参数。"
            )
        }
        return record
    }

    func customPluginUpdate(
        _ prepared: NativePreparedCustomPlugin,
        replacing pluginID: String?
    ) throws -> NativePreparedCustomPlugin {
        guard let previous = try installedCustomPluginReplacement(pluginID) else { return prepared }
        guard previous.source != nil, previous.source == prepared.record.source else {
            throw NativeConnectorError.pluginInstallation("更新必须使用已安装插件的原来源类型，不能把完整 Plugin 替换为 MCP 包装器")
        }
        let oldManifest = try NativePluginManifestLoader.loadManifest(
            from: URL(fileURLWithPath: previous.installationPath).appendingPathComponent("chatos.plugin.json")
        )
        let newManifest = try NativePluginManifestLoader.loadManifest(
            from: URL(fileURLWithPath: prepared.record.installationPath).appendingPathComponent("chatos.plugin.json")
        )
        guard oldManifest.name == newManifest.name else {
            throw NativeConnectorError.pluginInstallation("预检的包名与原插件不一致，不能原位更新")
        }
        var updated = prepared
        updated.record.pluginID = previous.pluginID
        return updated
    }

}

enum NativePluginInstallerMetadata {
    static func source(_ source: LocalConnectorPluginSourceDraft) -> LocalAgentJSONValue {
        var result: [String: LocalAgentJSONValue] = [
            "source_kind": .string(source.kind.rawValue), "location": .string(source.location),
        ]
        if let value = source.exactVersion { result["exact_version"] = .string(value) }
        if let value = source.gitReference { result["git_reference"] = .string(value) }
        if let value = source.subdirectory { result["subdirectory"] = .string(value) }
        if let program = source.program {
            var value: [String: LocalAgentJSONValue] = [
                "display_name": .string(program.displayName), "component_key": .string(program.componentKey),
                "executable_path": .string(program.executablePath),
                "arguments": .array(program.arguments.map(LocalAgentJSONValue.string)),
                "access_profile": .string(program.accessProfile.rawValue),
            ]
            if let asset = program.assetName { value["asset_name"] = .string(asset) }
            if let digest = program.archiveSHA256 { value["archive_sha256"] = .string(digest) }
            result["program"] = .object(value)
        }
        return .object(result)
    }

    static func npm(_ draft: LocalConnectorCustomMCPDraft) -> LocalAgentJSONValue {
        .object([
            "display_name": .string(draft.displayName), "description": .string(draft.description),
            "component_key": .string(draft.componentKey), "package_name": .string(draft.packageName),
            "package_version": .string(draft.packageVersion), "binary_name": .string(draft.binaryName),
            "arguments": .array(draft.arguments.map(LocalAgentJSONValue.string)),
            "access_profile": .string(draft.accessProfile.rawValue),
        ])
    }
}
