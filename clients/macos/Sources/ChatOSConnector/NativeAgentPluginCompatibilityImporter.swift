import Foundation

/// Converts portable Agent Plugins and the layouts used by Codex and Claude into the
/// internal immutable ChatOS package format. The source package is copied to staging
/// before this converter runs, so normalization never edits the user's original files.
enum NativeAgentPluginCompatibilityImporter {
    struct Result: Sendable {
        let manifest: NativePluginManifest
        let formatName: String
    }

    private struct Identity {
        let name: String
        let version: String
        let description: String
        let displayName: String
        let formatName: String
        let manifestPath: String?
        let rawManifest: [String: Any]?
    }

    static func normalizePackage(at packageRoot: URL) throws -> Result {
        let nativeManifestURL = packageRoot.appendingPathComponent("chatos.plugin.json")
        var identityDiagnostics: [String] = []
        var foundIdentityCandidate = false
        if FileManager.default.fileExists(atPath: nativeManifestURL.path) {
            foundIdentityCandidate = true
            do {
                return .init(
                    manifest: try NativePluginManifestLoader.loadManifest(from: nativeManifestURL),
                    formatName: "ChatOS Plugin · chatos.plugin.json"
                )
            } catch {
                identityDiagnostics.append("chatos.plugin.json：\(error.localizedDescription)")
            }
        }

        let identity = try discoverIdentity(
            at: packageRoot,
            initialDiagnostics: identityDiagnostics,
            foundInitialCandidate: foundIdentityCandidate
        )
        let skills = try discoverSkills(at: packageRoot, identityName: identity.name)
        let mcpServers = try discoverMCPServers(
            at: packageRoot,
            inlineManifest: identity.rawManifest
        )
        let hooks = try discoverHooks(
            at: packageRoot,
            inlineManifest: identity.rawManifest
        )
        guard !skills.isEmpty || !mcpServers.isEmpty || !hooks.isEmpty else {
            throw NativeConnectorError.pluginInstallation(
                "没有找到可导入的 Skill、MCP 或 Codex Hook"
            )
        }

        var permissions: [[String: Any]] = []
        if !mcpServers.isEmpty {
            permissions.append([
                "permission": "process.spawn",
                "required": true,
                "reason": "启动用户确认导入的本地 MCP Server",
                "components": mcpServers.keys.sorted(),
            ])
        }
        if !hooks.isEmpty {
            let hookComponents = hooks.compactMap { $0["componentKey"] as? String }.sorted()
            permissions.append(contentsOf: [
                [
                    "permission": "process.spawn",
                    "required": true,
                    "reason": "运行用户逐条确认的生命周期 Hook 命令",
                    "components": hookComponents,
                ],
                [
                    "permission": "workspace.read",
                    "required": true,
                    "reason": "向本地只读 Hook 提供当前授权工作区",
                    "components": hookComponents,
                ],
            ])
        }

        let manifest: [String: Any] = [
            "schemaVersion": 3,
            "name": identity.name,
            "version": identity.version,
            "description": identity.description,
            "skills": skills.map { ["path": $0] },
            "mcpServers": mcpServers,
            "hooks": hooks,
            "interface": ["displayName": identity.displayName],
            "permissions": permissions,
        ]
        try JSONSerialization.data(
            withJSONObject: manifest,
            options: [.prettyPrinted, .sortedKeys]
        ).write(to: nativeManifestURL, options: .atomic)
        return .init(
            manifest: try NativePluginManifestLoader.loadManifest(from: nativeManifestURL),
            formatName: identity.manifestPath.map { "\(identity.formatName) · \($0)" }
                ?? identity.formatName
        )
    }

