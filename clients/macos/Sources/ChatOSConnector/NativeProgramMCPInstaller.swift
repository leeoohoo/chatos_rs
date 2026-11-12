import ChatOSCore
import Foundation

extension NativePluginInstaller {
    func prepareProgramMCP(_ draft: LocalConnectorPluginSourceDraft) async throws -> NativePreparedCustomPlugin {
        guard let program = draft.program,
              !program.displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              program.displayName.count <= 120,
              program.componentKey.range(of: "^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$", options: .regularExpression) != nil,
              program.arguments.count <= 64,
              program.arguments.allSatisfy({ $0.count <= 2_048 && !$0.contains("\0") }),
              draft.gitReference == nil, draft.subdirectory == nil,
              let version = draft.exactVersion, !version.isEmpty, version.count <= 256,
              !version.contains("/"), !version.contains("\\"), !version.contains("\0"),
              version != ".", version != ".." else {
            throw NativeConnectorError.pluginInstallation("MCP 程序需要名称、组件 Key、固定版本、分项启动参数和访问级别")
        }
        let staging = try makeCustomStagingRoot()
        do {
            let package = staging.appendingPathComponent("package")
            let runtime = package.appendingPathComponent("runtime")
            try FileManager.default.createDirectory(at: runtime, withIntermediateDirectories: true)
            let identity: String
            let executablePath: String
            let summary: String
            if draft.kind == .githubRelease {
                let repo = try NativePluginRepositoryClient.Repository(draft.location)
                guard let asset = program.assetName, let expected = program.archiveSHA256,
                      expected.range(of: "^[a-f0-9]{64}$", options: .regularExpression) != nil else {
                    throw NativeConnectorError.pluginInstallation("请先解析官方发行包，取得固定资产名称与 SHA-256")
                }
                try NativePluginRepositoryClient.validateRelativePath(program.executablePath)
                let data = try await repositoryClient.downloadRelease(repo: repo, tag: version, assetName: asset)
                guard NativePluginRepositoryClient.digest(data) == expected else {
                    throw NativeConnectorError.pluginInstallation("发行包在解析后发生变化，请重新解析和预检")
                }
                let archive = staging.appendingPathComponent("download")
                try data.write(to: archive)
                _ = try await Self.programArchiveEntries(archive)
                _ = try await Self.runArchiveTool(["-xf", archive.path, "-C", runtime.path])
                // Recheck the complete tree after extraction; snapshots reject links and special files.
                _ = try NativePluginInstallationIntegrity.snapshot(
                    installationURL: runtime, maximumFiles: maximumFiles, maximumBytes: maximumUnpackedBytes
                )
                identity = repo.url
                executablePath = program.executablePath
                summary = "GitHub Release · \(repo.url) · \(version) · \(asset)"
            } else {
                guard draft.kind == .localExecutable, draft.location.hasPrefix("/"),
                      !draft.location.contains("\0"), program.assetName == nil, program.archiveSHA256 == nil,
                      program.executablePath == draft.location else {
                    throw NativeConnectorError.pluginInstallation("本机程序需要已安装可执行文件的绝对路径")
                }
                let source = URL(fileURLWithPath: draft.location).resolvingSymlinksInPath().standardizedFileURL
                let values = try source.resourceValues(forKeys: [.isRegularFileKey, .isExecutableKey, .fileSizeKey])
                guard values.isRegularFile == true, values.isExecutable == true,
                      (values.fileSize ?? 0) <= maximumUnpackedBytes else {
                    throw NativeConnectorError.pluginInstallation("本机 MCP 程序不存在、不可执行或体积超限")
                }
                // Copy into managed storage so preflight/install fingerprints bind actual code.
                executablePath = "program"
                try FileManager.default.copyItem(at: source, to: runtime.appendingPathComponent(executablePath))
                identity = source.path
                summary = "Local executable · \(source.path) · \(version)"
            }
            let entry = runtime.appendingPathComponent(executablePath).standardizedFileURL
            let values = try entry.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            guard entry.path.hasPrefix(runtime.path + "/"), values.isRegularFile == true,
                  values.isSymbolicLink != true else {
                throw NativeConnectorError.pluginInstallation("发行包中没有指定的 MCP 程序入口：\(executablePath)")
            }
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: entry.path)
            let bin = package.appendingPathComponent("bin")
            try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
            // Quote the relative path as a literal; neither file names nor arguments become shell code.
            let quoted = Self.programShellLiteral("/runtime/" + executablePath)
            let launcher = "#!/bin/sh\nexec \"$CHATOS_PLUGIN_ROOT\"\(quoted) \"$@\"\n"
            let launcherURL = bin.appendingPathComponent(program.componentKey)
            try Data(launcher.utf8).write(to: launcherURL)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: launcherURL.path)
            let name = "program-" + String(Self.sha256Hex(identity).prefix(24))
            let manifest: [String: Any] = [
                "schemaVersion": 3, "name": name, "version": version,
                "description": summary, "interface": ["displayName": program.displayName],
                "mcpServers": [program.componentKey: [
                    "type": "stdio", "bin": program.componentKey, "args": program.arguments,
                    "agent": ["displayName": program.displayName, "description": summary],
                ]],
                "permissions": NativeCustomMCPAccessPolicy.permissions(for: program.accessProfile).map {
                    ["permission": $0.id, "reason": $0.reason, "required": true,
                     "components": [program.componentKey]] as [String: Any]
                },
            ]
            try JSONSerialization.data(withJSONObject: manifest, options: [.sortedKeys, .prettyPrinted])
                .write(to: package.appendingPathComponent("chatos.plugin.json"))
            var prepared = try preparedCustomPlugin(
                packageRoot: package, temporaryRootURL: staging,
                pluginIDSeed: "program:\(identity):\(program.componentKey)", source: .programMCP, sourceSummary: summary
            )
            prepared.record.sourceDraft = draft
            return prepared
        } catch {
            try? FileManager.default.removeItem(at: staging)
            throw error
        }
    }

    /// libarchive's verbose listing is checked before extraction, including declared unpacked size.
    static func programArchiveEntries(_ archive: URL) async throws -> [String] {
        let listing = try await runArchiveTool(["-tf", archive.path])
        let entries = String(decoding: listing, as: UTF8.self).split(whereSeparator: \.isNewline).map(String.init)
        guard !entries.isEmpty, entries.count <= 20_000 else {
            throw NativeConnectorError.pluginInstallation("发行包文件数量异常或超限")
        }
        for entry in entries {
            var name = entry
            while name.hasPrefix("./") { name.removeFirst(2) }
            if name.hasSuffix("/") { name.removeLast() }
            if name.isEmpty { continue }
            try NativePluginRepositoryClient.validateRelativePath(name)
        }
        let verbose = try await runArchiveTool(["-tvf", archive.path])
        var total: Int64 = 0
        var count = 0
        for line in String(decoding: verbose, as: UTF8.self).split(whereSeparator: \.isNewline) {
            let fields = line.split(whereSeparator: \.isWhitespace)
            guard let kind = line.first, kind == "-" || kind == "d", fields.count >= 8,
                  let size = Int64(fields[4]), size >= 0, size <= 512 * 1_024 * 1_024 else {
                throw NativeConnectorError.pluginInstallation("发行包含链接、特殊文件或无法验证的条目")
            }
            total += size
            count += 1
            guard total <= 512 * 1_024 * 1_024 else {
                throw NativeConnectorError.pluginInstallation("发行包解压体积超限")
            }
        }
        guard count == entries.count else {
            throw NativeConnectorError.pluginInstallation("发行包路径清单无法完整验证")
        }
        return entries
    }

    private static func programShellLiteral(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private static func runArchiveTool(_ arguments: [String]) async throws -> Data {
        try await runNPMProcess(
            executableURL: URL(fileURLWithPath: "/usr/bin/tar"), arguments: arguments,
            currentDirectoryURL: nil, timeout: 60, stderrLimit: 256 * 1_024,
            overflowMessage: "发行包目录输出超限", timeoutMessage: "发行包读取或解压超过 60 秒",
            failureLabel: "发行包读取或解压"
        )
    }
}
