import Foundation

public struct ProjectCodeSymbolSelection: Sendable, Equatable {
    public var token: String
    public var line: Int
    public var column: Int

    public init(token: String, line: Int, column: Int) {
        self.token = token
        self.line = line
        self.column = column
    }
}

public struct ProjectCodeNavigationLocation: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { "\(path):\(line):\(column):\(endLine):\(endColumn)" }
    public var path: String
    public var relativePath: String
    public var line: Int
    public var column: Int
    public var endLine: Int
    public var endColumn: Int
    public var preview: String
    public var score: Double

    public init(
        path: String,
        relativePath: String,
        line: Int,
        column: Int,
        endLine: Int,
        endColumn: Int,
        preview: String,
        score: Double
    ) {
        self.path = path
        self.relativePath = relativePath
        self.line = line
        self.column = column
        self.endLine = endLine
        self.endColumn = endColumn
        self.preview = preview
        self.score = score
    }
}

public struct ProjectCodeNavigationResult: Sendable, Equatable {
    public var provider: String
    public var language: String
    public var mode: String
    public var token: String?
    public var locations: [ProjectCodeNavigationLocation]

    public init(
        provider: String,
        language: String,
        mode: String,
        token: String?,
        locations: [ProjectCodeNavigationLocation]
    ) {
        self.provider = provider
        self.language = language
        self.mode = mode
        self.token = token
        self.locations = locations
    }
}

public protocol ProjectCodeNavigationServicing: Sendable {
    func definition(
        projectRoot: String,
        filePath: String,
        line: Int,
        column: Int
    ) async throws -> ProjectCodeNavigationResult

    func references(
        projectRoot: String,
        filePath: String,
        line: Int,
        column: Int
    ) async throws -> ProjectCodeNavigationResult
}