    private static func discoverIdentity(
        at root: URL,
        initialDiagnostics: [String],
        foundInitialCandidate: Bool
    ) throws -> Identity {
        let candidates: [(String, String)] = [
            (".codex-plugin/plugin.json", "Codex Plugin"),
            (".claude-plugin/plugin.json", "Claude Plugin"),
            ("plugin.json", "Agent Plugin"),
            ("package.json", "npm Agent Plugin"),
        ]
        var diagnostics = initialDiagnostics
        var foundCandidate = foundInitialCandidate
        for (path, formatName) in candidates {
            let url = root.appendingPathComponent(path)
            guard FileManager.default.fileExists(atPath: url.path) else { continue }
            foundCandidate = true
            do {
                let manifest = try readJSONObject(url)
                guard let rawName = manifest["name"] as? String,
                      let rawVersion = manifest["version"] as? String else {
                    throw NativeConnectorError.pluginInstallation("必须声明 name 和 version")
                }
                let name = try normalizedIdentifier(rawName, field: "name")
                let version = rawVersion.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !version.isEmpty, version.count <= 128 else {
                    throw NativeConnectorError.pluginInstallation("Plugin version 无效")
                }
                let description = (manifest["description"] as? String ?? "")
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return .init(
                    name: name,
                    version: version,
                    description: String(description.prefix(2_000)),
                    displayName: displayName(in: manifest) ?? rawName,
                    formatName: formatName,
                    manifestPath: path,
                    rawManifest: manifest
                )
            } catch {
                diagnostics.append("\(path)：\(error.localizedDescription)")
            }
        }

        if foundCandidate {
            throw NativeConnectorError.pluginInstallation(
                "所有 Plugin 身份清单均无效：\n" + diagnostics.joined(separator: "\n")
            )
        }

        let hasMCP = mcpConfigurationCandidates.contains {
            FileManager.default.fileExists(atPath: root.appendingPathComponent($0.path).path)
        }
        let hasStandaloneSkill = FileManager.default.fileExists(
            atPath: root.appendingPathComponent("SKILL.md").path
        )
        guard hasMCP || hasStandaloneSkill else {
            throw NativeConnectorError.pluginInstallation(
                "无法识别该目录；需要 Plugin 清单、SKILL.md 或受支持的 MCP 配置"
            )
        }
        let rawName = root.lastPathComponent.isEmpty ? "local-mcp" : root.lastPathComponent
        return .init(
            name: try normalizedIdentifier(rawName, field: "folder name"),
            version: "0.0.0-local",
            description: "Imported standalone MCP configuration",
            displayName: rawName,
            formatName: "Standalone MCP",
            manifestPath: nil,
            rawManifest: nil
        )
    }

    private static func discoverHooks(
        at root: URL,
        inlineManifest: [String: Any]?
    ) throws -> [[String: Any]] {
        guard let raw = inlineManifest?["hooks"] else { return [] }
        let paths: [String]
        if let path = raw as? String {
            paths = [path]
        } else if let values = raw as? [Any],
                  values.allSatisfy({ $0 is String }) {
            paths = values.compactMap { $0 as? String }
        } else {
            throw NativeConnectorError.pluginInstallation(
                "Codex Plugin hooks 必须是路径字符串或路径数组"
            )
        }
        guard !paths.isEmpty, paths.count <= 16 else {
            throw NativeConnectorError.pluginInstallation("Codex Hook Bundle 数量无效")
        }
        var componentKeys = Set<String>()
        return try paths.enumerated().map { index, rawPath in
            var path = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
            while path.hasPrefix("./") { path.removeFirst(2) }
            guard !path.isEmpty,
                  !path.hasPrefix("/"),
                  !path.contains("\0"),
                  !path.split(separator: "/").contains("..") else {
                throw NativeConnectorError.pluginInstallation("Hook path 必须位于插件目录内")
            }
            let source = root.appendingPathComponent(path).standardizedFileURL
            let rootResolved = root.standardizedFileURL.resolvingSymlinksInPath()
            let sourceResolved = source.resolvingSymlinksInPath()
            let values = try source.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            guard sourceResolved.path.hasPrefix(rootResolved.path + "/"),
                  values.isRegularFile == true,
                  values.isSymbolicLink != true else {
                throw NativeConnectorError.pluginInstallation(
                    "Hook 文件不存在或发生符号链接逃逸：\(rawPath)"
                )
            }
            let rawComponent = source.deletingPathExtension().lastPathComponent
            let base = try normalizedIdentifier(
                rawComponent.isEmpty ? "lifecycle-hooks-\(index)" : rawComponent,
                field: "Hook component key"
            )
            guard componentKeys.insert(base).inserted else {
                throw NativeConnectorError.pluginInstallation("Hook component key 重复：\(base)")
            }
            return [
                "componentKey": base,
                "dialect": "codex",
                "path": path,
            ]
        }
    }

