import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSNotepadServiceTests: XCTestCase {
    func testListsFoldersAndSearchesNotesUsingExistingNotepadAPI() async throws {
        let transport = NotepadTransport()
        let service = makeService(transport: transport)

        try await service.initialize()
        let folders = try await service.listFolders()
        let notes = try await service.listNotes(query: "架构 设计", limit: 900)

        XCTAssertEqual(folders, ["", "项目/设计"])
        XCTAssertEqual(notes.first?.title, "架构草稿")
        XCTAssertEqual(notes.first?.folder, "项目/设计")
        XCTAssertNotNil(notes.first?.updatedAt)

        let requests = await transport.recordedRequests()
        let notesRequest = try XCTUnwrap(requests.first(where: { $0.url.path.hasSuffix("/notepad/notes") }))
        let queryItems = URLComponents(url: notesRequest.url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        XCTAssertEqual(queryItems.first(where: { $0.name == "query" })?.value, "架构 设计")
        XCTAssertEqual(queryItems.first(where: { $0.name == "limit" })?.value, "500")
        XCTAssertEqual(queryItems.first(where: { $0.name == "recursive" })?.value, "true")
    }

    func testCreateUpdateAndDeletePreserveNotepadPayloads() async throws {
        let transport = NotepadTransport()
        let service = makeService(transport: transport)

        let created = try await service.createNote(.init(
            folder: "项目/设计",
            title: "架构草稿",
            content: "# 初稿",
            tags: ["架构", "草稿"]
        ))
        XCTAssertEqual(created.content, "# 初稿")

        let updated = try await service.updateNote(
            id: "note/1",
            update: .init(title: "架构方案", content: "# 定稿", tags: ["架构"])
        )
        XCTAssertEqual(updated.note.title, "架构方案")
        XCTAssertEqual(updated.content, "# 定稿")

        try await service.createFolder("项目/新目录")
        try await service.deleteFolder("项目/新目录", recursive: true)
        try await service.deleteNote(id: "note/1")

        let requests = await transport.recordedRequests()
        let createRequest = try XCTUnwrap(requests.first(where: {
            $0.method == "POST" && $0.url.path.hasSuffix("/notepad/notes")
        }))
        let createBody = try XCTUnwrap(createRequest.body)
        let createJSON = try XCTUnwrap(JSONSerialization.jsonObject(with: createBody) as? [String: Any])
        XCTAssertEqual(createJSON["folder"] as? String, "项目/设计")
        XCTAssertEqual(createJSON["content"] as? String, "# 初稿")

        let updateRequest = try XCTUnwrap(requests.first(where: { $0.method == "PATCH" }))
        let encodedUpdatePath = URLComponents(
            url: updateRequest.url,
            resolvingAgainstBaseURL: false
        )?.percentEncodedPath
        XCTAssertTrue(encodedUpdatePath?.hasSuffix("/notepad/notes/note%2F1") == true)
        let deleteFolderRequest = try XCTUnwrap(requests.first(where: {
            $0.method == "DELETE" && $0.url.path.hasSuffix("/notepad/folders")
        }))
        let deleteQuery = URLComponents(url: deleteFolderRequest.url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        XCTAssertEqual(deleteQuery.first(where: { $0.name == "folder" })?.value, "项目/新目录")
        XCTAssertEqual(deleteQuery.first(where: { $0.name == "recursive" })?.value, "true")
    }

    private func makeService(transport: NotepadTransport) -> ChatOSNotepadService {
        ChatOSNotepadService(
            client: ChatOSAPIClient(
                configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
                accessToken: "token",
                transport: transport
            )
        )
    }
}

private actor NotepadTransport: HTTPTransport {
    private var requests: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let path = request.url.path
        let body: String
        switch (request.method, path) {
        case ("GET", let path) where path.hasSuffix("/notepad/init"):
            body = #"{"ok":true}"#
        case ("GET", let path) where path.hasSuffix("/notepad/folders"):
            body = #"{"ok":true,"folders":["","项目/设计"]}"#
        case ("GET", let path) where path.hasSuffix("/notepad/notes"):
            body = #"{"ok":true,"notes":[{"id":"note/1","title":"架构草稿","folder":"项目/设计","tags":["架构"],"created_at":"2026-08-27T01:00:00Z","updated_at":"2026-08-27T02:00:00Z","file":"notes/项目/设计/note-1.md"}]}"#
        case ("POST", let path) where path.hasSuffix("/notepad/notes"):
            body = noteDetailJSON(content: "# 初稿")
        case ("PATCH", let path) where path.contains("/notepad/notes/"):
            body = noteDetailJSON(title: "架构方案", content: "# 定稿")
        default:
            body = #"{"ok":true}"#
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func recordedRequests() -> [HTTPRequest] { requests }

    private func noteDetailJSON(
        title: String = "架构草稿",
        content: String?
    ) -> String {
        let contentValue = content.map { "\"\($0)\"" } ?? "null"
        return """
        {"note":{"id":"note/1","title":"\(title)","folder":"项目/设计","tags":["架构"],"created_at":"2026-08-27T01:00:00Z","updated_at":"2026-08-27T02:00:00Z","file":"notes/项目/设计/note-1.md"},"content":\(contentValue)}
        """
    }
}
