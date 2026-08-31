import ChatOSCore
import Foundation

public struct ChatOSProjectFilesystemService: ProjectFilesystemServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func listEntries(path: String, forceRefresh: Bool) async throws -> ProjectDirectoryListing {
        let response: EntriesDTO = try await client.request(
            "/fs/entries?path=\(path.queryEncoded)&force_refresh=\(forceRefresh)"
        )
        return response.domainModel(fallbackPath: path)
    }

    public func searchEntries(path: String, query: String, limit: Int) async throws -> [ProjectFileEntry] {
        let response: EntriesDTO = try await client.request(
            "/fs/search?path=\(path.queryEncoded)&q=\(query.queryEncoded)&limit=\(limit)"
        )
        return response.entries.map(\.domainModel)
    }

    public func searchContent(path: String, query: String, limit: Int) async throws -> [ProjectFileContentMatch] {
        let response: ContentSearchDTO = try await client.request(
            "/fs/search-content?path=\(path.queryEncoded)&q=\(query.queryEncoded)&limit=\(limit)"
        )
        return response.entries.map(\.domainModel)
    }

    public func readFile(path: String) async throws -> ProjectFileContent {
        let response: FileContentDTO = try await client.request("/fs/read?path=\(path.queryEncoded)")
        return response.domainModel
    }

    public func writeFile(path: String, content: String) async throws {
        let body = try JSONEncoder().encode(WriteRequest(path: path, content: content))
        let _: MutationDTO = try await client.request("/fs/write", method: "POST", body: body)
    }

    public func createFile(parentPath: String, name: String) async throws {
        let body = try JSONEncoder().encode(CreateRequest(parentPath: parentPath, name: name, content: ""))
        let _: MutationDTO = try await client.request("/fs/touch", method: "POST", body: body)
    }

    public func createDirectory(parentPath: String, name: String) async throws {
        let body = try JSONEncoder().encode(CreateRequest(parentPath: parentPath, name: name, content: nil))
        let _: MutationDTO = try await client.request("/fs/mkdir", method: "POST", body: body)
    }

    public func deleteEntry(path: String, recursive: Bool) async throws {
        let body = try JSONEncoder().encode(DeleteRequest(path: path, recursive: recursive))
        let _: MutationDTO = try await client.request("/fs/delete", method: "POST", body: body)
    }

    public func openExternally(path: String, mode: ProjectFileExternalOpenMode) async throws {
        let body = try JSONEncoder().encode(OpenRequest(path: path, mode: mode.rawValue))
        let _: MutationDTO = try await client.request("/fs/open", method: "POST", body: body)
    }
}

private struct EntriesDTO: Decodable, Sendable {
    var path: String?
    var parent: String?
    var writable: Bool?
    var entries: [EntryDTO] = []
    var truncated: Bool?

    func domainModel(fallbackPath: String) -> ProjectDirectoryListing {
        ProjectDirectoryListing(
            path: path ?? fallbackPath,
            parentPath: parent,
            isWritable: writable ?? false,
            entries: entries.map(\.domainModel),
            isTruncated: truncated ?? false
        )
    }
}

private struct EntryDTO: Decodable, Sendable {
    var name: String?
    var path: String?
    var displayPath: String?
    var isDirectory: Bool?
    var writable: Bool?
    var size: Int64?
    var modifiedAt: String?

    enum CodingKeys: String, CodingKey {
        case name, path, writable, size
        case displayPath = "display_path"
        case isDirectory = "is_dir"
        case modifiedAt = "modified_at"
    }

    var domainModel: ProjectFileEntry {
        ProjectFileEntry(
            name: name ?? URL(fileURLWithPath: path ?? "").lastPathComponent,
            path: path ?? "",
            displayPath: displayPath,
            isDirectory: isDirectory ?? false,
            isWritable: writable ?? false,
            size: size,
            modifiedAt: APIDateParser.parse(modifiedAt)
        )
    }
}

private struct FileContentDTO: Decodable, Sendable {
    var path: String?
    var displayPath: String?
    var relativePath: String?
    var name: String?
    var size: Int64?
    var contentType: String?
    var isBinary: Bool?
    var writable: Bool?
    var modifiedAt: String?
    var content: String?

    enum CodingKeys: String, CodingKey {
        case path, name, size, writable, content
        case displayPath = "display_path"
        case relativePath = "relative_path"
        case contentType = "content_type"
        case isBinary = "is_binary"
        case modifiedAt = "modified_at"
    }

    var domainModel: ProjectFileContent {
        ProjectFileContent(
            path: path ?? "",
            displayPath: displayPath ?? relativePath,
            name: name ?? URL(fileURLWithPath: path ?? "").lastPathComponent,
            contentType: contentType,
            isBinary: isBinary ?? false,
            isWritable: writable ?? false,
            size: size ?? 0,
            modifiedAt: APIDateParser.parse(modifiedAt),
            content: content ?? ""
        )
    }
}

private struct ContentSearchDTO: Decodable, Sendable {
    var entries: [ContentMatchDTO] = []
}

private struct ContentMatchDTO: Decodable, Sendable {
    var path: String?
    var relativePath: String?
    var line: Int?
    var column: Int?
    var text: String?

    enum CodingKeys: String, CodingKey {
        case path, line, column, text
        case relativePath = "relative_path"
    }

    var domainModel: ProjectFileContentMatch {
        ProjectFileContentMatch(
            path: path ?? relativePath ?? "",
            displayPath: relativePath,
            line: line ?? 1,
            column: column ?? 1,
            text: text ?? ""
        )
    }
}

private struct MutationDTO: Decodable, Sendable { var success: Bool? }
private struct WriteRequest: Encodable { var path: String; var content: String }
private struct DeleteRequest: Encodable { var path: String; var recursive: Bool }
private struct OpenRequest: Encodable { var path: String; var mode: String }
private struct CreateRequest: Encodable {
    var parentPath: String
    var name: String
    var content: String?
    enum CodingKeys: String, CodingKey { case parentPath = "parent_path"; case name, content }
}

private extension String {
    var queryEncoded: String {
        addingPercentEncoding(withAllowedCharacters: .urlQueryValueAllowed) ?? self
    }
}

private extension CharacterSet {
    static let urlQueryValueAllowed: CharacterSet = {
        var set = CharacterSet.urlQueryAllowed
        set.remove(charactersIn: "+&=?#")
        return set
    }()
}
