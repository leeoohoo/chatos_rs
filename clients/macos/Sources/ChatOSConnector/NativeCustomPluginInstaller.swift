import ChatOSCore
import ChatOSProcessRuntime
import CryptoKit
import Darwin
import Foundation

struct NativePreparedCustomPlugin: Sendable {
    var record: NativeInstalledPluginRecord
    var temporaryRootURL: URL
    var sourceSummary: String
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

            let npmURL = try Self.resolveExecutable(named: "npm")
            try await Self.runInstallerProcess(
                executableURL: npmURL,
                arguments: [
                    "install", "--ignore-scripts", "--omit=dev", "--no-audit", "--no-fund",
                    "--save-exact", "--package-lock=true",
                ],
                currentDirectoryURL: packageRoot
            )
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
                "permissions": [[
                    "permission": "process.spawn",
                    "required": true,
                    "reason": "启动用户确认安装的本地 MCP Server",
                    "components": [input.componentKey],
                ]],
            ]
            try JSONSerialization.data(withJSONObject: manifest, options: [.prettyPrinted, .sortedKeys])
                .write(to: packageRoot.appendingPathComponent("chatos.plugin.json"), options: .atomic)
            return try preparedCustomPlugin(
                packageRoot: packageRoot,
                temporaryRootURL: stagingRoot,
                pluginIDSeed: "npm:\(input.packageName):\(input.componentKey)",
                source: .customMCP,
                sourceSummary: "npm \(input.packageName)@\(input.packageVersion) · \(input.binaryName) \(input.arguments.joined(separator: " "))"
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
              isDirectory.boolValue else {
            throw NativeConnectorError.pluginInstallation("请选择一个存在的 Plugin 文件夹")
        }
        let stagingRoot = try makeCustomStagingRoot()
        do {
            let packageRoot = stagingRoot.appendingPathComponent("package", isDirectory: true)
            try FileManager.default.copyItem(at: sourceURL, to: packageRoot)
            let manifest = try NativePluginManifestLoader.loadManifest(
                from: packageRoot.appendingPathComponent("chatos.plugin.json")
            )
            guard manifest.schemaVersion == 3,
                  !manifest.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !manifest.version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !manifest.mcpServers.isEmpty,
                  manifest.mcpServers.values.allSatisfy({ $0.type == "stdio" }) else {
                throw NativeConnectorError.pluginInstallation("本地 Plugin manifest 必须是 schema v3，并至少声明一个 stdio MCP")
            }
            return try preparedCustomPlugin(
                packageRoot: packageRoot,
                temporaryRootURL: stagingRoot,
                pluginIDSeed: "folder:\(sourceURL.path):\(manifest.name)",
                source: .localPlugin,
                sourceSummary: sourceURL.path
            )
        } catch {
            try? FileManager.default.removeItem(at: stagingRoot)
            throw error
        }
    }

    func commitCustomPlugin(_ prepared: NativePreparedCustomPlugin) throws -> NativeInstalledPluginRecord {
        let record = prepared.record
        let pluginDirectory = rootURL
            .appendingPathComponent(Self.sha256Hex(record.pluginID), isDirectory: true)
        let finalURL = pluginDirectory.appendingPathComponent(record.version, isDirectory: true)
        let backupURL = rootURL.appendingPathComponent(
            ".backup-custom-\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        let hadPrevious = FileManager.default.fileExists(atPath: pluginDirectory.path)
        if hadPrevious { try FileManager.default.moveItem(at: pluginDirectory, to: backupURL) }
        do {
            try FileManager.default.createDirectory(at: pluginDirectory, withIntermediateDirectories: true)
            try FileManager.default.moveItem(
                at: URL(fileURLWithPath: record.installationPath, isDirectory: true),
                to: finalURL
            )
            try? FileManager.default.removeItem(at: prepared.temporaryRootURL)
            if hadPrevious { try? FileManager.default.removeItem(at: backupURL) }
            var committed = record
            committed.installationPath = finalURL.path
            return committed
        } catch {
            try? FileManager.default.removeItem(at: pluginDirectory)
            if hadPrevious { try? FileManager.default.moveItem(at: backupURL, to: pluginDirectory) }
            throw error
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

    private static func resolveExecutable(named name: String) throws -> URL {
        for directory in NativePluginProcessEnvironment.runtimePath().split(separator: ":") {
            let candidate = URL(fileURLWithPath: String(directory), isDirectory: true)
                .appendingPathComponent(name)
            if FileManager.default.isExecutableFile(atPath: candidate.path) { return candidate }
        }
        throw NativeConnectorError.pluginInstallation("没有找到 \(name)，请先安装 Node.js/npm")
    }

    private static func runInstallerProcess(
        executableURL: URL,
        arguments: [String],
        currentDirectoryURL: URL
    ) async throws {
        let worker = Task.detached(priority: .utility) {
            let process = Process()
            let stdout = Pipe()
            let stderr = Pipe()
            let stdoutCapture = NativeBoundedProcessOutput(maximumBytes: 2 * 1_024 * 1_024)
            let stderrCapture = NativeBoundedProcessOutput(maximumBytes: 2 * 1_024 * 1_024)
            NativeProcessPipeReader.install(
                on: stdout.fileHandleForReading,
                onData: stdoutCapture.append
            )
            NativeProcessPipeReader.install(
                on: stderr.fileHandleForReading,
                onData: stderrCapture.append
            )
            process.executableURL = executableURL
            process.arguments = arguments
            process.currentDirectoryURL = currentDirectoryURL
            process.standardOutput = stdout
            process.standardError = stderr
            let deadline = Date().addingTimeInterval(5 * 60)
            var timedOut = false
            do {
                try process.run()
                while process.isRunning, !Task.isCancelled, Date() < deadline {
                    try? await Task.sleep(for: .milliseconds(50))
                }
                if process.isRunning {
                    timedOut = !Task.isCancelled && Date() >= deadline
                    process.terminate()
                    let terminationDeadline = Date().addingTimeInterval(2)
                    while process.isRunning, Date() < terminationDeadline {
                        try? await Task.sleep(for: .milliseconds(50))
                    }
                    if process.isRunning { kill(process.processIdentifier, SIGKILL) }
                    process.waitUntilExit()
                }
            } catch {
                stdout.fileHandleForReading.readabilityHandler = nil
                stderr.fileHandleForReading.readabilityHandler = nil
                throw error
            }
            stdout.fileHandleForReading.readabilityHandler = nil
            stderr.fileHandleForReading.readabilityHandler = nil
            stdoutCapture.append(stdout.fileHandleForReading.readDataToEndOfFile())
            stderrCapture.append(stderr.fileHandleForReading.readDataToEndOfFile())
            let output = stdoutCapture.snapshot
            let errorOutput = stderrCapture.snapshot
            guard !output.discarded, !errorOutput.discarded else {
                throw NativeConnectorError.pluginInstallation("npm 输出超过安全限制")
            }
            if Task.isCancelled { throw CancellationError() }
            guard !timedOut else {
                throw NativeConnectorError.pluginInstallation("npm 安装超过 5 分钟，已终止")
            }
            guard process.terminationStatus == 0 else {
                let detail = String(decoding: errorOutput.data.suffix(8 * 1_024), as: UTF8.self)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                throw NativeConnectorError.pluginInstallation(
                    detail.isEmpty ? "npm 安装失败" : "npm 安装失败：\(detail)"
                )
            }
        }
        try await withTaskCancellationHandler {
            try await worker.value
        } onCancel: {
            worker.cancel()
        }
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

    init(draft: LocalConnectorCustomMCPDraft) throws {
        displayName = draft.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        description = draft.description.trimmingCharacters(in: .whitespacesAndNewlines)
        componentKey = draft.componentKey.trimmingCharacters(in: .whitespacesAndNewlines)
        packageName = draft.packageName.trimmingCharacters(in: .whitespacesAndNewlines)
        packageVersion = draft.packageVersion.trimmingCharacters(in: .whitespacesAndNewlines)
        binaryName = draft.binaryName.trimmingCharacters(in: .whitespacesAndNewlines)
        arguments = draft.arguments
        environment = draft.environment
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
