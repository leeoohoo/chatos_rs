import CryptoKit
import Darwin
import Foundation

enum NativePluginInstallationIntegrity {
    enum Status: Sendable, Equatable {
        case verified
        case missingRecord
        case failed
    }

    private static let defaultMaximumFiles = 20_000
    private static let defaultMaximumBytes: Int64 = 512 * 1_024 * 1_024

    static func verify(record: NativeInstalledPluginRecord, installationURL: URL) throws {
        guard let expected = record.packageFileSHA256, !expected.isEmpty else {
            throw NativePluginRuntimeError.invalidManifest(
                "Plugin 安装缺少逐文件完整性记录，请重新安装"
            )
        }
        let actual = try snapshot(
            installationURL: installationURL,
            maximumFiles: defaultMaximumFiles,
            maximumBytes: defaultMaximumBytes
        )
        guard actual.count == expected.count,
              actual.allSatisfy({ expected[$0.key] == $0.value }) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin 安装文件已被篡改")
        }
    }

    static func status(record: NativeInstalledPluginRecord) -> Status {
        guard let expected = record.packageFileSHA256, !expected.isEmpty else {
            return .missingRecord
        }
        let installationURL = URL(
            fileURLWithPath: record.installationPath,
            isDirectory: true
        )
        do {
            try verify(record: record, installationURL: installationURL)
            return .verified
        } catch {
            return .failed
        }
    }

    static func snapshot(
        installationURL: URL,
        maximumFiles: Int,
        maximumBytes: Int64
    ) throws -> [String: String] {
        let root = URL(fileURLWithPath: canonicalPath(installationURL.path), isDirectory: true)
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
        ]
        guard let enumerator = FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: Array(keys),
            options: []
        ) else {
            throw NativePluginRuntimeError.invalidManifest("无法读取 Plugin 安装目录")
        }
        var result: [String: String] = [:]
        var totalBytes: Int64 = 0
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        for case let fileURL as URL in enumerator {
            var info = stat()
            guard lstat(fileURL.path, &info) == 0 else {
                throw NativePluginRuntimeError.invalidManifest("Plugin 安装文件在校验期间发生变化")
            }
            let kind = info.st_mode & mode_t(S_IFMT)
            if kind == mode_t(S_IFDIR) { continue }
            guard kind != mode_t(S_IFLNK) else {
                throw NativePluginRuntimeError.invalidManifest("Plugin 安装目录包含符号链接")
            }
            guard kind == mode_t(S_IFREG), fileURL.path.hasPrefix(prefix) else {
                throw NativePluginRuntimeError.invalidManifest("Plugin 安装目录包含非常规文件")
            }
            let relativePath = String(fileURL.path.dropFirst(prefix.count))
            guard !relativePath.isEmpty, result[relativePath] == nil else {
                throw NativePluginRuntimeError.invalidManifest("Plugin 安装文件路径无效")
            }
            if info.st_nlink > 1 {
                throw NativePluginRuntimeError.invalidManifest("Plugin 安装目录包含硬链接")
            }
            totalBytes += Int64(info.st_size)
            guard result.count < maximumFiles, totalBytes <= maximumBytes else {
                throw NativePluginRuntimeError.invalidManifest("Plugin 安装目录超过完整性校验限制")
            }
            result[relativePath] = try sha256(fileURL, expected: info)
        }
        return result
    }

    private static func sha256(_ fileURL: URL, expected: stat) throws -> String {
        let descriptor = open(fileURL.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW)
        guard descriptor >= 0 else {
            throw NativePluginRuntimeError.invalidManifest(
                "Plugin 安装文件在校验期间发生变化"
            )
        }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? handle.close() }
        var before = stat()
        guard fstat(descriptor, &before) == 0,
              sameFile(before, expected),
              before.st_nlink == 1 else {
            throw NativePluginRuntimeError.invalidManifest(
                "Plugin 安装文件在校验期间发生变化"
            )
        }
        var hash = SHA256()
        while let data = try handle.read(upToCount: 1_024 * 1_024), !data.isEmpty {
            hash.update(data: data)
        }
        var after = stat()
        guard fstat(descriptor, &after) == 0, sameFile(after, before) else {
            throw NativePluginRuntimeError.invalidManifest(
                "Plugin 安装文件在校验期间发生变化"
            )
        }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func sameFile(_ lhs: stat, _ rhs: stat) -> Bool {
        (lhs.st_mode & mode_t(S_IFMT)) == mode_t(S_IFREG)
            && lhs.st_dev == rhs.st_dev
            && lhs.st_ino == rhs.st_ino
            && lhs.st_size == rhs.st_size
            && lhs.st_mtimespec.tv_sec == rhs.st_mtimespec.tv_sec
            && lhs.st_mtimespec.tv_nsec == rhs.st_mtimespec.tv_nsec
            && lhs.st_ctimespec.tv_sec == rhs.st_ctimespec.tv_sec
            && lhs.st_ctimespec.tv_nsec == rhs.st_ctimespec.tv_nsec
    }

    private static func canonicalPath(_ path: String) -> String {
        var resolved = [CChar](repeating: 0, count: Int(PATH_MAX))
        return path.withCString { pointer in
            guard realpath(pointer, &resolved) != nil else {
                return URL(fileURLWithPath: path).standardizedFileURL.path
            }
            return String(
                decoding: resolved.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) },
                as: UTF8.self
            )
        }
    }
}
