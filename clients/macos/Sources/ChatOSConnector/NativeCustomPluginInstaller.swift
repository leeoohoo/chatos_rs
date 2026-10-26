import ChatOSCore
import CryptoKit
import Foundation

struct NativePreparedCustomPlugin: Sendable {
    var record: NativeInstalledPluginRecord
    var temporaryRootURL: URL
    var sourceSummary: String
}

struct NativeCommittedCustomPlugin: Sendable {
    var record: NativeInstalledPluginRecord
    var pluginDirectoryURL: URL
    var backupURL: URL?
    var temporaryRootURL: URL
}

enum NativeCustomMCPAccessPolicy {
    struct Permission: Sendable, Equatable {
        var id: String
        var reason: String
    }

    static func permissions(
        for profile: LocalConnectorCustomMCPDraft.AccessProfile
    ) -> [Permission] {
        let process = Permission(
            id: "process.spawn",
            reason: "启动用户确认安装的本地 MCP Server"
        )
        switch profile {
        case .restricted:
            return [process]
        case .networkClient:
            return [
                process,
                .init(id: "network.client", reason: "允许 MCP 发起出站网络请求"),
            ]
        case .developerAnalysis:
            return [
                process,
                .init(id: "network.client", reason: "允许分析工具发起出站网络请求"),
                .init(id: "system.developer-tools.read", reason: "读取并调用本机开发工具链"),
                .init(id: "browser.managed.launch", reason: "启动隔离的本机浏览器进行动态分析"),
                .init(id: "artifact.create", reason: "在 ChatOS 管理目录中保存分析证据"),
                .init(id: "workspace.read", reason: "读取当前任务绑定的项目文件"),
            ]
        }
    }
}

