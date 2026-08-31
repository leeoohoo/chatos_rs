import Foundation

public enum RemoteFileKind: String, Sendable, Equatable, Codable {
    case directory
    case file
    case symlink
    case other
}

public struct RemoteFileEntry: Identifiable, Sendable, Equatable {
    public var id: String { path }
    public var name: String
    public var path: String
    public var kind: RemoteFileKind
    public var size: Int64?
    public var modifiedAt: Date?
    public var permissions: String?

    public init(
        name: String,
        path: String,
        kind: RemoteFileKind,
        size: Int64?,
        modifiedAt: Date?,
        permissions: String?
    ) {
        self.name = name
        self.path = path
        self.kind = kind
        self.size = size
        self.modifiedAt = modifiedAt
        self.permissions = permissions
    }

    public var isDirectory: Bool { kind == .directory }
}

public struct RemoteDirectoryListing: Sendable, Equatable {
    public var path: String
    public var parentPath: String?
    public var entries: [RemoteFileEntry]

    public init(path: String, parentPath: String?, entries: [RemoteFileEntry]) {
        self.path = path
        self.parentPath = parentPath
        self.entries = entries
    }
}

public protocol RemoteFileServicing: Sendable {
    func initialDirectory(connectionID: String) async throws -> String
    func listDirectory(connectionID: String, path: String) async throws -> RemoteDirectoryListing
    func uploadFile(
        connectionID: String,
        localURL: URL,
        remoteDirectory: String,
        overwrite: Bool
    ) async throws -> String
    func downloadFile(
        connectionID: String,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws
    func createDirectory(connectionID: String, parentPath: String, name: String) async throws
    func renameEntry(connectionID: String, path: String, newName: String) async throws
    func deleteEntry(connectionID: String, path: String, recursively: Bool) async throws
}
