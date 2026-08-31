import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSAttachmentServiceTests: XCTestCase {
    func testSignsUploadsBytesAndReturnsChatPayload() async throws {
        let signingTransport = AttachmentSigningTransport()
        let uploadTransport = AttachmentUploadTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: signingTransport
        )
        let service = ChatOSAttachmentService(
            client: client,
            uploadTransport: uploadTransport
        )
        let data = Data("document body".utf8)

        let references = try await service.upload(
            [
                ConversationAttachmentDraft(
                    name: "notes.md",
                    mimeType: "text/markdown",
                    kind: .file,
                    origin: .file,
                    data: data
                ),
            ],
            conversationID: "conversation-1"
        )

        XCTAssertEqual(references.count, 1)
        XCTAssertEqual(references[0].name, "notes.md")
        XCTAssertEqual(references[0].objectKey, "conversation-1/notes.md")

        let capturedSigningRequest = await signingTransport.recordedRequest()
        let signingRequest = try XCTUnwrap(capturedSigningRequest)
        XCTAssertEqual(signingRequest.url.path, "/api/chatos/attachments/uploads")
        let signingBody = try XCTUnwrap(signingRequest.body)
        let signingJSON = try XCTUnwrap(
            JSONSerialization.jsonObject(with: signingBody) as? [String: Any]
        )
        XCTAssertEqual(signingJSON["conversation_id"] as? String, "conversation-1")
        let items = try XCTUnwrap(signingJSON["attachments"] as? [[String: Any]])
        XCTAssertEqual(items.first?["name"] as? String, "notes.md")
        XCTAssertEqual(items.first?["mimeType"] as? String, "text/markdown")

        let capturedUploadRequest = await uploadTransport.recordedRequest()
        let uploadRequest = try XCTUnwrap(capturedUploadRequest)
        XCTAssertEqual(uploadRequest.method, "PUT")
        XCTAssertEqual(uploadRequest.body, data)
        XCTAssertEqual(uploadRequest.headers["Content-Type"], "text/markdown")
        XCTAssertNil(uploadRequest.headers["Host"])
    }
}

private actor AttachmentSigningTransport: HTTPTransport {
    private var request: HTTPRequest?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        self.request = request
        return HTTPResponse(
            statusCode: 200,
            headers: [:],
            body: Data(#"{"uploads":[{"id":"attachment-1","name":"notes.md","mimeType":"text/markdown","size":13,"type":"file","storageProvider":"minio","bucket":"attachments","objectKey":"conversation-1/notes.md","uploadUrl":"https://storage.example/upload","uploadHeaders":{"Host":"storage.example"},"viewUrl":"/api/attachments/object?token=abc"}]}"#.utf8)
        )
    }

    func recordedRequest() -> HTTPRequest? { request }
}

private actor AttachmentUploadTransport: HTTPTransport {
    private var request: HTTPRequest?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        self.request = request
        return HTTPResponse(statusCode: 200, headers: [:], body: Data())
    }

    func recordedRequest() -> HTTPRequest? { request }
}
