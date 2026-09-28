import ChatOSCore
import Foundation

public struct ChatOSNotepadService: NotepadServicing {
    private let client: ChatOSAPIClient
    private let uploadTransport: any HTTPTransport

    public init(
        client: ChatOSAPIClient,
        uploadTransport: any HTTPTransport = URLSessionHTTPTransport()
    ) {
        self.client = client
        self.uploadTransport = uploadTransport
    }

    public func initialize() async throws {
        let _: SimpleResponse = try await client.request("/notepad/init")
    }

    public func listFolders() async throws -> [String] {
        let response: FoldersResponse = try await client.request("/notepad/folders")
        return response.folders ?? []
    }

    public func createFolder(_ folder: String) async throws {
        let body = try JSONEncoder().encode(FolderRequest(folder: folder))
        let _: SimpleResponse = try await client.request(
            "/notepad/folders",
            method: "POST",
            body: body
        )
    }

    public func renameFolder(from: String, to: String) async throws {
        let body = try JSONEncoder().encode(RenameFolderRequest(from: from, to: to))
        let _: SimpleResponse = try await client.request(
            "/notepad/folders",
            method: "PATCH",
            body: body
        )
    }

    public func deleteFolder(_ folder: String, recursive: Bool) async throws {
        var components = URLComponents()
        components.queryItems = [
            URLQueryItem(name: "folder", value: folder),
            URLQueryItem(name: "recursive", value: recursive ? "true" : "false"),
        ]
        let suffix = components.percentEncodedQuery.map { "?\($0)" } ?? ""
        let endpoint = "/notepad/folders\(suffix)"
        let _: SimpleResponse = try await client.request(endpoint, method: "DELETE")
    }

    public func listNotes(query: String?, limit: Int) async throws -> [NotepadNote] {
        var components = URLComponents()
        var items = [
            URLQueryItem(name: "recursive", value: "true"),
            URLQueryItem(name: "limit", value: String(min(max(limit, 1), 500))),
        ]
        if let query = query?.trimmingCharacters(in: .whitespacesAndNewlines), !query.isEmpty {
            items.append(URLQueryItem(name: "query", value: query))
        }
        components.queryItems = items
        let suffix = components.percentEncodedQuery.map { "?\($0)" } ?? ""
        let response: NotesResponse = try await client.request("/notepad/notes\(suffix)")
        return (response.notes ?? []).map(\.domainModel)
    }

    public func createNote(_ draft: NotepadNoteDraft) async throws -> NotepadNoteDetail {
        let body = try JSONEncoder().encode(NoteDraftDTO(draft))
        let response: NoteDetailResponse = try await client.request(
            "/notepad/notes",
            method: "POST",
            body: body
        )
        return try response.domainModel()
    }

    public func fetchNote(id: String) async throws -> NotepadNoteDetail {
        let response: NoteDetailResponse = try await client.request(
            "/notepad/notes/\(id.notepadPathEncoded)"
        )
        return try response.domainModel()
    }

    public func updateNote(id: String, update: NotepadNoteUpdate) async throws -> NotepadNoteDetail {
        let body = try JSONEncoder().encode(NoteUpdateDTO(update))
        let response: NoteDetailResponse = try await client.request(
            "/notepad/notes/\(id.notepadPathEncoded)",
            method: "PATCH",
            body: body
        )
        return try response.domainModel()
    }