    private static func discoverSkills(at root: URL, identityName: String) throws -> [String] {
        let rootSkill = root.appendingPathComponent("SKILL.md")
        if FileManager.default.fileExists(atPath: rootSkill.path) {
            let collectionName = try standaloneSkillName(
                at: rootSkill,
                fallback: identityName
            )
            let collection = root
                .appendingPathComponent("skills", isDirectory: true)
                .appendingPathComponent(collectionName, isDirectory: true)
            try FileManager.default.createDirectory(
                at: collection.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            guard !FileManager.default.fileExists(atPath: collection.path) else {
                throw NativeConnectorError.pluginInstallation(
                    "根目录 SKILL.md 与 skills/\(collectionName) 冲突"
                )
            }
            try FileManager.default.createDirectory(at: collection, withIntermediateDirectories: true)
            try FileManager.default.copyItem(
                at: rootSkill,
                to: collection.appendingPathComponent("SKILL.md")
            )
            for resourceDirectory in ["scripts", "references", "assets"] {
                let source = root.appendingPathComponent(resourceDirectory, isDirectory: true)
                var isDirectory: ObjCBool = false
                guard FileManager.default.fileExists(atPath: source.path, isDirectory: &isDirectory),
                      isDirectory.boolValue else { continue }
                try FileManager.default.copyItem(
                    at: source,
                    to: collection.appendingPathComponent(resourceDirectory, isDirectory: true)
                )
            }
        }
        let skillsRoot = root.appendingPathComponent("skills", isDirectory: true)
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: skillsRoot.path, isDirectory: &isDirectory),
              isDirectory.boolValue else { return [] }
        let children = try FileManager.default.contentsOfDirectory(
            at: skillsRoot,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
            options: [.skipsHiddenFiles]
        )
        return try children.compactMap { child in
            let values = try child.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values.isDirectory == true,
                  values.isSymbolicLink != true,
                  FileManager.default.fileExists(
                    atPath: child.appendingPathComponent("SKILL.md").path
                  ) else { return nil }
            _ = try normalizedIdentifier(child.lastPathComponent, field: "skill name")
            return "skills/\(child.lastPathComponent)"
        }.sorted()
    }

    private static func standaloneSkillName(at url: URL, fallback: String) throws -> String {
        let data = try NativeBoundedFileReader.read(url, maximumBytes: 256 * 1_024)
        guard let text = String(data: data, encoding: .utf8) else {
            throw NativeConnectorError.pluginInstallation("SKILL.md 必须是 UTF-8 文本")
        }
        let frontmatter = text
            .replacingOccurrences(of: "\r\n", with: "\n")
            .split(separator: "\n---\n", maxSplits: 1, omittingEmptySubsequences: false)
            .first
            .map(String.init) ?? ""
        let declaredName = frontmatter.split(separator: "\n").compactMap { line -> String? in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard trimmed.hasPrefix("name:") else { return nil }
            var value = String(trimmed.dropFirst("name:".count))
                .trimmingCharacters(in: .whitespaces)
            if value.count >= 2,
               (value.hasPrefix("\"") && value.hasSuffix("\"")
                || value.hasPrefix("'") && value.hasSuffix("'")) {
                value = String(value.dropFirst().dropLast())
            }
            return value
        }.first
        return try normalizedIdentifier(declaredName ?? fallback, field: "skill name")
    }

