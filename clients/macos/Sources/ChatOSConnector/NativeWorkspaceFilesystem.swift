import ChatOSCore
import Foundation

struct NativeWorkspaceFilesystem: Sendable {
    private static let maximumTextPreviewBytes: Int64 = 2 * 1_024 * 1_024
    private static let maximumImagePreviewBytes: Int64 = 25 * 1_024 * 1_024
    private static let maximumSearchFileBytes: Int64 = 2 * 1_024 * 1_024
    private static let maximumSearchVisits = 20_000
    private static let searchDuration: TimeInterval = 3

    private let workspace: LocalConnectorWorkspace
    private var fileManager: FileManager { .default }

    private static let previewableImageExtensions: Set<String> = [
        "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg",
        "png", "svg", "tif", "tiff", "webp",
    ]

    init(workspace: LocalConnectorWorkspace) {
        self.workspace = workspace
    }

    func resolveExistingURL(_ path: String) throws -> URL {
        let root = try workspaceRoot()
        return try existingURL(path, root: root)
    }

    func list(path: String, includeFiles: Bool) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let directory = try existingURL(path, root: root)
        guard try resourceValues(directory).isDirectory == true else {
            throw NativeWorkspaceRelayError.notDirectory
        }
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey,
            .fileSizeKey, .contentModificationDateKey,
        ]
        let urls = try fileManager.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: Array(keys),
            options: []
        )
        let entries = try urls.compactMap { url -> NativeWorkspaceEntry? in
            let values = try url.resourceValues(forKeys: keys)
            guard values.isSymbolicLink != true else { return nil }
            let isDirectory = values.isDirectory == true
            guard includeFiles || isDirectory else { return nil }
            return NativeWorkspaceEntry(
                name: url.lastPathComponent,
                path: relativePath(url, root: root),
                isDirectory: isDirectory,
                size: Int64(values.fileSize ?? 0),
                modifiedAt: values.contentModificationDate.map(milliseconds)
            )
        }
        .sorted {
            if $0.isDirectory != $1.isDirectory { return $0.isDirectory && !$1.isDirectory }
            return $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
        let relative = relativePath(directory, root: root)
        let parent: NativeJSONValue
        if directory == root {
            parent = .null
        } else {
            parent = .string(relativePath(directory.deletingLastPathComponent(), root: root))
        }
        return .object([
            "path": .string(relative),
            "parent": parent,
            "entries": .array(entries.map(\.jsonValue)),
        ])
    }

    func read(path: String) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let url = try existingURL(path, root: root)
        let values = try resourceValues(url)
        guard values.isRegularFile == true else { throw NativeWorkspaceRelayError.notFile }
        let size = Int64(values.fileSize ?? 0)
        let extensionName = url.pathExtension.lowercased()
        let maximumPreviewBytes = Self.previewableImageExtensions.contains(extensionName)
            ? Self.maximumImagePreviewBytes
            : Self.maximumTextPreviewBytes
        guard size <= maximumPreviewBytes else {
            throw NativeWorkspaceRelayError.fileTooLarge(size)
        }
        let data = try Data(contentsOf: url, options: [.mappedIfSafe])
        let isBinary = data.prefix(8_000).contains(0)
        return .object([
            "path": .string(relativePath(url, root: root)),
            "size": .number(Double(size)),
            "modified_at": values.contentModificationDate.map { .number(Double(milliseconds($0))) } ?? .null,
            "is_binary": .bool(isBinary),
            "content": .string(isBinary ? data.base64EncodedString() : String(decoding: data, as: UTF8.self)),
        ])
    }

    func searchEntries(path: String, query: String, limit: Int) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let start = try existingURL(path, root: root)
        guard try resourceValues(start).isDirectory == true else {
            throw NativeWorkspaceRelayError.notDirectory
        }
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else { throw NativeWorkspaceRelayError.missingField("query") }
        let maximum = min(max(limit, 1), 500)
        let deadline = Date().addingTimeInterval(Self.searchDuration)
        var stack = [start]
        var matches: [NativeWorkspaceEntry] = []
        var visitedDirectories = 0
        var truncated = false

        while let directory = stack.popLast() {
            guard !currentTaskIsCancelled,
                  Date() < deadline,
                  visitedDirectories < Self.maximumSearchVisits else {
                truncated = true
                break
            }
            visitedDirectories += 1
            let urls = (try? fileManager.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey, .fileSizeKey, .contentModificationDateKey],
                options: []
            )) ?? []
            for url in urls {
                guard !currentTaskIsCancelled, Date() < deadline, matches.count < maximum else {
                    truncated = true
                    break
                }
                let values = try? url.resourceValues(forKeys: [
                    .isDirectoryKey, .isSymbolicLinkKey, .fileSizeKey, .contentModificationDateKey,
                ])
                guard values?.isSymbolicLink != true else { continue }
                let isDirectory = values?.isDirectory == true
                if isDirectory, !shouldSkipSearchDirectory(url) { stack.append(url) }
                let relative = relativePath(url, root: root)
                guard url.lastPathComponent.localizedCaseInsensitiveContains(needle)
                        || relative.localizedCaseInsensitiveContains(needle) else { continue }
                matches.append(.init(
                    name: url.lastPathComponent,
                    path: relative,
                    isDirectory: isDirectory,
                    size: Int64(values?.fileSize ?? 0),
                    modifiedAt: values?.contentModificationDate.map(milliseconds)
                ))
            }
            if truncated { break }
        }
        matches.sort { $0.path.localizedCaseInsensitiveCompare($1.path) == .orderedAscending }
        return .object([
            "matches": .array(matches.map(\.jsonValue)),
            "visited_dirs": .number(Double(visitedDirectories)),
            "truncated": .bool(truncated),
        ])
    }

    func searchContent(path: String, query: String, limit: Int) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let start = try existingURL(path, root: root)
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else { throw NativeWorkspaceRelayError.missingField("query") }
        let maximum = min(max(limit, 1), 500)
        let deadline = Date().addingTimeInterval(Self.searchDuration)
        var stack = [start]
        var matches: [NativeJSONValue] = []
        var scannedFiles = 0
        var visits = 0
        var truncated = false

        while let url = stack.popLast() {
            guard !currentTaskIsCancelled,
                  Date() < deadline,
                  visits < Self.maximumSearchVisits else {
                truncated = true
                break
            }
            visits += 1
            let values = try? resourceValues(url)
            guard values?.isSymbolicLink != true else { continue }
            if values?.isDirectory == true {
                guard url == start || !shouldSkipSearchDirectory(url) else { continue }
                stack.append(contentsOf: (try? fileManager.contentsOfDirectory(
                    at: url,
                    includingPropertiesForKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey],
                    options: []
                )) ?? [])
                continue
            }
            let size = Int64(values?.fileSize ?? 0)
            guard values?.isRegularFile == true, size <= Self.maximumSearchFileBytes,
                  let data = try? Data(contentsOf: url, options: [.mappedIfSafe]),
                  !data.prefix(8_000).contains(0) else { continue }
            scannedFiles += 1
            let content = String(decoding: data, as: UTF8.self)
            for (index, line) in content.split(separator: "\n", omittingEmptySubsequences: false).enumerated() {
                guard !currentTaskIsCancelled else {
                    truncated = true
                    break
                }
                guard let range = line.range(of: needle, options: [.caseInsensitive, .diacriticInsensitive]) else { continue }
                matches.append(.object([
                    "path": .string(relativePath(url, root: root)),
                    "line": .number(Double(index + 1)),
                    "column": .number(Double(line.distance(from: line.startIndex, to: range.lowerBound) + 1)),
                    "text": .string(String(line.prefix(2_000))),
                ]))
                if matches.count >= maximum {
                    truncated = true
                    break
                }
            }
            if truncated { break }
        }
        return .object([
            "matches": .array(matches),
            "scanned_files": .number(Double(scannedFiles)),
            "truncated": .bool(truncated),
        ])
    }

    private func shouldSkipSearchDirectory(_ url: URL) -> Bool {
        Self.ignoredSearchDirectoryNames.contains(url.lastPathComponent)
    }

    private var currentTaskIsCancelled: Bool {
        withUnsafeCurrentTask { $0?.isCancelled == true }
    }

    private static let ignoredSearchDirectoryNames: Set<String> = [
        ".git", ".build", ".cache", ".next", ".idea", ".vscode",
        "node_modules", "DerivedData", "Pods", "target", "dist", "build", "vendor",
    ]

    func createDirectory(path: String) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let components = try normalizedComponents(path, permitsRoot: false)
        var current = root
        for component in components {
            current.appendPathComponent(component, isDirectory: true)
            do {
                let values = try current.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                guard values.isSymbolicLink != true else { throw NativeWorkspaceRelayError.symbolicLink }
                guard values.isDirectory == true else { throw NativeWorkspaceRelayError.notDirectory }
            } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
                try fileManager.createDirectory(at: current, withIntermediateDirectories: false)
            }
        }
        return .object(["path": .string(components.joined(separator: "/")), "created": .bool(true)])
    }

    func write(path: String, content: String, createOnly: Bool) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let target = try writableURL(path, root: root)
        let existed = fileManager.fileExists(atPath: target.path)
        if existed {
            let values = try resourceValues(target)
            guard values.isSymbolicLink != true, values.isRegularFile == true else {
                throw NativeWorkspaceRelayError.notFile
            }
            guard !createOnly else { throw NativeWorkspaceRelayError.alreadyExists }
        }
        let data = Data(content.utf8)
        if createOnly {
            do {
                try data.write(to: target, options: [.withoutOverwriting])
            } catch let error as CocoaError where error.code == .fileWriteFileExists {
                throw NativeWorkspaceRelayError.alreadyExists
            }
        } else {
            try data.write(to: target, options: [.atomic])
        }
        let values = try resourceValues(target)
        return .object([
            "path": .string(relativePath(target, root: root)),
            "size": .number(Double(values.fileSize ?? data.count)),
            "modified_at": values.contentModificationDate.map { .number(Double(milliseconds($0))) } ?? .null,
            "created": .bool(!existed),
        ])
    }

    func delete(path: String, recursive: Bool) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let target = try entryURLWithoutFollowingLeaf(path, root: root)
        let values = try target.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        let isDirectory = values.isDirectory == true && values.isSymbolicLink != true
        do {
            if isDirectory && !recursive {
                try fileManager.removeItem(at: target)
            } else {
                try fileManager.removeItem(at: target)
            }
        } catch let error as CocoaError where error.code == .fileWriteFileExists || error.code == .fileWriteUnknown {
            if isDirectory && !recursive { throw NativeWorkspaceRelayError.directoryNotEmpty }
            throw error
        }
        return .object([
            "path": .string(relativePath(target, root: root)),
            "is_dir": .bool(isDirectory),
            "recursive": .bool(recursive),
            "deleted": .bool(true),
        ])
    }

    func move(sourcePath: String, targetPath: String, replaceExisting: Bool) throws -> NativeJSONValue {
        let root = try workspaceRoot()
        let source = try entryURLWithoutFollowingLeaf(sourcePath, root: root)
        let target = try writableURL(targetPath, root: root)
        let sourceValues = try source.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        let isDirectory = sourceValues.isDirectory == true && sourceValues.isSymbolicLink != true
        if source == target {
            return moveValue(source: source, target: target, root: root, isDirectory: isDirectory, replaced: false, moved: false)
        }
        if isDirectory {
            let sourcePrefix = source.path.hasSuffix("/") ? source.path : source.path + "/"
            guard !target.path.hasPrefix(sourcePrefix) else { throw NativeWorkspaceRelayError.unsafePath }
        }
        var replaced = false
        if fileManager.fileExists(atPath: target.path) {
            guard replaceExisting else { throw NativeWorkspaceRelayError.alreadyExists }
            try fileManager.removeItem(at: target)
            replaced = true
        }
        try fileManager.moveItem(at: source, to: target)
        return moveValue(source: source, target: target, root: root, isDirectory: isDirectory, replaced: replaced, moved: true)
    }

    private func moveValue(
        source: URL,
        target: URL,
        root: URL,
        isDirectory: Bool,
        replaced: Bool,
        moved: Bool
    ) -> NativeJSONValue {
        .object([
            "from_path": .string(relativePath(source, root: root)),
            "to_path": .string(relativePath(target, root: root)),
            "name": .string(target.lastPathComponent),
            "is_dir": .bool(isDirectory),
            "replaced": .bool(replaced),
            "moved": .bool(moved),
        ])
    }

    private func workspaceRoot() throws -> URL {
        let raw = URL(fileURLWithPath: workspace.absoluteRoot, isDirectory: true).standardizedFileURL
        let root = raw.resolvingSymlinksInPath().standardizedFileURL
        let values = try resourceValues(root)
        guard values.isDirectory == true else { throw NativeWorkspaceRelayError.notDirectory }
        return root
    }

    private func existingURL(_ path: String, root: URL) throws -> URL {
        let components = try normalizedComponents(path, permitsRoot: true)
        let candidate = components.reduce(root) { $0.appendingPathComponent($1) }
        guard fileManager.fileExists(atPath: candidate.path) else { throw NativeWorkspaceRelayError.notFound }
        let resolved = candidate.resolvingSymlinksInPath().standardizedFileURL
        guard contains(resolved, root: root) else { throw NativeWorkspaceRelayError.unsafePath }
        return resolved
    }

    private func writableURL(_ path: String, root: URL) throws -> URL {
        let components = try normalizedComponents(path, permitsRoot: false)
        let target = components.reduce(root) { $0.appendingPathComponent($1) }
        try validateParent(target.deletingLastPathComponent(), root: root)
        if fileManager.fileExists(atPath: target.path) {
            let resolved = target.resolvingSymlinksInPath().standardizedFileURL
            guard resolved == target.standardizedFileURL, contains(resolved, root: root) else {
                throw NativeWorkspaceRelayError.symbolicLink
            }
        }
        return target.standardizedFileURL
    }

    private func entryURLWithoutFollowingLeaf(_ path: String, root: URL) throws -> URL {
        let components = try normalizedComponents(path, permitsRoot: false)
        let target = components.reduce(root) { $0.appendingPathComponent($1) }.standardizedFileURL
        try validateParent(target.deletingLastPathComponent(), root: root)
        guard fileManager.fileExists(atPath: target.path) else { throw NativeWorkspaceRelayError.notFound }
        return target
    }

    private func validateParent(_ parent: URL, root: URL) throws {
        guard fileManager.fileExists(atPath: parent.path) else { throw NativeWorkspaceRelayError.notFound }
        let resolved = parent.resolvingSymlinksInPath().standardizedFileURL
        guard resolved == parent.standardizedFileURL, contains(resolved, root: root) else {
            throw NativeWorkspaceRelayError.symbolicLink
        }
        guard try resourceValues(resolved).isDirectory == true else {
            throw NativeWorkspaceRelayError.notDirectory
        }
    }

    private func normalizedComponents(_ path: String, permitsRoot: Bool) throws -> [String] {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.hasPrefix("/"), !trimmed.contains("\0") else {
            throw NativeWorkspaceRelayError.unsafePath
        }
        var result: [String] = []
        for component in trimmed.split(separator: "/", omittingEmptySubsequences: true) {
            let value = String(component)
            if value == "." { continue }
            guard value != ".." else { throw NativeWorkspaceRelayError.unsafePath }
            result.append(value)
        }
        guard permitsRoot || !result.isEmpty else { throw NativeWorkspaceRelayError.rootMutation }
        return result
    }

    private func resourceValues(_ url: URL) throws -> URLResourceValues {
        do {
            return try url.resourceValues(forKeys: [
                .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey,
                .fileSizeKey, .contentModificationDateKey,
            ])
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            throw NativeWorkspaceRelayError.notFound
        }
    }

    private func contains(_ candidate: URL, root: URL) -> Bool {
        candidate.path == root.path || candidate.path.hasPrefix(root.path.hasSuffix("/") ? root.path : root.path + "/")
    }

    private func relativePath(_ url: URL, root: URL) -> String {
        let resolvedURL = url.standardizedFileURL.resolvingSymlinksInPath()
        let resolvedRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        if resolvedURL.path == resolvedRoot.path { return "." }
        let prefix = resolvedRoot.path.hasSuffix("/") ? resolvedRoot.path : resolvedRoot.path + "/"
        return resolvedURL.path.hasPrefix(prefix)
            ? String(resolvedURL.path.dropFirst(prefix.count))
            : "."
    }

    private func milliseconds(_ date: Date) -> Int64 {
        Int64(date.timeIntervalSince1970 * 1_000)
    }
}

private struct NativeWorkspaceEntry: Sendable {
    var name: String
    var path: String
    var isDirectory: Bool
    var size: Int64
    var modifiedAt: Int64?

    var jsonValue: NativeJSONValue {
        .object([
            "name": .string(name),
            "path": .string(path),
            "is_dir": .bool(isDirectory),
            "size": .number(Double(size)),
            "modified_at": modifiedAt.map { .number(Double($0)) } ?? .null,
        ])
    }
}
