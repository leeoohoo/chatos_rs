import ChatOSAPI
import XCTest

final class ChatOSAttachmentURLResolverTests: XCTestCase {
    func testAttachmentObjectPathUsesGatewayAlias() throws {
        let url = try XCTUnwrap(ChatOSAttachmentURLResolver.resolve(
            "/api/attachments/object?token=signed%2Etoken",
            apiBaseURL: URL(string: "http://127.0.0.1:9080/api/chatos")!
        ))

        XCTAssertEqual(
            url.absoluteString,
            "http://127.0.0.1:9080/api/chatos/attachments/object?token=signed%2Etoken"
        )
    }

    func testAbsoluteAttachmentURLIsNotRewritten() throws {
        let url = try XCTUnwrap(ChatOSAttachmentURLResolver.resolve(
            "https://cdn.example.com/image.png",
            apiBaseURL: URL(string: "http://127.0.0.1:9080/api/chatos")!
        ))

        XCTAssertEqual(url.absoluteString, "https://cdn.example.com/image.png")
    }
}
