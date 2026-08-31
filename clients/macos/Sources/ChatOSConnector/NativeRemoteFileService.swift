import ChatOSCore
import Foundation

public actor NativeRemoteFileService: RemoteFileServicing {
    private let runtime: any NativeRemoteConnectionRuntimeProviding
    private let ssh: any NativeRemoteSSHExecuting

    public init(runtime: any NativeRemoteConnectionRuntimeProviding) {
        self.runtime = runtime
        self.ssh = NativeOpenSSHClient()
    }

    init(
        runtime: any NativeRemoteConnectionRuntimeProviding,
        ssh: any NativeRemoteSSHExecuting
    ) {
        self.runtime = runtime
        self.ssh = ssh
    }

    public func initialDirectory(connectionID: String) async throws -> String {
        let draft = try await runtime.resolvedDraft(id: connectionID)
        return try await ssh.resolveDirectory(
            draft: draft,
            path: draft.defaultRemotePath?.trimmedNonEmpty ?? "."
        )
    }

    public func listDirectory(
        connectionID: String,
        path: String
    ) async throws -> RemoteDirectoryListing {
        let draft = try await runtime.resolvedDraft(id: connectionID)
        let resolvedPath = try await ssh.resolveDirectory(draft: draft, path: path)
        let rawEntries = try await ssh.listDirectory(
            draft: draft,
            path: resolvedPath,
            limit: 1_000
        )
        let entries = rawEntries.map { raw in
            let kind = RemoteFileKind(rawValue: raw.type) ?? .other
            return RemoteFileEntry(
                name: raw.name,
                path: raw.path,
                kind: kind,
                size: kind == .directory ? nil : raw.size,
                modifiedAt: raw.modifiedAt,
                permissions: raw.permissions
            )
        }
        .sorted { lhs, rhs in
            if lhs.isDirectory != rhs.isDirectory { return lhs.isDirectory }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
        return RemoteDirectoryListing(
            path: resolvedPath,
            parentPath: Self.parentPath(of: resolvedPath),
            entries: entries
        )
    }

    public func uploadFile(
        connectionID: String,
        localURL: URL,
        remoteDirectory: String,
        overwrite: Bool
    ) async throws -> String {
        let name = try Self.validateName(localURL.lastPathComponent)
        let targetPath = Self.join(remoteDirectory, name)
        let draft = try await runtime.resolvedDraft(id: connectionID)
        try await ssh.uploadFile(
            draft: draft,
            localURL: localURL,
            remotePath: targetPath,
            overwrite: overwrite
        )
        return targetPath
    }

    public func downloadFile(
        connectionID: String,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {
        let draft = try await runtime.resolvedDraft(id: connectionID)
        try await ssh.downloadFile(
            draft: draft,
            remotePath: remotePath,
            localURL: localURL,
            overwrite: overwrite
        )
    }

    public func createDirectory(
        connectionID: String,
        parentPath: String,
        name: String
    ) async throws {
        let validatedName = try Self.validateName(name)
        let draft = try await runtime.resolvedDraft(id: connectionID)
        try await ssh.createDirectory(
            draft: draft,
            path: Self.join(parentPath, validatedName)
        )
    }

    public func renameEntry(
        connectionID: String,
        path: String,
        newName: String
    ) async throws {
        let validatedName = try Self.validateName(newName)
        guard let parent = Self.parentPath(of: path) else {
            throw NativeRemoteFileServiceError("不能重命名远端根目录。")
        }
        let draft = try await runtime.resolvedDraft(id: connectionID)
        try await ssh.renameEntry(
            draft: draft,
            path: path,
            destinationPath: Self.join(parent, validatedName)
        )
    }

    public func deleteEntry(
        connectionID: String,
        path: String,
        recursively: Bool
    ) async throws {
        guard path != "/" else {
            throw NativeRemoteFileServiceError("不能删除远端根目录。")
        }
        let draft = try await runtime.resolvedDraft(id: connectionID)
        try await ssh.deleteEntry(draft: draft, path: path, recursively: recursively)
    }

    private static func validateName(_ name: String) throws -> String {
        let value = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty, value != ".", value != "..",
              !value.contains("/"), !value.contains("\0") else {
            throw NativeRemoteFileServiceError("文件名不合法。")
        }
        return value
    }

    private static func join(_ directory: String, _ name: String) -> String {
        if directory == "/" { return "/" + name }
        return directory.hasSuffix("/") ? directory + name : directory + "/" + name
    }

    private static func parentPath(of path: String) -> String? {
        let normalized = path.count > 1 && path.hasSuffix("/")
            ? String(path.dropLast())
            : path
        guard normalized != "/" else { return nil }
        guard let slash = normalized.lastIndex(of: "/") else { return "." }
        if slash == normalized.startIndex { return "/" }
        return String(normalized[..<slash])
    }
}

private struct NativeRemoteFileServiceError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