    private static func discoverMCPServers(
        at root: URL,
        inlineManifest: [String: Any]?
    ) throws -> [String: Any] {
        var rawServers: [String: Any]?
        for candidate in mcpConfigurationCandidates {
            let file = candidate.path
            let url = root.appendingPathComponent(file)
            guard FileManager.default.fileExists(atPath: url.path) else { continue }
            let object: [String: Any]
            do {
                object = try readJSONObject(url)
            } catch {
                if candidate.isDedicatedMCPConfiguration { throw error }
                continue
            }
            let declaredServers = object["mcpServers"] ?? object["servers"] ?? object["mcp"]
            guard let declaredServers else {
                if candidate.isDedicatedMCPConfiguration {
                    throw NativeConnectorError.pluginInstallation(
                        "\(file) 缺少 mcpServers 或 servers 对象"
                    )
                }
                continue
            }
            guard let servers = declaredServers as? [String: Any] else {
                throw NativeConnectorError.pluginInstallation("\(file) 缺少 mcpServers 或 servers 对象")
            }
            rawServers = servers
            break
        }
        if rawServers == nil {
            rawServers = inlineManifest?["mcpServers"] as? [String: Any]
        }
        guard let rawServers else { return [:] }
        guard rawServers.count <= 64 else {
            throw NativeConnectorError.pluginInstallation("MCP Server 数量超过限制")
        }

        var result: [String: Any] = [:]
        for rawKey in rawServers.keys.sorted() {
            guard let server = rawServers[rawKey] as? [String: Any] else {
                throw NativeConnectorError.pluginInstallation("MCP \(rawKey) 配置必须是对象")
            }
            let declaredType = (server["type"] as? String ?? "stdio").lowercased()
            let type = declaredType == "local" ? "stdio" : declaredType
            guard type == "stdio" else {
                throw NativeConnectorError.pluginInstallation(
                    "MCP \(rawKey) 使用 \(type) 传输；当前本地导入只接受 stdio MCP"
                )
            }
            let commandAndArguments = try commandAndArguments(server, serverName: rawKey)
            guard let rawCommand = commandAndArguments.first else {
                throw NativeConnectorError.pluginInstallation("MCP \(rawKey) 缺少 command")
            }
            let componentKey = try uniqueComponentKey(rawKey, existing: Set(result.keys))
            let launcherName = "compat-\(componentKey)"
            try writeLauncher(
                command: rawCommand,
                named: launcherName,
                packageRoot: root
            )
            let arguments = Array(commandAndArguments.dropFirst())
                + (try stringArray(server["args"], field: "\(rawKey).args"))
            let environment = try stringDictionary(
                server["env"] ?? server["environment"],
                field: "\(rawKey).env"
            )
            result[componentKey] = [
                "type": "stdio",
                "bin": launcherName,
                "args": arguments,
                "env": environment,
                "agent": [
                    "displayName": rawKey,
                    "description": (server["description"] as? String ?? "Imported MCP server"),
                ],
            ]
        }
        return result
    }

    private struct MCPConfigurationCandidate {
        let path: String
        let isDedicatedMCPConfiguration: Bool
    }

    private static let mcpConfigurationCandidates = [
        MCPConfigurationCandidate(path: "mcp.json", isDedicatedMCPConfiguration: true),
        MCPConfigurationCandidate(path: ".mcp.json", isDedicatedMCPConfiguration: true),
        MCPConfigurationCandidate(path: ".cursor/mcp.json", isDedicatedMCPConfiguration: true),
        MCPConfigurationCandidate(path: ".vscode/mcp.json", isDedicatedMCPConfiguration: true),
        MCPConfigurationCandidate(path: ".gemini/settings.json", isDedicatedMCPConfiguration: false),
        MCPConfigurationCandidate(path: "claude_desktop_config.json", isDedicatedMCPConfiguration: true),
        MCPConfigurationCandidate(path: "mcp_config.json", isDedicatedMCPConfiguration: true),
        MCPConfigurationCandidate(path: "cline_mcp_settings.json", isDedicatedMCPConfiguration: true),
        MCPConfigurationCandidate(path: "opencode.json", isDedicatedMCPConfiguration: false),
    ]

    private static func commandAndArguments(
        _ server: [String: Any],
        serverName: String
    ) throws -> [String] {
        if let command = server["command"] as? String { return [command] }
        if let command = server["command"] as? [Any] {
            let values = try stringArray(command, field: "\(serverName).command")
            guard !values.isEmpty else {
                throw NativeConnectorError.pluginInstallation("MCP \(serverName) command 不能为空")
            }
            return values
        }
        return []
    }

    private static func writeLauncher(
        command rawCommand: String,
        named name: String,
        packageRoot: URL
    ) throws {
        let command = rawCommand.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !command.isEmpty, command.count <= 2_048, !command.contains("\0") else {
            throw NativeConnectorError.pluginInstallation("MCP command 无效")
        }
        let executable: String
        if command.range(of: "^[A-Za-z0-9._+-]{1,128}$", options: .regularExpression) != nil {
            executable = "exec /usr/bin/env '\(command)' \"$@\""
        } else {
            let relative = normalizedPluginCommandPath(command)
            let target = packageRoot.appendingPathComponent(relative).standardizedFileURL
            let prefix = packageRoot.standardizedFileURL.path + "/"
            let values = try target.resourceValues(forKeys: [
                .isRegularFileKey, .isSymbolicLinkKey, .isExecutableKey,
            ])
            guard target.path.hasPrefix(prefix),
                  values.isRegularFile == true,
                  values.isSymbolicLink != true,
                  values.isExecutable == true else {
                throw NativeConnectorError.pluginInstallation(
                    "MCP command 必须是 PATH 中的命令，或插件目录内可执行文件：\(command)"
                )
            }
            executable = "exec \"$CHATOS_PLUGIN_ROOT/\(relative)\" \"$@\""
        }
        let bin = packageRoot.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let launcher = bin.appendingPathComponent(name)
        try Data("#!/bin/sh\nset -eu\n\(executable)\n".utf8)
            .write(to: launcher, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: launcher.path
        )
    }

