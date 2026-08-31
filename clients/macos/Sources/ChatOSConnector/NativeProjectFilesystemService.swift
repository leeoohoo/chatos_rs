import AppKit
import ChatOSCore
import Foundation

public struct NativeProjectFilesystemService: ProjectFilesystemServicing, Sendable {
    private let connector: NativeLocalConnectorService

    public init(connector: NativeLocalConnectorService) {
        self.connector = connector
    }

    public func listEntries(path: String, forceRefresh: Bool) async throws -> ProjectDirectoryListing {
        let resolved = try await connector.resolveProjectPath(path)
        let value = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace)
                .list(path: resolved.relativePath, includeFiles: true)
        }.value
        let object = try value.objectValue()
        let entries = try object.array("entries").map { item -> ProjectFileEntry in
            let entry = try item.objectValue()
            let relative = try entry.string("path")
            return ProjectFileEntry(
                name: try entry.string("name"),
                path: resolved.logicalPath(for: relative),
                displayPath: "/" + (relative == "." ? "" : relative),
                isDirectory: try entry.bool("is_dir"),
                isWritable: true,
                size: entry.int64("size"),
                modifiedAt: entry.millisecondsDate("modified_at")
            )
        }
        let relativePath = try object.string("path")
        let parentRelative = object.optionalString("parent")
        return .init(
            path: resolved.logicalPath(for: relativePath),
            parentPath: parentRelative.map(resolved.logicalPath),
            isWritable: true,
            entries: entries,
            isTruncated: false
        )
    }

    public func searchEntries(path: String, query: String, limit: Int) async throws -> [ProjectFileEntry] {
        let resolved = try await connector.resolveProjectPath(path)
        let value = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace)
                .searchEntries(path: resolved.relativePath, query: query, limit: limit)
        }.value
        return try value.objectValue().array("matches").map { item in
            let entry = try item.objectValue()
            let relative = try entry.string("path")
            return ProjectFileEntry(
                name: try entry.string("name"),
                path: resolved.logicalPath(for: relative),
                displayPath: "/" + relative,
                isDirectory: try entry.bool("is_dir"),
                isWritable: true,
                size: entry.int64("size"),
                modifiedAt: entry.millisecondsDate("modified_at")
            )
        }
    }

    public func searchContent(path: String, query: String, limit: Int) async throws -> [ProjectFileContentMatch] {
        let resolved = try await connector.resolveProjectPath(path)
        let value = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace)
                .searchContent(path: resolved.relativePath, query: query, limit: limit)
        }.value
        return try value.objectValue().array("matches").map { item in
            let match = try item.objectValue()
            let relative = try match.string("path")
            return ProjectFileContentMatch(
                path: resolved.logicalPath(for: relative),
                displayPath: "/" + relative,
                line: Int(match.int64("line") ?? 1),
                column: Int(match.int64("column") ?? 1),
                text: try match.string("text")
            )
        }
    }

    public func readFile(path: String) async throws -> ProjectFileContent {
        let resolved = try await connector.resolveProjectPath(path)
        let value = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace).read(path: resolved.relativePath)
        }.value
        let object = try value.objectValue()
        return .init(
            path: path,
            displayPath: resolved.relativePath == "." ? resolved.absoluteURL.lastPathComponent : resolved.relativePath,
            name: resolved.absoluteURL.lastPathComponent,
            contentType: nil,
            isBinary: try object.bool("is_binary"),
            isWritable: FileManager.default.isWritableFile(atPath: resolved.absoluteURL.path),
            size: object.int64("size") ?? 0,
            modifiedAt: object.millisecondsDate("modified_at"),
            content: try object.string("content")
        )
    }

    public func writeFile(path: String, content: String) async throws {
        let resolved = try await connector.resolveProjectPath(path)
        _ = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace)
                .write(path: resolved.relativePath, content: content, createOnly: false)
        }.value
    }

    public func createFile(parentPath: String, name: String) async throws {
        let resolved = try await connector.resolveProjectPath(parentPath)
        let target = childPath(parent: resolved.relativePath, name: name)
        _ = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace)
                .write(path: target, content: "", createOnly: true)
        }.value
    }

    public func createDirectory(parentPath: String, name: String) async throws {
        let resolved = try await connector.resolveProjectPath(parentPath)
        let target = childPath(parent: resolved.relativePath, name: name)
        _ = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace).createDirectory(path: target)
        }.value
    }

    public func deleteEntry(path: String, recursive: Bool) async throws {
        let resolved = try await connector.resolveProjectPath(path)
        _ = try await Task.detached {
            try NativeWorkspaceFilesystem(workspace: resolved.workspace)
                .delete(path: resolved.relativePath, recursive: recursive)
        }.value
    }

    public func openExternally(path: String, mode: ProjectFileExternalOpenMode) async throws {
        let resolved = try await connector.resolveProjectPath(path)
        try await MainActor.run {
            switch mode {
            case .reveal:
                NSWorkspace.shared.activateFileViewerSelecting([resolved.absoluteURL])
            case .default:
                guard NSWorkspace.shared.open(resolved.absoluteURL) else {
                    throw NativeProjectFilesystemError.openFailed
                }
            case .code:
                let configuration = NSWorkspace.OpenConfiguration()
                NSWorkspace.shared.open(
                    [resolved.absoluteURL],
                    withApplicationAt: URL(fileURLWithPath: "/Applications/Visual Studio Code.app"),
                    configuration: configuration
                )
            }
        }
    }

    private func childPath(parent: String, name: String) -> String {
        parent == "." ? name : parent + "/" + name
    }
}

private enum NativeProjectFilesystemError: LocalizedError {
    case invalidPayload(String)
    case openFailed

    var errorDescription: String? {
        switch self {
        case let .invalidPayload(field): "本机文件系统返回的数据缺少字段：\(field)"
        case .openFailed: "无法使用默认应用打开文件"
        }
    }
}

private extension NativeJSONValue {
    func objectValue() throws -> [String: NativeJSONValue] {
        guard case let .object(value) = self else {
            throw NativeProjectFilesystemError.invalidPayload("object")
        }
        return value
    }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
    func string(_ key: String) throws -> String {
        guard case let .string(value)? = self[key] else {
            throw NativeProjectFilesystemError.invalidPayload(key)
        }
        return value
    }

    func optionalString(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func bool(_ key: String) throws -> Bool {
        guard case let .bool(value)? = self[key] else {
            throw NativeProjectFilesystemError.invalidPayload(key)
        }
        return value
    }

    func array(_ key: String) throws -> [NativeJSONValue] {
        guard case let .array(value)? = self[key] else {
            throw NativeProjectFilesystemError.invalidPayload(key)
        }
        return value
    }

    func int64(_ key: String) -> Int64? {
        guard case let .number(value)? = self[key] else { return nil }
        return Int64(value)
    }

    func millisecondsDate(_ key: String) -> Date? {
        int64(key).map { Date(timeIntervalSince1970: Double($0) / 1_000) }
    }
}
