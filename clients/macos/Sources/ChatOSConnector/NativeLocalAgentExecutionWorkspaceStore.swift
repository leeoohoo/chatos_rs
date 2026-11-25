import ChatOSCore
import CryptoKit
import Darwin
import Foundation

actor NativeLocalAgentExecutionWorkspaceStore {
    private struct Lease {
        let sourceRoot: URL
        let isolatedRoot: URL
        let temporaryContainer: URL?
        let sourceGitFingerprint: String?
    }

    private let configuredBaseURL: URL?
    private var leases: [String: Lease] = [:]

    init(baseURL: URL? = nil) {
        configuredBaseURL = baseURL
    }

    func bind(
        context: NativeLocalAgentTaskExecutionContext,
        runID: String
    ) throws -> NativeLocalAgentTaskExecutionContext {
        guard context.projectID != nil,
              context.toolAuthorization.requiresIsolatedProjectWorkspace else {
            return context
        }
        let sourceRoot = context.executionRootURL.standardizedFileURL.resolvingSymlinksInPath()
        let lease: Lease
        if let existing = leases[runID] {
            guard existing.sourceRoot == sourceRoot else {
                throw NativeLocalAgentExecutionWorkspaceError.runRebound
            }
            lease = existing
        } else {
            lease = try createLease(runID: runID, sourceRoot: sourceRoot)
            leases[runID] = lease
        }
        try verifySourceGitMetadata(lease)
        let workspace = LocalConnectorWorkspace(
            id: context.workspaceScopeID + ":isolated:" + Self.safeComponent(runID),
            alias: context.resolvedPath?.workspace.alias ?? "Isolated Task Workspace",
            absoluteRoot: lease.isolatedRoot.path,
            fingerprint: NativePluginManifestLoader.sha256(
                context.workspaceScopeID + "\n" + runID + "\n" + lease.isolatedRoot.path
            ),
            projectConfigTrusted: context.resolvedPath?.workspace.projectConfigTrusted,
            projectConfigTrustStale: context.resolvedPath?.workspace.projectConfigTrustStale
        )
        let resolvedPath = NativeResolvedProjectPath(
            workspace: workspace,
            relativePath: ".",
            absoluteURL: lease.isolatedRoot,
            logicalPrefix: nil
        )
        return NativeLocalAgentTaskExecutionContext(
            conversationID: context.conversationID,
            projectID: context.projectID,
            applicationContext: context.applicationContext,
            resolvedPath: resolvedPath,
            executionRootURL: lease.isolatedRoot,
            workspaceScopeID: workspace.id,
            toolAuthorization: context.toolAuthorization,
            remoteConnectionID: context.remoteConnectionID,
            sourceProjectRootURL: sourceRoot,
            usesIsolatedWorkspace: true
        )
    }

    func verifySourceUnchanged(runID: String) throws {
        guard let lease = leases[runID] else { return }
        try verifySourceGitMetadata(lease)
    }

    func release(runID: String) {
        guard let lease = leases.removeValue(forKey: runID) else { return }
        try? FileManager.default.removeItem(at: lease.isolatedRoot)
        if let temporaryContainer = lease.temporaryContainer {
            try? FileManager.default.removeItem(at: temporaryContainer)
        }
    }

    func reset() {
        let currentLeases = Array(leases.values)
        leases.removeAll()
        for lease in currentLeases {
            try? FileManager.default.removeItem(at: lease.isolatedRoot)
            if let temporaryContainer = lease.temporaryContainer {
                try? FileManager.default.removeItem(at: temporaryContainer)
            }
        }
    }

    private func createLease(runID: String, sourceRoot: URL) throws -> Lease {
        let fileManager = FileManager.default
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: sourceRoot.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw NativeLocalAgentExecutionWorkspaceError.sourceUnavailable
        }
        let gitEntry = sourceRoot.appendingPathComponent(".git", isDirectory: false)
        if fileManager.fileExists(atPath: gitEntry.path, isDirectory: &isDirectory),
           !isDirectory.boolValue {
            // A linked worktree's .git file points back to shared metadata. Copying it would
            // reproduce the exact cross-worktree corruption this lease is designed to prevent.
            throw NativeLocalAgentExecutionWorkspaceError.linkedGitWorktreeUnsupported
        }
        let baseURL: URL
        let temporaryContainer: URL?
        if let configuredBaseURL {
            try fileManager.createDirectory(at: configuredBaseURL, withIntermediateDirectories: true)
            baseURL = configuredBaseURL
            temporaryContainer = nil
        } else {
            baseURL = try fileManager.url(
                for: .itemReplacementDirectory,
                in: .userDomainMask,
                appropriateFor: sourceRoot,
                create: true
            )
            temporaryContainer = baseURL
        }
        let volumeKeys: Set<URLResourceKey> = [
            .volumeIdentifierKey,
            .volumeSupportsFileCloningKey,
        ]
        let sourceVolume = try sourceRoot.resourceValues(forKeys: volumeKeys)
        let destinationVolume = try baseURL.resourceValues(forKeys: volumeKeys)
        guard sourceVolume.volumeSupportsFileCloning == true,
              sourceVolume.volumeIdentifier?.isEqual(destinationVolume.volumeIdentifier) == true else {
            if let temporaryContainer {
                try? fileManager.removeItem(at: temporaryContainer)
            }
            throw NativeLocalAgentExecutionWorkspaceError.copyOnWriteUnavailable
        }
        let name = Self.safeComponent(runID) + "-" + UUID().uuidString.lowercased()
        let partial = baseURL.appendingPathComponent(name + ".partial", isDirectory: true)
        let destination = baseURL.appendingPathComponent(name, isDirectory: true)
        let before = try Self.gitMetadataFingerprint(projectRoot: sourceRoot)
        do {
            try Self.cloneTree(from: sourceRoot, to: partial)
            let after = try Self.gitMetadataFingerprint(projectRoot: sourceRoot)
            guard before == after else {
                throw NativeLocalAgentExecutionWorkspaceError.sourceChangedDuringSnapshot
            }
            try fileManager.moveItem(at: partial, to: destination)
            return Lease(
                sourceRoot: sourceRoot,
                isolatedRoot: destination.standardizedFileURL.resolvingSymlinksInPath(),
                temporaryContainer: temporaryContainer,
                sourceGitFingerprint: after
            )
        } catch {
            try? fileManager.removeItem(at: partial)
            try? fileManager.removeItem(at: destination)
            if let temporaryContainer {
                try? fileManager.removeItem(at: temporaryContainer)
            }
            throw error
        }
    }

    private func verifySourceGitMetadata(_ lease: Lease) throws {
        guard try Self.gitMetadataFingerprint(projectRoot: lease.sourceRoot)
            == lease.sourceGitFingerprint else {
            throw NativeLocalAgentExecutionWorkspaceError.sourceGitMetadataChanged
        }
    }

    private static func cloneTree(from source: URL, to destination: URL) throws {
        let flags = copyfile_flags_t(COPYFILE_ALL | COPYFILE_RECURSIVE | COPYFILE_CLONE)
        let result = source.path.withCString { sourcePath in
            destination.path.withCString { destinationPath in
                copyfile(sourcePath, destinationPath, nil, flags)
            }
        }
        guard result == 0 else {
            throw NativeLocalAgentExecutionWorkspaceError.cloneFailed(errno)
        }
    }

    private static func gitMetadataFingerprint(projectRoot: URL) throws -> String? {
        let gitRoot = projectRoot.appendingPathComponent(".git", isDirectory: true)
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: gitRoot.path, isDirectory: &isDirectory) else {
            return nil
        }
        guard isDirectory.boolValue else {
            throw NativeLocalAgentExecutionWorkspaceError.linkedGitWorktreeUnsupported
        }
        var files = [
            gitRoot.appendingPathComponent("HEAD"),
            gitRoot.appendingPathComponent("index"),
            gitRoot.appendingPathComponent("config"),
            gitRoot.appendingPathComponent("packed-refs"),
        ].filter { FileManager.default.fileExists(atPath: $0.path) }
        let refs = gitRoot.appendingPathComponent("refs", isDirectory: true)
        if let enumerator = FileManager.default.enumerator(
            at: refs,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey],
            options: [.skipsHiddenFiles]
        ) {
            for case let url as URL in enumerator {
                let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                if values.isRegularFile == true || values.isSymbolicLink == true {
                    files.append(url)
                }
            }
        }
        files.sort { $0.path < $1.path }
        var hasher = SHA256()
        for file in files {
            let relative = String(file.path.dropFirst(gitRoot.path.count))
            hasher.update(data: Data(relative.utf8))
            let handle = try FileHandle(forReadingFrom: file)
            defer { try? handle.close() }
            while let chunk = try handle.read(upToCount: 1_048_576), !chunk.isEmpty {
                hasher.update(data: chunk)
            }
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func safeComponent(_ value: String) -> String {
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_"))
        let sanitized = value.unicodeScalars.map { allowed.contains($0) ? Character(String($0)) : "-" }
        return String(sanitized.prefix(80))
    }
}

enum NativeLocalAgentExecutionWorkspaceError: LocalizedError, Equatable {
    case sourceUnavailable
    case runRebound
    case linkedGitWorktreeUnsupported
    case copyOnWriteUnavailable
    case sourceChangedDuringSnapshot
    case sourceGitMetadataChanged
    case cloneFailed(Int32)

    var errorDescription: String? {
        switch self {
        case .sourceUnavailable:
            "项目目录不可用，无法创建隔离验证工作区。"
        case .runRebound:
            "同一任务运行绑定到了不同的项目目录。"
        case .linkedGitWorktreeUnsupported:
            "当前项目是共享 Git 元数据的 linked worktree，不能安全创建只读执行副本。"
        case .copyOnWriteUnavailable:
            "项目所在磁盘不支持同卷 COW 隔离，已拒绝全量复制项目。"
        case .sourceChangedDuringSnapshot:
            "创建隔离工作区时项目 Git 元数据发生变化，请稍后重试。"
        case .sourceGitMetadataChanged:
            "只读任务运行期间原项目 Git 元数据发生变化，已停止继续执行。"
        case let .cloneFailed(code):
            "无法创建 APFS 隔离工作区（错误码 \(code)）。"
        }
    }
}
