import ChatOSProcessRuntime
import CryptoKit
import Darwin
import Foundation

struct NativePluginInstaller: Sendable {
    private struct VerifiedPackage {
        let version: String
        let artifactSHA256: String
        let packageRoot: URL
        let packageFileSHA256: [String: String]
    }

    private let rootURL: URL
    private let maximumFiles = 20_000
    private let maximumUnpackedBytes: Int64 = 512 * 1_024 * 1_024

    init(rootURL: URL) {
        self.rootURL = rootURL
    }

    func install(
        source: GatewayPluginSourceDTO,
        token: String,
        gateway: NativeConnectorGateway
    ) async throws -> NativeInstalledPluginRecord {
        try await withVerifiedPackage(source: source, token: token, gateway: gateway) { package in
            let pluginDirectory = rootURL
                .appendingPathComponent(pluginDirectoryName(source.catalog.id), isDirectory: true)
            let finalURL = pluginDirectory.appendingPathComponent(package.version, isDirectory: true)
            let backupURL = rootURL.appendingPathComponent(
                ".backup-\(pluginDirectory.lastPathComponent)-\(UUID().uuidString)",
                isDirectory: true
            )
            let hadPreviousInstallation = FileManager.default.fileExists(atPath: pluginDirectory.path)
            if hadPreviousInstallation {
                try FileManager.default.moveItem(at: pluginDirectory, to: backupURL)
            }
            do {
                try FileManager.default.createDirectory(
                    at: pluginDirectory,
                    withIntermediateDirectories: true
                )
                try FileManager.default.moveItem(at: package.packageRoot, to: finalURL)
                if hadPreviousInstallation {
                    try? FileManager.default.removeItem(at: backupURL)
                }
            } catch {
                try? FileManager.default.removeItem(at: pluginDirectory)
                if hadPreviousInstallation,
                   FileManager.default.fileExists(atPath: backupURL.path) {
                    try? FileManager.default.moveItem(at: backupURL, to: pluginDirectory)
                }
                throw error
            }

            return NativeInstalledPluginRecord(
                pluginID: source.catalog.id,
                releaseID: source.release.id,
                version: package.version,
                artifactSHA256: package.artifactSHA256,
                installationPath: finalURL.path,
                installedAt: ISO8601DateFormatter().string(from: Date()),
                pluginKey: source.catalog.pluginKey,
                packageFileSHA256: package.packageFileSHA256
            )
        }
    }

    func attestLegacyInstallation(
        record: NativeInstalledPluginRecord,
        source: GatewayPluginSourceDTO,
        token: String,
        gateway: NativeConnectorGateway
    ) async throws -> NativeInstalledPluginRecord {
        guard record.pluginID == source.catalog.id,
              record.releaseID == source.release.id,
              record.version == source.release.version?.trimmedNonEmpty,
              record.artifactSHA256 == source.release.artifactSHA256?.trimmedNonEmpty?.lowercased()
        else {
            throw NativeConnectorError.pluginInstallation("旧 Plugin 安装记录与 Marketplace Release 不一致")
        }
        return try await withVerifiedPackage(
            source: source,
            token: token,
            gateway: gateway
        ) { package in
            try attestLegacyInstallation(
                record: record,
                trustedPackageFileSHA256: package.packageFileSHA256
            )
        }
    }

    func attestLegacyInstallation(
        record: NativeInstalledPluginRecord,
        trustedPackageFileSHA256: [String: String]
    ) throws -> NativeInstalledPluginRecord {
        guard record.packageFileSHA256?.isEmpty != false,
              !trustedPackageFileSHA256.isEmpty else {
            throw NativeConnectorError.pluginInstallation("Plugin 安装记录不需要兼容校验")
        }
        let current = try NativePluginInstallationIntegrity.snapshot(
            installationURL: URL(fileURLWithPath: record.installationPath, isDirectory: true),
            maximumFiles: maximumFiles,
            maximumBytes: maximumUnpackedBytes
        )
        guard current == trustedPackageFileSHA256 else {
            throw NativeConnectorError.pluginInstallation("本机 Plugin 文件与可信 Marketplace 制品不一致")
        }
        var attested = record
        attested.packageFileSHA256 = trustedPackageFileSHA256
        return attested
    }

    func uninstall(pluginID: String) throws {
        let directory = rootURL
            .appendingPathComponent(pluginDirectoryName(pluginID), isDirectory: true)
        if FileManager.default.fileExists(atPath: directory.path) {
            try FileManager.default.removeItem(at: directory)
        }
    }