extension NativePluginInstaller {
    func prepareCustomMCP(
        _ draft: LocalConnectorCustomMCPDraft
    ) async throws -> NativePreparedCustomPlugin {
        let input = try NativeCustomPluginInput(draft: draft)
        let stagingRoot = try makeCustomStagingRoot()
        do {
            let packageRoot = stagingRoot.appendingPathComponent("package", isDirectory: true)
            try FileManager.default.createDirectory(at: packageRoot, withIntermediateDirectories: true)
            let packageJSON: [String: Any] = [
                "name": "chatos-custom-mcp-host",
                "private": true,
                "version": "1.0.0",
                "dependencies": [input.packageName: input.packageVersion],
            ]
            try JSONSerialization.data(withJSONObject: packageJSON, options: [.prettyPrinted, .sortedKeys])
                .write(to: packageRoot.appendingPathComponent("package.json"), options: .atomic)

            try await npmInstall(packageRoot)
            try removeSymbolicLinks(in: packageRoot)

            let dependencyRoot = packageRoot
                .appendingPathComponent("node_modules", isDirectory: true)
                .appendingPathComponent(input.packageName, isDirectory: true)
                .standardizedFileURL
            let dependencyManifestURL = dependencyRoot.appendingPathComponent("package.json")
            let dependencyManifest = try Self.readJSONObject(dependencyManifestURL)
            guard dependencyManifest["name"] as? String == input.packageName,
                  dependencyManifest["version"] as? String == input.packageVersion else {
                throw NativeConnectorError.pluginInstallation("npm 安装结果与固定的包名和版本不一致")
            }
            let binaryPath = try Self.binaryPath(
                named: input.binaryName,
                packageManifest: dependencyManifest
            )
            let entryURL = dependencyRoot.appendingPathComponent(binaryPath).standardizedFileURL
            let dependencyPrefix = dependencyRoot.path + "/"
            let values = try entryURL.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            guard entryURL.path.hasPrefix(dependencyPrefix),
                  values.isRegularFile == true,
                  values.isSymbolicLink != true else {
                throw NativeConnectorError.pluginInstallation("npm MCP 的可执行入口无效")
            }

            let launcherURL = packageRoot
                .appendingPathComponent("bin", isDirectory: true)
                .appendingPathComponent(input.componentKey)
            try FileManager.default.createDirectory(
                at: launcherURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let relativeEntry = entryURL.path.replacingOccurrences(
                of: packageRoot.path + "/",
                with: ""
            )
            let launcher = """
            #!/bin/sh
            exec /usr/bin/env node "$CHATOS_PLUGIN_ROOT/\(relativeEntry)" "$@"
            """
            try Data(launcher.utf8).write(to: launcherURL, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: launcherURL.path)

            let manifest: [String: Any] = [
                "schemaVersion": 3,
                "name": input.packageName,
                "version": input.packageVersion,
                "description": input.description,
                "interface": ["displayName": input.displayName],
                "mcpServers": [
                    input.componentKey: [
                        "type": "stdio",
                        "bin": input.componentKey,
                        "args": input.arguments,
                        "env": input.environment,
                        "agent": [
                            "displayName": input.displayName,
                            "description": input.description,
                        ],
                    ],
                ],
                "permissions": NativeCustomMCPAccessPolicy.permissions(
                    for: input.accessProfile
                ).map { permission in
                    [
                        "permission": permission.id,
                        "required": true,
                        "reason": permission.reason,
                        "components": [input.componentKey],
                    ] as [String: Any]
                },
            ]
            try JSONSerialization.data(withJSONObject: manifest, options: [.prettyPrinted, .sortedKeys])
                .write(to: packageRoot.appendingPathComponent("chatos.plugin.json"), options: .atomic)
            return try preparedCustomPlugin(
                packageRoot: packageRoot,
                temporaryRootURL: stagingRoot,
                pluginIDSeed: "npm:\(input.packageName):\(input.componentKey)",
                source: .customMCP,
                sourceSummary: "npm \(input.packageName)@\(input.packageVersion) · \(input.binaryName) \(input.arguments.joined(separator: " ")) · \(input.accessProfile.rawValue)"
            )
        } catch {
            try? FileManager.default.removeItem(at: stagingRoot)
            throw error
        }
    }

    func prepareLocalPlugin(atPath path: String) throws -> NativePreparedCustomPlugin {
        let sourceURL = URL(fileURLWithPath: path, isDirectory: true)
            .standardizedFileURL.resolvingSymlinksInPath()
        var isDirectory: ObjCBool = false
        guard sourceURL.path.hasPrefix("/"),
              FileManager.default.fileExists(atPath: sourceURL.path, isDirectory: &isDirectory),
              isDirectory.boolValue,
              !rootURL.standardizedFileURL.path.hasPrefix(sourceURL.path + "/") else {
            throw NativeConnectorError.pluginInstallation("请选择一个存在的 Plugin 文件夹")
        }
        let stagingRoot = try makeCustomStagingRoot()
        do {
            let packageRoot = stagingRoot.appendingPathComponent("package", isDirectory: true)
            try FileManager.default.copyItem(at: sourceURL, to: packageRoot)
            let imported = try NativeAgentPluginCompatibilityImporter.normalizePackage(
                at: packageRoot
            )
            let manifest = imported.manifest
            guard manifest.schemaVersion == 3,
                  !manifest.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !manifest.version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !manifest.mcpServers.isEmpty || !manifest.skills.isEmpty || !manifest.hooks.isEmpty,
                  manifest.mcpServers.values.allSatisfy({ $0.type == "stdio" }) else {
                throw NativeConnectorError.pluginInstallation(
                    "导入目录必须至少包含一个有效 Skill、stdio MCP 或 Hook"
                )
            }
            return try preparedCustomPlugin(
                packageRoot: packageRoot,
                temporaryRootURL: stagingRoot,
                pluginIDSeed: "folder:\(sourceURL.path):\(manifest.name)",
                source: .localPlugin,
                sourceSummary: "\(imported.formatName) · \(sourceURL.path)"
            )
        } catch {
            try? FileManager.default.removeItem(at: stagingRoot)
            throw error
        }
    }

    func preparePluginSource(
        _ draft: LocalConnectorPluginSourceDraft
    ) async throws -> NativePreparedCustomPlugin {
        let input = try NativePluginSourceInput(draft: draft)
        if input.kind == .localFolder, input.subdirectory == nil {
            return try prepareLocalPlugin(atPath: input.location)
        }
        let stagingRoot = try makeCustomStagingRoot()
        do {
            let acquiredRoot = stagingRoot.appendingPathComponent("acquired", isDirectory: true)
            let source: LocalConnectorPluginSource
            let summary: String
            let identity: String
            switch input.kind {
            case .localFolder:
                let local = URL(fileURLWithPath: input.location, isDirectory: true)
                    .standardizedFileURL.resolvingSymlinksInPath()
                try Self.requireDirectory(local, message: "请选择一个存在的 Plugin 文件夹")
                guard !rootURL.standardizedFileURL.path.hasPrefix(local.path + "/") else {
                    throw NativeConnectorError.pluginInstallation(
                        "Plugin 来源目录不能包含 ChatOS 的插件安装目录"
                    )
                }
                try FileManager.default.copyItem(at: local, to: acquiredRoot)
                source = .localPlugin
                summary = "Local folder · \(input.location)"
                identity = "folder:\(local.path)"
            case .gitRepository:
                try FileManager.default.createDirectory(at: acquiredRoot, withIntermediateDirectories: true)
                let reference = input.gitReference ?? "HEAD"
                try await Self.cloneGitRepository(
                    url: input.location,
                    reference: reference,
                    into: acquiredRoot
                )
                try? FileManager.default.removeItem(
                    at: acquiredRoot.appendingPathComponent(".git", isDirectory: true)
                )
                source = .gitPlugin
                summary = "Git · \(input.location) · \(reference)"
                identity = "git:\(input.location):\(reference)"
            case .npmPackage:
                let version = input.exactVersion!
                let installRoot = stagingRoot.appendingPathComponent("npm", isDirectory: true)
                try FileManager.default.createDirectory(at: installRoot, withIntermediateDirectories: true)
                let wrapper: [String: Any] = [
                    "name": "chatos-agent-plugin-source",
                    "private": true,
                    "version": "1.0.0",
                    "dependencies": [input.location: version],
                ]
                try JSONSerialization.data(withJSONObject: wrapper, options: [.prettyPrinted, .sortedKeys])
                    .write(to: installRoot.appendingPathComponent("package.json"), options: .atomic)
                try await Self.runInstallerProcess(
                    executableURL: try Self.resolveExecutable(named: "npm"),
                    arguments: [
                        "install", "--ignore-scripts", "--omit=dev", "--no-audit", "--no-fund",
                        "--save-exact", "--package-lock=true",
                    ],
                    currentDirectoryURL: installRoot
                )
                let dependencyRoot = installRoot
                    .appendingPathComponent("node_modules", isDirectory: true)
                    .appendingPathComponent(input.location, isDirectory: true)
                    .standardizedFileURL
                let metadata = try Self.readJSONObject(
                    dependencyRoot.appendingPathComponent("package.json")
                )
                guard metadata["name"] as? String == input.location,
                      metadata["version"] as? String == version else {
                    throw NativeConnectorError.pluginInstallation("npm Plugin 安装结果与固定版本不一致")
                }
                try FileManager.default.copyItem(at: dependencyRoot, to: acquiredRoot)
                let dependencies = installRoot.appendingPathComponent("node_modules", isDirectory: true)
                try FileManager.default.copyItem(
                    at: dependencies,
                    to: acquiredRoot.appendingPathComponent("node_modules", isDirectory: true)
                )
                source = .npmPlugin
                summary = "npm Agent Plugin · \(input.location)@\(version)"
                identity = "npm-plugin:\(input.location):\(version)"
            }

            try removeSymbolicLinks(in: acquiredRoot)
            let selectedRoot = try Self.selectedPluginRoot(
                acquiredRoot: acquiredRoot,
                subdirectory: input.subdirectory
            )
            let packageRoot = stagingRoot.appendingPathComponent("package", isDirectory: true)
            try FileManager.default.copyItem(at: selectedRoot, to: packageRoot)
            let imported = try NativeAgentPluginCompatibilityImporter.normalizePackage(at: packageRoot)
            let manifest = imported.manifest
            guard manifest.schemaVersion == 3,
                  !manifest.mcpServers.isEmpty || !manifest.skills.isEmpty || !manifest.hooks.isEmpty,
                  manifest.mcpServers.values.allSatisfy({ $0.type == "stdio" }) else {
                throw NativeConnectorError.pluginInstallation(
                    "该来源没有可安装的 Skill、stdio MCP 或 Hook"
                )
            }
            let suffix = input.subdirectory.map { " · \($0)" } ?? ""
            return try preparedCustomPlugin(
                packageRoot: packageRoot,
                temporaryRootURL: stagingRoot,
                pluginIDSeed: "\(identity):\(input.subdirectory ?? ""):\(manifest.name)",
                source: source,
                sourceSummary: "\(imported.formatName) · \(summary)\(suffix)"
            )
        } catch {
            try? FileManager.default.removeItem(at: stagingRoot)
            throw error
        }
    }

    func committedCustomPluginRecord(
        _ prepared: NativePreparedCustomPlugin
    ) -> NativeInstalledPluginRecord {
        var record = prepared.record
        let pluginDirectory = rootURL
            .appendingPathComponent(Self.sha256Hex(record.pluginID), isDirectory: true)
        let finalURL = pluginDirectory.appendingPathComponent(record.version, isDirectory: true)
        record.installationPath = finalURL.path
        return record
    }

    func commitCustomPlugin(
        _ prepared: NativePreparedCustomPlugin
    ) throws -> NativeCommittedCustomPlugin {
        let record = committedCustomPluginRecord(prepared)
        let pluginDirectory = URL(
            fileURLWithPath: record.installationPath,
            isDirectory: true
        ).deletingLastPathComponent()
        let backupURL = rootURL.appendingPathComponent(
            ".backup-custom-\(pluginDirectory.lastPathComponent)-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        let hadPrevious = FileManager.default.fileExists(atPath: pluginDirectory.path)
        if hadPrevious { try FileManager.default.moveItem(at: pluginDirectory, to: backupURL) }
        do {
            try FileManager.default.createDirectory(at: pluginDirectory, withIntermediateDirectories: true)
            try FileManager.default.moveItem(
                at: URL(fileURLWithPath: prepared.record.installationPath, isDirectory: true),
                to: URL(fileURLWithPath: record.installationPath, isDirectory: true)
            )
            return NativeCommittedCustomPlugin(
                record: record,
                pluginDirectoryURL: pluginDirectory,
                backupURL: hadPrevious ? backupURL : nil,
                temporaryRootURL: prepared.temporaryRootURL
            )
        } catch {
            try? FileManager.default.removeItem(at: pluginDirectory)
            if hadPrevious { try? FileManager.default.moveItem(at: backupURL, to: pluginDirectory) }
            try? FileManager.default.removeItem(at: prepared.temporaryRootURL)
            throw error
        }
    }

    func finalizeCustomPluginCommit(_ commit: NativeCommittedCustomPlugin) throws {
        if let backupURL = commit.backupURL,
           FileManager.default.fileExists(atPath: backupURL.path) {
            try FileManager.default.removeItem(at: backupURL)
        }
        if FileManager.default.fileExists(atPath: commit.temporaryRootURL.path) {
            try FileManager.default.removeItem(at: commit.temporaryRootURL)
        }
    }

    func rollbackCustomPluginCommit(_ commit: NativeCommittedCustomPlugin) throws {
        if FileManager.default.fileExists(atPath: commit.pluginDirectoryURL.path) {
            try FileManager.default.removeItem(at: commit.pluginDirectoryURL)
        }
        if let backupURL = commit.backupURL,
           FileManager.default.fileExists(atPath: backupURL.path) {
            try FileManager.default.moveItem(at: backupURL, to: commit.pluginDirectoryURL)
        }
        if FileManager.default.fileExists(atPath: commit.temporaryRootURL.path) {
            try FileManager.default.removeItem(at: commit.temporaryRootURL)
        }
    }

    func recoverCustomPluginTransactions(
        referencedInstallationPaths: [String]
    ) throws {
        guard FileManager.default.fileExists(atPath: rootURL.path) else { return }
        let referencesByDirectory = Dictionary(
            uniqueKeysWithValues: referencedInstallationPaths.compactMap { path -> (String, URL)? in
                let installationURL = URL(fileURLWithPath: path, isDirectory: true)
                    .standardizedFileURL
                let pluginDirectory = installationURL.deletingLastPathComponent()
                guard pluginDirectory.deletingLastPathComponent() == rootURL.standardizedFileURL else {
                    return nil
                }
                return (pluginDirectory.lastPathComponent, installationURL)
            }
        )
        let children = try FileManager.default.contentsOfDirectory(
            at: rootURL,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
            options: [.skipsSubdirectoryDescendants]
        )
        let backupPrefix = ".backup-custom-"
        for backupURL in children where backupURL.lastPathComponent.hasPrefix(backupPrefix) {
            let suffix = String(backupURL.lastPathComponent.dropFirst(backupPrefix.count))
            guard suffix.count > 65 else { continue }
            let separator = suffix.index(suffix.startIndex, offsetBy: 64)
            let pluginDirectoryName = String(suffix[..<separator])
            guard suffix[separator] == "-",
                  pluginDirectoryName.allSatisfy(\.isHexDigit) else {
                continue
            }
            let values = try backupURL.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values.isDirectory == true, values.isSymbolicLink != true else { continue }
            let pluginDirectoryURL = rootURL.appendingPathComponent(
                pluginDirectoryName,
                isDirectory: true
            )
            if let referencedURL = referencesByDirectory[pluginDirectoryName],
               !FileManager.default.fileExists(atPath: referencedURL.path) {
                if FileManager.default.fileExists(atPath: pluginDirectoryURL.path) {
                    try FileManager.default.removeItem(at: pluginDirectoryURL)
                }
                try FileManager.default.moveItem(at: backupURL, to: pluginDirectoryURL)
            } else {
                try FileManager.default.removeItem(at: backupURL)
            }
        }
        let stagingRoot = rootURL.appendingPathComponent(".custom-staging", isDirectory: true)
        if FileManager.default.fileExists(atPath: stagingRoot.path) {
            try FileManager.default.removeItem(at: stagingRoot)
        }
    }

    func discardCustomPlugin(_ prepared: NativePreparedCustomPlugin) {
        try? FileManager.default.removeItem(at: prepared.temporaryRootURL)
    }

    private func preparedCustomPlugin(
        packageRoot: URL,
        temporaryRootURL: URL,
        pluginIDSeed: String,
        source: LocalConnectorPluginSource,
        sourceSummary: String
    ) throws -> NativePreparedCustomPlugin {
        let manifest = try NativePluginManifestLoader.loadManifest(
            from: packageRoot.appendingPathComponent("chatos.plugin.json")
        )
        let files = try NativePluginInstallationIntegrity.snapshot(
            installationURL: packageRoot,
            maximumFiles: maximumFiles,
            maximumBytes: maximumUnpackedBytes
        )
        let artifact = Self.snapshotFingerprint(files)
        let pluginID = "local-\(Self.sha256Hex(pluginIDSeed).prefix(24))"
        return .init(
            record: .init(
                pluginID: pluginID,
                releaseID: "local-\(artifact.prefix(24))",
                version: manifest.version,
                artifactSHA256: artifact,
                installationPath: packageRoot.path,
                installedAt: ISO8601DateFormatter().string(from: Date()),
                pluginKey: "\(manifest.name)@local",
                packageFileSHA256: files,
                source: source,
                sourceSummary: sourceSummary
            ),
            temporaryRootURL: temporaryRootURL,
            sourceSummary: sourceSummary
        )
    }

    private func makeCustomStagingRoot() throws -> URL {
        let value = rootURL
            .appendingPathComponent(".custom-staging", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: value, withIntermediateDirectories: true)
        return value
    }

    private func removeSymbolicLinks(in root: URL) throws {
        guard let enumerator = FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: [.isSymbolicLinkKey],
            options: []
        ) else { return }
        var links: [URL] = []
        for case let fileURL as URL in enumerator {
            if try fileURL.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink == true {
                links.append(fileURL)
                enumerator.skipDescendants()
            }
        }
        for link in links { try FileManager.default.removeItem(at: link) }
    }

    private static func requireDirectory(_ url: URL, message: String) throws {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw NativeConnectorError.pluginInstallation(message)
        }
    }

