import Foundation

public struct ProjectFileEntry: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { path }
    public var name: String
    public var path: String
    public var displayPath: String?
    public var isDirectory: Bool
    public var isWritable: Bool
    public var size: Int64?
    public var modifiedAt: Date?

    public init(
        name: String,
        path: String,
        displayPath: String? = nil,
        isDirectory: Bool,
        isWritable: Bool,
        size: Int64? = nil,
        modifiedAt: Date? = nil
    ) {
        self.name = name
        self.path = path
        self.displayPath = displayPath
        self.isDirectory = isDirectory
        self.isWritable = isWritable
        self.size = size
        self.modifiedAt = modifiedAt
    }
}

public struct ProjectDirectoryListing: Sendable, Equatable {
    public var path: String
    public var parentPath: String?
    public var isWritable: Bool
    public var entries: [ProjectFileEntry]
    public var isTruncated: Bool

    public init(
        path: String,
        parentPath: String?,
        isWritable: Bool,
        entries: [ProjectFileEntry],
        isTruncated: Bool
    ) {
        self.path = path
        self.parentPath = parentPath
        self.isWritable = isWritable
        self.entries = entries
        self.isTruncated = isTruncated
    }
}

public struct ProjectFileContent: Sendable, Equatable {
    public var path: String
    public var displayPath: String?
    public var name: String
    public var contentType: String?
    public var isBinary: Bool
    public var isWritable: Bool
    public var size: Int64
    public var modifiedAt: Date?
    public var content: String

    public init(
        path: String,
        displayPath: String? = nil,
        name: String,
        contentType: String?,
        isBinary: Bool,
        isWritable: Bool,
        size: Int64,
        modifiedAt: Date?,
        content: String
    ) {
        self.path = path
        self.displayPath = displayPath
        self.name = name
        self.contentType = contentType
        self.isBinary = isBinary
        self.isWritable = isWritable
        self.size = size
        self.modifiedAt = modifiedAt
        self.content = content
    }
}

public struct ProjectFileContentMatch: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { "\(path):\(line):\(column)" }
    public var path: String
    public var displayPath: String?
    public var line: Int
    public var column: Int
    public var text: String

    public init(
        path: String,
        displayPath: String? = nil,
        line: Int,
        column: Int,
        text: String
    ) {
        self.path = path
        self.displayPath = displayPath
        self.line = line
        self.column = column
        self.text = text
    }
}

public enum ProjectFileExternalOpenMode: String, Sendable {
    case `default`
    case reveal
    case code
}

public protocol ProjectFilesystemServicing: Sendable {
    func listEntries(path: String, forceRefresh: Bool) async throws -> ProjectDirectoryListing
    func searchEntries(path: String, query: String, limit: Int) async throws -> [ProjectFileEntry]
    func searchContent(path: String, query: String, limit: Int) async throws -> [ProjectFileContentMatch]
    func readFile(path: String) async throws -> ProjectFileContent
    func writeFile(path: String, content: String) async throws
    func createFile(parentPath: String, name: String) async throws
    func createDirectory(parentPath: String, name: String) async throws
    func deleteEntry(path: String, recursive: Bool) async throws
    func openExternally(path: String, mode: ProjectFileExternalOpenMode) async throws
}
