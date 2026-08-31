import Foundation

struct GatewayPluginInstallationStatusMessage: Encodable {
    var type = "plugin_installation_status"
    var items: [GatewayPluginInstallationStatusItem]
}

struct GatewayPluginInstallationStatusItem: Encodable, Equatable {
    var ownerUserID: String
    var deviceID: String
    var pluginID: String
    var releaseID: String
    var version: String
    var artifactSHA256: String
    var platform: String
    var installStatus: String
    var availabilityStatus: String
    var dependencyStatus: String
    var permissionStatus: String
    var grantedPermissions: [String]
    var authStatus: String
    var componentStatuses: [GatewayPluginComponentStatus]
    var active: Bool
    var installedAt: String?
    var lastError: String?

    enum CodingKeys: String, CodingKey {
        case version, platform, active
        case ownerUserID = "owner_user_id"
        case deviceID = "device_id"
        case pluginID = "plugin_id"
        case releaseID = "release_id"
        case artifactSHA256 = "artifact_sha256"
        case installStatus = "install_status"
        case availabilityStatus = "availability_status"
        case dependencyStatus = "dependency_status"
        case permissionStatus = "permission_status"
        case grantedPermissions = "granted_permissions"
        case authStatus = "auth_status"
        case componentStatuses = "component_statuses"
        case installedAt = "installed_at"
        case lastError = "last_error"
    }
}

struct GatewayPluginComponentStatus: Encodable, Equatable {
    var componentKey: String
    var kind: String
    var availabilityStatus: String
    var lastError: String?
    var lastCheckedAt: String

    enum CodingKeys: String, CodingKey {
        case kind
        case componentKey = "component_key"
        case availabilityStatus = "availability_status"
        case lastError = "last_error"
        case lastCheckedAt = "last_checked_at"
    }
}