    private static func selectedPluginRoot(
        acquiredRoot: URL,
        subdirectory: String?
    ) throws -> URL {
        guard let subdirectory else { return acquiredRoot }
        let selected = acquiredRoot.appendingPathComponent(subdirectory, isDirectory: true)
            .standardizedFileURL
        guard selected.path.hasPrefix(acquiredRoot.standardizedFileURL.path + "/") else {
            throw NativeConnectorError.pluginInstallation("Plugin 子目录超出来源根目录")
        }
        try requireDirectory(selected, message: "Plugin 子目录不存在：\(subdirectory)")
        let values = try selected.resourceValues(forKeys: [.isSymbolicLinkKey])
        guard values.isSymbolicLink != true else {
            throw NativeConnectorError.pluginInstallation("Plugin 子目录不能是符号链接")
        }
        return selected
    }

    private static func cloneGitRepository(
        url: String,
        reference: String,
        into destination: URL
    ) async throws {
        try await NativeGitProcess.performBlockingWork { controller in
            _ = try NativeGitProcess.run(
                arguments: ["init", "--quiet"],
                directory: destination,
                controller: controller
            )
            _ = try NativeGitProcess.run(
                arguments: ["remote", "add", "origin", url],
                directory: destination,
                controller: controller
            )
            _ = try NativeGitProcess.run(
                arguments: [
                    "-c", "core.hooksPath=/dev/null", "-c", "protocol.file.allow=never",
                    "fetch", "--quiet", "--depth", "1", "--no-tags", "origin", reference,
                ],
                directory: destination,
                timeout: 5 * 60,
                controller: controller
            )
            _ = try NativeGitProcess.run(
                arguments: [
                    "-c", "core.hooksPath=/dev/null", "checkout", "--quiet", "--detach", "FETCH_HEAD",
                ],
                directory: destination,
                controller: controller
            )
        }
    }

