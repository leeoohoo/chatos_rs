import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSUserLanguagePreferencesServiceTests: XCTestCase {
    func testFetchUsesEffectiveServerLanguages() async throws {
        let transport = UserLanguageSettingsTransport()
        let service = makeService(transport: transport)

        let preferences = try await service.fetch()

        XCTAssertEqual(preferences.interfaceLanguage, .english)
        XCTAssertEqual(preferences.internalContextLanguage, .simplifiedChinese)
    }

    func testUpdateSendsBothAuthoritativePreferenceKeys() async throws {
        let transport = UserLanguageSettingsTransport()
        let service = makeService(transport: transport)

        _ = try await service.update(
            userID: "user-1",
            preferences: .init(
                interfaceLanguage: .simplifiedChinese,
                internalContextLanguage: .english
            )
        )

        let capturedRequest = await transport.updateRequest()
        let request = try XCTUnwrap(capturedRequest)
        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let settings = try XCTUnwrap(payload["settings"] as? [String: Any])
        XCTAssertEqual(payload["user_id"] as? String, "user-1")
        XCTAssertEqual(settings["UI_LOCALE"] as? String, "zh-CN")
        XCTAssertEqual(settings["INTERNAL_CONTEXT_LOCALE"] as? String, "en-US")
    }

    private func makeService(
        transport: UserLanguageSettingsTransport
    ) -> ChatOSUserLanguagePreferencesService {
        ChatOSUserLanguagePreferencesService(
            client: ChatOSAPIClient(
                configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
                accessToken: "token",
                transport: transport
            )
        )
    }
}

private actor UserLanguageSettingsTransport: HTTPTransport {
    private var requests: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let body: String
        if request.method == "PUT" {
            body = #"{"effective":{"UI_LOCALE":"zh-CN","INTERNAL_CONTEXT_LOCALE":"en-US"}}"#
        } else {
            body = #"{"effective":{"UI_LOCALE":"en-US","INTERNAL_CONTEXT_LOCALE":"zh-CN"}}"#
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func updateRequest() -> HTTPRequest? {
        requests.first(where: { $0.method == "PUT" })
    }
}