    private func withVerifiedPackage<T>(
        source: GatewayPluginSourceDTO,
        token: String,
        gateway: NativeConnectorGateway,
        body: (VerifiedPackage) throws -> T
    ) async throws -> T {
        guard let version = source.release.version?.trimmedNonEmpty,
              let artifactSHA256 = source.release.artifactSHA256?.trimmedNonEmpty,
              let npmPackage = source.release.npmPackage else {
            throw NativeConnectorError.pluginInstallation("安装源缺少 Release 校验信息")
        }
        guard npmPackage.version == version else {
            throw NativeConnectorError.pluginInstallation("npm 版本与 Release 不一致")
        }
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        let archiveURL = try await gateway.downloadPluginArtifact(
            token: token,
            pluginID: source.catalog.id,
            releaseID: source.release.id
        )
        defer { try? FileManager.default.removeItem(at: archiveURL) }
        guard try sha256(of: archiveURL) == artifactSHA256.lowercased() else {
            throw NativeConnectorError.pluginInstallation("安装包 SHA-256 校验失败")
        }
        try verifyNPMIntegrity(npmPackage.integrity, archiveURL: archiveURL)

        let stagingURL = rootURL
            .appendingPathComponent(".staging", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: stagingURL) }
        try FileManager.default.createDirectory(at: stagingURL, withIntermediateDirectories: true)
        try validateArchiveEntries(archiveURL)
        try runTar(["-xzf", archiveURL.path, "-C", stagingURL.path])
        let packageRoot = stagingURL.appendingPathComponent("package", isDirectory: true)
        guard FileManager.default.fileExists(atPath: packageRoot.path) else {
            throw NativeConnectorError.pluginInstallation("npm 安装包缺少 package 目录")
        }
        try validateExtractedTree(packageRoot)
        try validatePackageJSON(
            packageRoot.appendingPathComponent("package.json"),
            expectedName: npmPackage.name,
            expectedVersion: version
        )
        let packageFileSHA256 = try NativePluginInstallationIntegrity.snapshot(
            installationURL: packageRoot,
            maximumFiles: maximumFiles,
            maximumBytes: maximumUnpackedBytes
        )
        return try body(.init(
            version: version,
            artifactSHA256: artifactSHA256.lowercased(),
            packageRoot: packageRoot,
            packageFileSHA256: packageFileSHA256
        ))
    }

    private func validateArchiveEntries(_ archiveURL: URL) throws {
        let output = try runTar(["-tzf", archiveURL.path])
        let entries = output.split(whereSeparator: \.isNewline).map(String.init)
        guard !entries.isEmpty, entries.count <= maximumFiles else {
            throw NativeConnectorError.pluginInstallation("安装包文件数量异常")
        }
        for entry in entries {
            let normalized = entry.hasPrefix("./") ? String(entry.dropFirst(2)) : entry
            let components = normalized.split(separator: "/", omittingEmptySubsequences: false)
            if normalized.hasPrefix("/")
                || normalized.contains("\0")
                || components.contains("..")
                || !normalized.hasPrefix("package/") {
                throw NativeConnectorError.pluginInstallation("安装包包含越界路径：\(entry)")
            }
        }

        let verbose = try runTar(["-tvzf", archiveURL.path])
        for line in verbose.split(whereSeparator: \.isNewline) {
            if let kind = line.first, kind == "l" || kind == "h" {
                throw NativeConnectorError.pluginInstallation("安装包不允许包含符号链接或硬链接")
            }
        }
    }

    private func validateExtractedTree(_ directory: URL) throws {
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [.isSymbolicLinkKey, .isRegularFileKey, .fileSizeKey],
            options: []
        ) else {
            throw NativeConnectorError.pluginInstallation("无法读取解压目录")
        }
        var count = 0
        var totalBytes: Int64 = 0
        for case let fileURL as URL in enumerator {
            count += 1
            if count > maximumFiles {
                throw NativeConnectorError.pluginInstallation("解压文件数量超过限制")
            }
            let values = try fileURL.resourceValues(forKeys: [
                .isSymbolicLinkKey,
                .isRegularFileKey,
                .fileSizeKey,
            ])
            if values.isSymbolicLink == true {
                throw NativeConnectorError.pluginInstallation("解压结果包含符号链接")
            }
            if values.isRegularFile == true {
                totalBytes += Int64(values.fileSize ?? 0)
                if totalBytes > maximumUnpackedBytes {
                    throw NativeConnectorError.pluginInstallation("解压体积超过限制")
                }
                let attributes = try FileManager.default.attributesOfItem(atPath: fileURL.path)
                if let permissions = attributes[.posixPermissions] as? NSNumber {
                    try FileManager.default.setAttributes(
                        [.posixPermissions: permissions.intValue & 0o755],
                        ofItemAtPath: fileURL.path
                    )
                }
            }
        }
    }

    private func validatePackageJSON(
        _ fileURL: URL,
        expectedName: String,
        expectedVersion: String
    ) throws {
        let data = try NativeBoundedFileReader.read(
            fileURL,
            maximumBytes: 1 * 1_024 * 1_024
        )
        guard let value = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              value["name"] as? String == expectedName,
              value["version"] as? String == expectedVersion,
              value["bin"] != nil else {
            throw NativeConnectorError.pluginInstallation("package.json 身份或可执行入口无效")
        }
    }

    private func verifyNPMIntegrity(_ integrity: String, archiveURL: URL) throws {
        guard integrity.hasPrefix("sha512-"),
              let expected = Data(base64Encoded: String(integrity.dropFirst("sha512-".count))) else {
            throw NativeConnectorError.pluginInstallation("npm integrity 格式无效")
        }
        guard try sha512(of: archiveURL) == expected else {
            throw NativeConnectorError.pluginInstallation("npm integrity 校验失败")
        }
    }

    @discardableResult
    func runTar(
        _ arguments: [String],
        timeout: TimeInterval = 5 * 60,
        maximumOutputBytes: Int = 8 * 1_024 * 1_024
    ) throws -> String {
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        let outputCapture = NativeBoundedProcessOutput(maximumBytes: maximumOutputBytes)
        let errorCapture = NativeBoundedProcessOutput(maximumBytes: 1 * 1_024 * 1_024)
        NativeProcessPipeReader.install(
            on: outputPipe.fileHandleForReading,
            onData: outputCapture.append
        )
        NativeProcessPipeReader.install(
            on: errorPipe.fileHandleForReading,
            onData: errorCapture.append
        )
        let nullInput = open("/dev/null", O_RDONLY)
        guard nullInput >= 0 else {
            throw NativeConnectorError.pluginInstallation("无法打开 tar 标准输入")
        }
        defer { close(nullInput) }
        let executable = "/usr/bin/tar"
        let processArguments = [executable] + arguments
        let environment = ProcessInfo.processInfo.environment
        var processID: pid_t = 0
        let spawnResult = withCStringArray(processArguments) { argv in
            withCStringArray(environment.map { "\($0.key)=\($0.value)" }) { envp in
                chatos_spawn_process_group(
                    executable,
                    argv,
                    envp,
                    nil,
                    nullInput,
                    outputPipe.fileHandleForWriting.fileDescriptor,
                    errorPipe.fileHandleForWriting.fileDescriptor,
                    &processID
                )
            }
        }
        outputPipe.fileHandleForWriting.closeFile()
        errorPipe.fileHandleForWriting.closeFile()
        guard spawnResult == 0, processID > 0 else {
            outputPipe.fileHandleForReading.readabilityHandler = nil
            errorPipe.fileHandleForReading.readabilityHandler = nil
            throw NativeConnectorError.pluginInstallation(
                "无法启动 tar：\(String(cString: strerror(spawnResult)))"
            )
        }
        let exitSignal = NativeProcessExitSignal.reap(processID: processID)
        var exitCode = exitSignal.wait(timeout: timeout)
        let timedOut = exitCode == nil
        if timedOut {
            _ = chatos_signal_process_group(processID, SIGTERM)
            exitCode = exitSignal.wait(timeout: 0.75)
            if exitCode == nil {
                _ = chatos_signal_process_group(processID, SIGKILL)
                exitCode = exitSignal.wait(timeout: 2)
            }
        }
        _ = chatos_signal_process_group(processID, SIGKILL)
        guard let exitCode else {
            outputPipe.fileHandleForReading.readabilityHandler = nil
            errorPipe.fileHandleForReading.readabilityHandler = nil
            try? outputPipe.fileHandleForReading.close()
            try? errorPipe.fileHandleForReading.close()
            throw NativeConnectorError.pluginInstallation("tar 进程无法终止")
        }
        outputPipe.fileHandleForReading.readabilityHandler = nil
        errorPipe.fileHandleForReading.readabilityHandler = nil
        outputCapture.append(outputPipe.fileHandleForReading.readDataToEndOfFile())
        errorCapture.append(errorPipe.fileHandleForReading.readDataToEndOfFile())
        let output = outputCapture.snapshot
        let error = errorCapture.snapshot
        if timedOut {
            throw NativeConnectorError.pluginInstallation("tar 执行超时")
        }
        guard !output.discarded, !error.discarded else {
            throw NativeConnectorError.pluginInstallation("tar 输出超过安全限制")
        }
        if exitCode != 0 {
            let detail = String(decoding: error.data, as: UTF8.self)
            throw NativeConnectorError.pluginInstallation(detail.trimmedNonEmpty ?? "tar 执行失败")
        }
        return String(decoding: output.data, as: UTF8.self)
    }

    private func withCStringArray<Result>(
        _ values: [String],
        _ body: ([UnsafeMutablePointer<CChar>?]) throws -> Result
    ) rethrows -> Result {
        let pointers = values.map { strdup($0) }
        defer { pointers.forEach { free($0) } }
        return try body(pointers + [nil])
    }

    private func sha256(of fileURL: URL) throws -> String {
        var hash = SHA256()
        try updateHash(&hash, from: fileURL)
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private func sha512(of fileURL: URL) throws -> Data {
        var hash = SHA512()
        try updateHash(&hash, from: fileURL)
        return Data(hash.finalize())
    }

    private func updateHash<Hash: HashFunction>(_ hash: inout Hash, from fileURL: URL) throws {
        let handle = try FileHandle(forReadingFrom: fileURL)
        defer { try? handle.close() }
        while let data = try handle.read(upToCount: 1024 * 1024), !data.isEmpty {
            hash.update(data: data)
        }
    }

    private func pluginDirectoryName(_ pluginID: String) -> String {
        SHA256.hash(data: Data(pluginID.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