    private static func binaryPath(
        named name: String,
        packageManifest: [String: Any]
    ) throws -> String {
        if let value = packageManifest["bin"] as? String, !value.isEmpty { return value }
        if let values = packageManifest["bin"] as? [String: Any],
           let value = values[name] as? String, !value.isEmpty { return value }
        throw NativeConnectorError.pluginInstallation("npm package 没有声明名为 \(name) 的 bin")
    }

    private static func readJSONObject(_ url: URL) throws -> [String: Any] {
        let data = try NativeBoundedFileReader.read(url, maximumBytes: 2 * 1_024 * 1_024)
        guard let value = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw NativeConnectorError.pluginInstallation("package.json 格式无效")
        }
        return value
    }

    static func resolveExecutable(named name: String) throws -> URL {
        for directory in NativePluginProcessEnvironment.runtimePath().split(separator: ":") {
            let candidate = URL(fileURLWithPath: String(directory), isDirectory: true)
                .appendingPathComponent(name)
            if FileManager.default.isExecutableFile(atPath: candidate.path) { return candidate }
        }
        throw NativeConnectorError.pluginInstallation("没有找到 \(name)，请先安装 Node.js/npm")
    }

    static func snapshotFingerprint(_ snapshot: [String: String]) -> String {
        let canonical = snapshot.keys.sorted().map { "\($0)\u{0}\(snapshot[$0]!)" }.joined(separator: "\n")
        return sha256Hex(canonical)
    }

    static func sha256Hex(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}