    public func uploadImage(
        _ image: NotepadImageUpload,
        noteID: String
    ) async throws -> NotepadImageAsset {
        let body = try JSONEncoder().encode(MediaUploadsRequestDTO(assets: [
            .init(name: image.name, mimeType: image.mimeType, size: image.data.count),
        ]))
        let response: MediaUploadsResponseDTO = try await client.request(
            "/media/uploads",
            method: "POST",
            body: body
        )
        guard let target = response.uploads.first,
              let uploadURL = URL(string: target.uploadURL) else {
            throw ChatOSAPIError.decoding("记事本图片上传地址无效")
        }
        var headers = (target.uploadHeaders ?? [:]).filter { key, _ in
            key.caseInsensitiveCompare("Host") != .orderedSame
                && key.caseInsensitiveCompare("Content-Length") != .orderedSame
        }
        if !headers.keys.contains(where: { $0.caseInsensitiveCompare("Content-Type") == .orderedSame }) {
            headers["Content-Type"] = image.mimeType
        }
        let uploadResponse = try await uploadTransport.send(HTTPRequest(
            url: uploadURL,
            method: "PUT",
            headers: headers,
            body: image.data
        ))
        guard (200..<300).contains(uploadResponse.statusCode) else {
            throw ChatOSAPIError.server(
                statusCode: uploadResponse.statusCode,
                message: "记事本图片上传失败"
            )
        }
        guard let resolvedURL = await client.resolvePublicURL(target.viewURL ?? target.url ?? "") else {
            throw ChatOSAPIError.decoding("记事本图片访问地址无效")
        }
        return NotepadImageAsset(
            url: resolvedURL,
            mimeType: target.mimeType,
            name: target.name,
            size: target.size
        )
    }

    public func deleteNote(id: String) async throws {
        let _: SimpleResponse = try await client.request(
            "/notepad/notes/\(id.notepadPathEncoded)",
            method: "DELETE"
        )
    }
}

private struct SimpleResponse: Decodable, Sendable {
    var ok: Bool?
}

private struct MediaUploadsRequestDTO: Encodable {
    var assets: [MediaUploadItemDTO]
}

private struct MediaUploadItemDTO: Encodable {
    var name: String
    var mimeType: String
    var size: Int
}

private struct MediaUploadsResponseDTO: Decodable, Sendable {
    var uploads: [MediaUploadTargetDTO]
}

private struct MediaUploadTargetDTO: Decodable, Sendable {
    var name: String
    var mimeType: String
    var size: Int
    var uploadURL: String
    var uploadHeaders: [String: String]?
    var url: String?
    var viewURL: String?
}

private struct FoldersResponse: Decodable, Sendable {
    var folders: [String]?
}

private struct NotesResponse: Decodable, Sendable {
    var notes: [NoteDTO]?
}

private struct NoteDetailResponse: Decodable, Sendable {
    var note: NoteDTO?
    var content: String?

    func domainModel() throws -> NotepadNoteDetail {
        guard let note else {
            throw ChatOSAPIError.decoding("记事本响应缺少 note")
        }
        return NotepadNoteDetail(note: note.domainModel, content: content ?? "")
    }
}

private struct NoteDTO: Decodable, Sendable {
    var id: String
    var title: String?
    var folder: String?
    var tags: [String]?
    var createdAt: String?
    var updatedAt: String?
    var file: String?

    enum CodingKeys: String, CodingKey {
        case id, title, folder, tags, file
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    var domainModel: NotepadNote {
        NotepadNote(
            id: id,
            title: title ?? "",
            folder: folder ?? "",
            tags: tags ?? [],
            createdAt: APIDateParser.parse(createdAt),
            updatedAt: APIDateParser.parse(updatedAt),
            file: file ?? ""
        )
    }
}

private struct FolderRequest: Encodable {
    var folder: String
}

private struct RenameFolderRequest: Encodable {
    var from: String
    var to: String
}

private struct NoteDraftDTO: Encodable {
    var folder: String
    var title: String
    var content: String
    var tags: [String]

    init(_ draft: NotepadNoteDraft) {
        folder = draft.folder
        title = draft.title
        content = draft.content
        tags = draft.tags
    }
}

private struct NoteUpdateDTO: Encodable {
    var title: String?
    var content: String?
    var folder: String?
    var tags: [String]?

    init(_ update: NotepadNoteUpdate) {
        title = update.title
        content = update.content
        folder = update.folder
        tags = update.tags
    }
}

private extension String {
    var notepadPathEncoded: String {
        var allowed = CharacterSet.urlPathAllowed
        allowed.remove(charactersIn: "/?#")
        return addingPercentEncoding(withAllowedCharacters: allowed) ?? self
    }
}