enum NativePluginInstallationStatusBuilder {
    static func makeItem(
        record: NativeInstalledPluginRecord,
        ownerUserID: String,
        deviceID: String,
        platform: String,
        active: Bool,
        now: Date = Date(),
        fileManager: FileManager = .default
    ) throws -> GatewayPluginInstallationStatusItem {
        let installationURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .standardizedFileURL
        let manifestURL = installationURL.appendingPathComponent("chatos.plugin.json")
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: manifestURL, options: .mappedIfSafe)
        )
        guard manifest.schemaVersion == 3, manifest.version == record.version else {
            throw NativePluginRuntimeError.invalidManifest("Plugin manifest 与已安装 Release 不一致")
        }

        let checkedAt = ISO8601DateFormatter().string(from: now)
        let skillStatuses = manifest.skills.enumerated().map { index, skill in
            let componentKey = componentKeyFromPath(skill.path, fallback: "skills", index: index)
            let error = skillCollectionError(
                skill: skill,
                installationURL: installationURL,
                fileManager: fileManager
            )
            return GatewayPluginComponentStatus(
                componentKey: componentKey,
                kind: "skill_collection",
                availabilityStatus: error == nil ? "ready" : "unavailable",
                lastError: error,
                lastCheckedAt: checkedAt
            )
        }
        let mcpStatuses = manifest.mcpServers.keys.sorted().map { componentKey in
            let server = manifest.mcpServers[componentKey]!
            let error = executableError(
                server: server,
                installationURL: installationURL,
                fileManager: fileManager
            )
            return GatewayPluginComponentStatus(
                componentKey: componentKey,
                kind: "mcp_server",
                availabilityStatus: error == nil ? "ready" : "unavailable",
                lastError: error,
                lastCheckedAt: checkedAt
            )
        }
        let componentStatuses = skillStatuses + mcpStatuses
        guard !componentStatuses.isEmpty else {
            throw NativePluginRuntimeError.invalidManifest("Plugin 没有可运行的 MCP 组件")
        }

        let readyCount = componentStatuses.count(where: { $0.availabilityStatus == "ready" })
        let availabilityStatus: String
        switch readyCount {
        case componentStatuses.count:
            availabilityStatus = "ready"
        case 1...:
            availabilityStatus = "partially_available"
        default:
            availabilityStatus = "unavailable"
        }
        let lastError = componentStatuses.compactMap(\.lastError).first
        let grantedPermissions = Array(Set(manifest.permissions.map(\.permission))).sorted()

        return GatewayPluginInstallationStatusItem(
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            pluginID: record.pluginID,
            releaseID: record.releaseID,
            version: record.version,
            artifactSHA256: record.artifactSHA256,
            platform: platform,
            installStatus: "installed",
            availabilityStatus: availabilityStatus,
            dependencyStatus: "satisfied",
            permissionStatus: "satisfied",
            grantedPermissions: grantedPermissions,
            authStatus: "satisfied",
            componentStatuses: componentStatuses,
            active: active,
            installedAt: record.installedAt,
            lastError: lastError
        )
    }

    private static func executableError(
        server: NativePluginManifest.MCPServer,
        installationURL: URL,
        fileManager: FileManager
    ) -> String? {
        guard server.type == "stdio" else { return "当前客户端只支持 stdio MCP 组件" }
        guard safeExecutableName(server.bin) else { return "Plugin 可执行文件名无效" }
        let executableURL = installationURL
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(server.bin, isDirectory: false)
            .standardizedFileURL
        guard executableURL.path.hasPrefix(installationURL.path + "/") else {
            return "Plugin 可执行文件越过安装目录"
        }
        guard fileManager.fileExists(atPath: executableURL.path) else {
            return "Plugin 可执行文件不存在"
        }
        guard let values = try? executableURL.resourceValues(forKeys: [
            .isRegularFileKey,
            .isSymbolicLinkKey,
            .isExecutableKey,
        ]),
            values.isRegularFile == true,
            values.isSymbolicLink != true,
            values.isExecutable == true else {
            return "Plugin 可执行文件不可运行"
        }
        return nil
    }

    private static func skillCollectionError(
        skill: NativePluginManifest.PathReference,
        installationURL: URL,
        fileManager: FileManager
    ) -> String? {
        guard let relativePath = normalizedRelativePath(skill.path) else {
            return "Plugin Skill 路径无效"
        }
        let collectionURL = installationURL
            .appendingPathComponent(relativePath, isDirectory: true)
            .standardizedFileURL
        guard collectionURL.path.hasPrefix(installationURL.path + "/") else {
            return "Plugin Skill 路径越过安装目录"
        }
        guard fileManager.fileExists(atPath: collectionURL.path) else {
            return "Plugin Skill 目录不存在"
        }
        guard let collectionValues = try? collectionURL.resourceValues(forKeys: [
            .isDirectoryKey,
            .isSymbolicLinkKey,
        ]),
            collectionValues.isDirectory == true,
            collectionValues.isSymbolicLink != true else {
            return "Plugin Skill 目录不可用"
        }
        let entrypointURL = collectionURL.appendingPathComponent("SKILL.md", isDirectory: false)
        guard fileManager.fileExists(atPath: entrypointURL.path) else {
            return "Plugin Skill 缺少 SKILL.md"
        }
        guard let entrypointValues = try? entrypointURL.resourceValues(forKeys: [
            .isRegularFileKey,
            .isSymbolicLinkKey,
        ]),
            entrypointValues.isRegularFile == true,
            entrypointValues.isSymbolicLink != true else {
            return "Plugin Skill 入口不可用"
        }
        return nil
    }

    private static func normalizedRelativePath(_ value: String) -> String? {
        var trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasPrefix("./") { trimmed.removeFirst(2) }
        guard !trimmed.isEmpty, !trimmed.hasPrefix("/") else { return nil }
        guard trimmed.split(separator: "/", omittingEmptySubsequences: false).allSatisfy({
            !$0.isEmpty && $0 != "." && $0 != ".."
        }) else { return nil }
        return trimmed
    }

    private static func componentKeyFromPath(_ path: String, fallback: String, index: Int) -> String {
        let candidate = path
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .split(separator: "/")
            .last
            .map(String.init)?
            .split(separator: ".")
            .first
            .map(String.init) ?? fallback
        var key = candidate.lowercased().map { character -> Character in
            character.isASCII && (character.isLetter || character.isNumber) ? character : "-"
        }
        while String(key).contains("--") {
            key = Array(String(key).replacingOccurrences(of: "--", with: "-"))
        }
        var normalized = String(key).trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        if normalized.isEmpty { normalized = fallback }
        return index > 0 && normalized == fallback ? "\(normalized)-\(index + 1)" : normalized
    }

    private static func safeExecutableName(_ value: String) -> Bool {
        !value.isEmpty && value.count <= 128 && !value.contains("/") && value != "." && value != ".."
    }
}