private struct NativeCustomPluginInput {
    var displayName: String
    var description: String
    var componentKey: String
    var packageName: String
    var packageVersion: String
    var binaryName: String
    var arguments: [String]
    var environment: [String: String]
    var accessProfile: LocalConnectorCustomMCPDraft.AccessProfile

    init(draft: LocalConnectorCustomMCPDraft) throws {
        displayName = draft.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        description = draft.description.trimmingCharacters(in: .whitespacesAndNewlines)
        componentKey = draft.componentKey.trimmingCharacters(in: .whitespacesAndNewlines)
        packageName = draft.packageName.trimmingCharacters(in: .whitespacesAndNewlines)
        packageVersion = draft.packageVersion.trimmingCharacters(in: .whitespacesAndNewlines)
        binaryName = draft.binaryName.trimmingCharacters(in: .whitespacesAndNewlines)
        arguments = draft.arguments
        environment = draft.environment
        accessProfile = draft.accessProfile
        let safeID = try NSRegularExpression(pattern: "^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
        let safePackage = try NSRegularExpression(pattern: "^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$")
        let exactVersion = try NSRegularExpression(pattern: "^[0-9]+\\.[0-9]+\\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\\+[0-9A-Za-z.-]+)?$")
        func matches(_ regex: NSRegularExpression, _ value: String) -> Bool {
            regex.firstMatch(in: value, range: NSRange(value.startIndex..., in: value)) != nil
        }
        guard !displayName.isEmpty, displayName.count <= 120,
              matches(safeID, componentKey), matches(safeID, binaryName),
              matches(safePackage, packageName), matches(exactVersion, packageVersion),
              arguments.count <= 64,
              arguments.allSatisfy({ !$0.contains("\0") && $0.count <= 2_048 }),
              environment.isEmpty else {
            throw NativeConnectorError.pluginInstallation(
                "自定义 MCP 参数无效；npm 必须使用精确版本，命令参数必须分项填写，当前版本不接受明文环境变量"
            )
        }
    }
}

private struct NativePluginSourceInput {
    var kind: LocalConnectorPluginSourceDraft.Kind
    var location: String
    var exactVersion: String?
    var gitReference: String?
    var subdirectory: String?

