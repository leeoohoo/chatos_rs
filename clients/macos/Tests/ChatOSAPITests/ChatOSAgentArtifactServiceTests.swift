import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSAgentArtifactServiceTests: XCTestCase {
    func testUploadsCompletesAndDownloadsArtifactWithoutLeakingPresignedURL() async throws {
        let apiTransport = AgentArtifactAPITransport()
        let uploadTransport = AgentArtifactPUTTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "test-token",
            transport: apiTransport
        )
        let service = ChatOSAgentArtifactService(
            client: client,
            uploadTransport: uploadTransport
        )
        let data = Data("# 实施方案".utf8)
        let sha256 = "a".repeating(64)

        let metadata = try await service.upload(.init(
            name: "方案.md",
            mimeType: "text/markdown; charset=utf-8",
            data: data,
            sha256: sha256,
            idempotencyKey: "agent-attachment:local-1"
        ))
        let page = try await service.list(limit: 50, cursor: nil)
        let restored = try await service.download(artifactID: metadata.artifactID)

        XCTAssertEqual(metadata.artifactID, "artifact_0123456789abcdef0123456789abcdef")
        XCTAssertEqual(metadata.objectKey, "private/object-key")
        XCTAssertEqual(page.artifacts.map(\.artifactID), [metadata.artifactID])
        XCTAssertEqual(page.artifacts.first?.name, "方案.md")
        XCTAssertEqual(page.nextCursor, "next-page")
        XCTAssertEqual(restored, Data("# restored".utf8))
        let requests = await apiTransport.recordedRequests()
        XCTAssertEqual(requests.map(\.url.path), [
            "/api/chatos/agent-artifacts/uploads",
            "/api/chatos/agent-artifacts/artifact_0123456789abcdef0123456789abcdef/complete",
            "/api/chatos/agent-artifacts",
            "/api/chatos/agent-artifacts/artifact_0123456789abcdef0123456789abcdef/content",
        ])
        XCTAssertEqual(requests[2].url.query, "limit=50")
        let signingBody = try XCTUnwrap(requests.first?.body)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: signingBody) as? [String: Any])
        let item = try XCTUnwrap((json["artifacts"] as? [[String: Any]])?.first)
        XCTAssertEqual(item["idempotencyKey"] as? String, "agent-attachment:local-1")
        XCTAssertEqual(item["sha256"] as? String, sha256)
        XCTAssertEqual(requests.last?.headers["Authorization"], "Bearer test-token")
        let capturedUpload = await uploadTransport.recordedRequest()
        let upload = try XCTUnwrap(capturedUpload)
        XCTAssertEqual(upload.body, data)
        XCTAssertNil(upload.headers["Host"])
        XCTAssertEqual(upload.headers["Content-Type"], "text/markdown; charset=utf-8")
    }
}

private extension String {
    func repeating(_ count: Int) -> String { String(repeating: self, count: count) }
}

private actor AgentArtifactAPITransport: HTTPTransport {
    private var requests: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        switch request.url.path {
        case "/api/chatos/agent-artifacts/uploads":
            return .init(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"uploads":[{"artifactId":"artifact_0123456789abcdef0123456789abcdef","name":"方案.md","mimeType":"text/markdown; charset=utf-8","size":14,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","status":"staged","storageProvider":"minio","bucket":"agent-artifacts","objectKey":"private/object-key","uploadUrl":"https://storage.example/upload","uploadHeaders":{"Host":"storage.example"},"remoteViewPath":"/api/agent-artifacts/artifact_0123456789abcdef0123456789abcdef/content"}]}"#.utf8)
            )
        case let path where path.hasSuffix("/complete"):
            return .init(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"artifactId":"artifact_0123456789abcdef0123456789abcdef","name":"方案.md","mimeType":"text/markdown; charset=utf-8","size":14,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","status":"uploaded","remoteViewPath":"/api/agent-artifacts/artifact_0123456789abcdef0123456789abcdef/content"}"#.utf8)
            )
        case "/api/chatos/agent-artifacts":
            return .init(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"artifacts":[{"artifactId":"artifact_0123456789abcdef0123456789abcdef","name":"方案.md","mimeType":"text/markdown; charset=utf-8","size":14,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","status":"uploaded","remoteViewPath":"/api/agent-artifacts/artifact_0123456789abcdef0123456789abcdef/content","createdAt":"2026-09-20T03:00:00.123Z","updatedAt":"2026-09-20T03:00:01Z"}],"nextCursor":"next-page"}"#.utf8)
            )
        case let path where path.hasSuffix("/content"):
            return .init(
                statusCode: 200,
                headers: ["content-type": "text/markdown; charset=utf-8"],
                body: Data("# restored".utf8)
            )
        default:
            XCTFail("Unexpected API request: \(request.url.path)")
            return .init(statusCode: 404, headers: [:], body: Data())
        }
    }

    func recordedRequests() -> [HTTPRequest] { requests }
}

private actor AgentArtifactPUTTransport: HTTPTransport {
    private var request: HTTPRequest?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        self.request = request
        return .init(statusCode: 200, headers: [:], body: Data())
    }

    func recordedRequest() -> HTTPRequest? { request }
}