    private static func normalizedPluginCommandPath(_ raw: String) -> String {
        var value = raw
        for prefix in [
            "${PLUGIN_ROOT}/", "$PLUGIN_ROOT/",
            "${CLAUDE_PLUGIN_ROOT}/", "$CLAUDE_PLUGIN_ROOT/",
        ] where value.hasPrefix(prefix) {
            value = String(value.dropFirst(prefix.count))
            break
        }
        while value.hasPrefix("./") { value.removeFirst(2) }
        return value
    }

    private static func displayName(in manifest: [String: Any]) -> String? {
        if let extensions = manifest["extensions"] as? [String: Any],
           let openAI = extensions["com.openai"] as? [String: Any],
           let interface = openAI["interface"] as? [String: Any],
           let value = interface["displayName"] as? String,
           !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return value
        }
        if let interface = manifest["interface"] as? [String: Any],
           let value = interface["displayName"] as? String,
           !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return value
        }
        return manifest["displayName"] as? String
    }

    private static func normalizedIdentifier(_ raw: String, field: String) throws -> String {
        let lowered = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let mapped = lowered.map { character -> Character in
            character.isLetter || character.isNumber || character == "." || character == "_" || character == "-"
                ? character : "-"
        }
        let value = String(mapped).trimmingCharacters(in: CharacterSet(charactersIn: "-._"))
        guard !value.isEmpty, value.count <= 128,
              value.range(of: "^[a-z0-9][a-z0-9._-]{0,127}$", options: .regularExpression) != nil else {
            throw NativeConnectorError.pluginInstallation("\(field) 无法转换为安全标识")
        }
        return value
    }

    private static func uniqueComponentKey(
        _ raw: String,
        existing: Set<String>
    ) throws -> String {
        let base = try normalizedIdentifier(raw, field: "MCP Server name")
        guard !existing.contains(base) else {
            throw NativeConnectorError.pluginInstallation("MCP Server 名称转换后重复：\(raw)")
        }
        return base
    }

    private static func stringArray(_ raw: Any?, field: String) throws -> [String] {
        guard let raw else { return [] }
        guard let values = raw as? [Any], values.count <= 64,
              values.allSatisfy({ $0 is String }) else {
            throw NativeConnectorError.pluginInstallation("\(field) 必须是字符串数组")
        }
        let result = values.compactMap { $0 as? String }
        guard result.allSatisfy({ !$0.contains("\0") && $0.count <= 2_048 }) else {
            throw NativeConnectorError.pluginInstallation("\(field) 包含无效参数")
        }
        return result
    }

    private static func stringDictionary(_ raw: Any?, field: String) throws -> [String: String] {
        guard let raw else { return [:] }
        guard let values = raw as? [String: Any], values.count <= 64,
              values.values.allSatisfy({ $0 is String }) else {
            throw NativeConnectorError.pluginInstallation("\(field) 必须是字符串对象")
        }
        var result: [String: String] = [:]
        for (key, rawValue) in values {
            guard let value = rawValue as? String,
                  key.range(of: "^[A-Za-z_][A-Za-z0-9_]{0,127}$", options: .regularExpression) != nil,
                  !value.contains("\0"), value.count <= 8_192 else {
                throw NativeConnectorError.pluginInstallation("\(field) 包含无效环境变量")
            }
            result[key] = value
        }
        return result
    }

    private static func readJSONObject(_ url: URL) throws -> [String: Any] {
        let data = try NativeBoundedFileReader.read(url, maximumBytes: 2 * 1_024 * 1_024)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw NativeConnectorError.pluginInstallation("\(url.lastPathComponent) 必须是 JSON 对象")
        }
        return object
    }
}
