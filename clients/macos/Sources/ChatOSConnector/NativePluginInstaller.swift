import CryptoKit
import Foundation

struct NativePluginInstaller: Sendable {
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

        let pluginDirectory = rootURL
            .appendingPathComponent(pluginDirectoryName(source.catalog.id), isDirectory: true)
        let finalURL = pluginDirectory.appendingPathComponent(version, isDirectory: true)
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
            try FileManager.default.moveItem(at: packageRoot, to: finalURL)
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
            version: version,
            artifactSHA256: artifactSHA256.lowercased(),
            installationPath: finalURL.path,
            installedAt: ISO8601DateFormatter().string(from: Date()),
            pluginKey: source.catalog.pluginKey
        )
    }

    func uninstall(pluginID: String) throws {
        let directory = rootURL
            .appendingPathComponent(pluginDirectoryName(pluginID), isDirectory: true)
        if FileManager.default.fileExists(atPath: directory.path) {
            try FileManager.default.removeItem(at: directory)
        }
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
        let data = try Data(contentsOf: fileURL, options: .mappedIfSafe)
        guard data.count <= 1_024 * 1_024,
              let value = try JSONSerialization.jsonObject(with: data) as? [String: Any],
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
    private func runTar(_ arguments: [String]) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
        process.arguments = arguments
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        try process.run()
        process.waitUntilExit()
        let output = String(decoding: stdout.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        if process.terminationStatus != 0 {
            let detail = String(decoding: stderr.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            throw NativeConnectorError.pluginInstallation(detail.trimmedNonEmpty ?? "tar 执行失败")
        }
        return output
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
