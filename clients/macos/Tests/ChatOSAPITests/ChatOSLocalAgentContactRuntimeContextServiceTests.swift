// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation
@testable import ChatOSAPI
import XCTest

final class ChatOSLocalAgentContactRuntimeContextServiceTests: XCTestCase {
    func testFetchesOnlyReadOnlyContactRuntimeConfiguration() async throws {
        let transport = ContactRuntimeTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let context = try await ChatOSLocalAgentContactRuntimeContextService(client: client)
            .fetchRuntimeContext(agentID: "agent/design")

        XCTAssertEqual(context.agentID, "agent/design")
        XCTAssertEqual(context.name, "Visual Designer")
        XCTAssertEqual(context.roleDefinition, "Design polished interfaces.")
        XCTAssertEqual(
            context.skills,
            [
                LocalAgentContactSkill(
                    id: "visual-review",
                    name: "Visual review",
                    content: "Review hierarchy, rhythm and typography."
                ),
            ]
        )
        let path = await transport.path()
        XCTAssertEqual(path, "/api/chatos/agents/agent%2Fdesign/runtime-context")
    }
}

private actor ContactRuntimeTransport: HTTPTransport {
    private var requestPath: String?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requestPath = request.url.path(percentEncoded: true)
        let body = #"{"agent_id":"agent/design","user_id":"user-1","name":"Visual Designer","description":"Design partner","category":"design","role_definition":"Design polished interfaces.","skills":[{"id":"visual-review","name":"Visual review","content":"Review hierarchy, rhythm and typography."}],"updated_at":"2026-09-12T00:00:00Z"}"#
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func path() -> String? { requestPath }
}
