import Foundation

/// Shared filesystem contract for Plugin Skills and product-bundled Skills. It keeps both
/// surfaces on the same path, regular-file, symlink and pagination rules.
public enum ProgressiveSkillFileLoader {
    public struct TextPage: Sendable, Equatable {
        public let content: String
        public let offset: Int
        public let nextOffset: Int?
        public let truncated: Bool

        public init(content: String, offset: Int, nextOffset: Int?, truncated: Bool) {
            self.content = content
            self.offset = offset
            self.nextOffset = nextOffset
            self.truncated = truncated
        }
    }

    public enum LoaderError: LocalizedError {
        case invalidPath
        case unavailableDirectory
        case unavailableFile
        case fileTooLarge
        case invalidUTF8
        case invalidOffset

        public var errorDescription: String? {
            switch self {
            case .invalidPath: "Skill 资源路径无效"
            case .unavailableDirectory: "Skill 目录不存在、不是目录或是符号链接"
            case .unavailableFile: "Skill 资源不存在、不是普通文件或是符号链接"
            case .fileTooLarge: "Skill 资源超过大小限制"
            case .invalidUTF8: "Skill 文本资源不是 UTF-8"
            case .invalidOffset: "Skill 资源 offset 无效"
            }
        }
    }

    public static func normalizedRelativePath(_ value: String) throws -> String {
        var path = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if path.hasPrefix("./") { path.removeFirst(2) }
        let parts = path.split(separator: "/", omittingEmptySubsequences: false)
        guard !path.isEmpty, !path.hasPrefix("/"), parts.allSatisfy({
            !$0.isEmpty && $0 != "." && $0 != ".."
        }) else { throw LoaderError.invalidPath }
        return path
    }

    public static func relativePath(
        of fileURL: URL,
        beneath directoryURL: URL
    ) throws -> String {
        let directory = directoryURL.resolvingSymlinksInPath().standardizedFileURL
        let file = fileURL.resolvingSymlinksInPath().standardizedFileURL
        let prefix = directory.path + "/"
        guard file.path.hasPrefix(prefix) else { throw LoaderError.invalidPath }
        return try normalizedRelativePath(String(file.path.dropFirst(prefix.count)))
    }

    public static func validateDirectory(
        _ directoryURL: URL,
        beneath rootURL: URL,
        fileManager: FileManager = .default
    ) throws {
        let root = rootURL.standardizedFileURL
        let directory = directoryURL.standardizedFileURL
        guard directory.path.hasPrefix(root.path + "/"),
              fileManager.fileExists(atPath: directory.path) else {
            throw LoaderError.unavailableDirectory
        }
        let values = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true else {
            throw LoaderError.unavailableDirectory
        }
    }

    public static func readRegularFile(
        _ fileURL: URL,
        beneath rootURL: URL,
        maximumBytes: Int,
        fileManager: FileManager = .default
    ) throws -> Data {
        let root = rootURL.standardizedFileURL
        let file = fileURL.standardizedFileURL
        guard file.path.hasPrefix(root.path + "/"), fileManager.fileExists(atPath: file.path) else {
            throw LoaderError.unavailableFile
        }
        let values = try file.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
        ])
        guard values.isRegularFile == true, values.isSymbolicLink != true else {
            throw LoaderError.unavailableFile
        }
        guard values.fileSize ?? maximumBytes + 1 <= maximumBytes else {
            throw LoaderError.fileTooLarge
        }
        let handle = try FileHandle(forReadingFrom: file)
        defer { try? handle.close() }
        let data = try handle.read(upToCount: maximumBytes + 1) ?? Data()
        guard data.count <= maximumBytes else { throw LoaderError.fileTooLarge }
        return data
    }

    public static func readTextPage(
        _ fileURL: URL,
        beneath rootURL: URL,
        maximumBytes: Int,
        offset: Int,
        maximumCharacters: Int,
        fileManager: FileManager = .default
    ) throws -> TextPage {
        let data = try readRegularFile(
            fileURL,
            beneath: rootURL,
            maximumBytes: maximumBytes,
            fileManager: fileManager
        )
        guard let text = String(data: data, encoding: .utf8) else { throw LoaderError.invalidUTF8 }
        return try textPage(text, offset: offset, maximumCharacters: maximumCharacters)
    }

    /// Shared character pagination for both bundled filesystem references and immutable
    /// in-memory run snapshots.
    public static func textPage(
        _ text: String,
        offset: Int,
        maximumCharacters: Int
    ) throws -> TextPage {
        let characters = Array(text)
        guard offset >= 0, offset <= characters.count else { throw LoaderError.invalidOffset }
        let limit = min(max(maximumCharacters, 1), 64_000)
        let end = min(offset + limit, characters.count)
        return .init(
            content: String(characters[offset..<end]),
            offset: offset,
            nextOffset: end < characters.count ? end : nil,
            truncated: end < characters.count
        )
    }
}

/// Namespace for run-bound profession and project-type Skill projections.
public enum LocalAgentProgressiveSkillCatalog {}