    init(draft: LocalConnectorPluginSourceDraft) throws {
        kind = draft.kind
        location = draft.location.trimmingCharacters(in: .whitespacesAndNewlines)
        exactVersion = draft.exactVersion?.trimmingCharacters(in: .whitespacesAndNewlines)
        gitReference = draft.gitReference?.trimmingCharacters(in: .whitespacesAndNewlines)
        subdirectory = draft.subdirectory?.trimmingCharacters(in: .whitespacesAndNewlines)
        if exactVersion?.isEmpty == true { exactVersion = nil }
        if gitReference?.isEmpty == true { gitReference = nil }
        if subdirectory?.isEmpty == true { subdirectory = nil }

        guard !location.isEmpty, location.count <= 4_096, !location.contains("\0") else {
            throw NativeConnectorError.pluginInstallation("Plugin 来源无效")
        }
        if let subdirectory {
            let parts = subdirectory.split(separator: "/", omittingEmptySubsequences: false)
            guard !subdirectory.hasPrefix("/"), subdirectory.count <= 1_024,
                  parts.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }) else {
                throw NativeConnectorError.pluginInstallation("Plugin 子目录必须是安全的相对路径")
            }
        }

        switch kind {
        case .localFolder:
            guard location.hasPrefix("/"), exactVersion == nil, gitReference == nil else {
                throw NativeConnectorError.pluginInstallation("本地 Plugin 来源必须是绝对目录")
            }
        case .gitRepository:
            guard exactVersion == nil,
                  let url = URL(string: location),
                  url.scheme?.lowercased() == "https",
                  url.host != nil,
                  url.user == nil,
                  url.password == nil,
                  url.query == nil,
                  url.fragment == nil,
                  !location.contains("\r"), !location.contains("\n") else {
                throw NativeConnectorError.pluginInstallation(
                    "Git Plugin 目前只接受不含凭据的 HTTPS 仓库 URL"
                )
            }
            if let gitReference {
                guard gitReference.count <= 256,
                      !gitReference.hasPrefix("-"),
                      !gitReference.contains(".."),
                      !gitReference.contains("@{"),
                      gitReference.range(of: "^[A-Za-z0-9][A-Za-z0-9._/-]*$", options: .regularExpression) != nil else {
                    throw NativeConnectorError.pluginInstallation("Git ref 无效")
                }
            }
        case .npmPackage:
            guard gitReference == nil,
                  let exactVersion,
                  location.range(
                    of: "^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$",
                    options: .regularExpression
                  ) != nil,
                  exactVersion.range(
                    of: "^[0-9]+\\.[0-9]+\\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\\+[0-9A-Za-z.-]+)?$",
                    options: .regularExpression
                  ) != nil else {
                throw NativeConnectorError.pluginInstallation(
                    "npm Agent Plugin 必须使用合法包名和精确版本"
                )
            }
        }
    }
}
