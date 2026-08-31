import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSConversationCommandServiceTests: XCTestCase {
    func testNewTurnUsesRuntimeModelAndPostsChatCommand() async throws {
        let transport = CommandTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSConversationCommandService(client: client)

        let ack = try await service.sendNewTurn(
            ConversationSendCommand(
                sessionID: "conversation-1",
                turnID: "turn-1",
                content: "hello"
            )
        )

        XCTAssertTrue(ack.accepted)
        XCTAssertEqual(ack.userMessageID, "message-1")
        let capturedCommandRequest = await transport.commandRequest()
        let commandRequest = try XCTUnwrap(capturedCommandRequest)
        let body = try XCTUnwrap(commandRequest.body)
        let payload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: body) as? [String: Any]
        )
        XCTAssertEqual(payload["conversation_id"] as? String, "conversation-1")
        XCTAssertEqual(payload["turn_id"] as? String, "turn-1")
        XCTAssertEqual(payload["model_config_id"] as? String, "model-config-1")
        XCTAssertEqual(payload["plan_mode"] as? Bool, false)
        let model = try XCTUnwrap(payload["ai_model_config"] as? [String: Any])
        XCTAssertEqual(model["model_name"] as? String, "gpt-5.6-terra")
        XCTAssertEqual(model["thinking_level"] as? String, "medium")
    }

    func testCommandOverrideCarriesPlanModeToRustBackend() async throws {
        let transport = CommandTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        _ = try await ChatOSConversationCommandService(client: client).sendNewTurn(
            ConversationSendCommand(
                sessionID: "conversation-1",
                turnID: "turn-plan",
                content: "先规划",
                reasoningEnabled: true,
                planModeEnabled: true
            )
        )

        let capturedRequest = await transport.commandRequest()
        let request = try XCTUnwrap(capturedRequest)
        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(payload["plan_mode"] as? Bool, true)
        XCTAssertEqual(payload["reasoning_enabled"] as? Bool, true)
    }

    func testGuidanceUsesDedicatedEndpoint() async throws {
        let transport = CommandTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        _ = try await ChatOSConversationCommandService(client: client).sendGuidance(
            ConversationSendCommand(
                sessionID: "conversation-1",
                turnID: "turn-running",
                content: "continue"
            )
        )

        let capturedGuidanceRequest = await transport.guidanceRequest()
        let request = try XCTUnwrap(capturedGuidanceRequest)
        XCTAssertEqual(request.url.path, "/api/chatos/agent/chat/guidance")
    }

    func testStopTurnUsesConversationAndTurnScope() async throws {
        let transport = CommandTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        try await ChatOSConversationCommandService(client: client).stopTurn(
            conversationID: "conversation-1",
            turnID: "turn-running"
        )

        let capturedRequest = await transport.stopRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(request.url.path, "/api/chatos/agent/chat/stop")
        XCTAssertEqual(request.method, "POST")
        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(payload["conversation_id"] as? String, "conversation-1")
        XCTAssertEqual(payload["turn_id"] as? String, "turn-running")
    }

    func testUploadedAttachmentsAreIncludedInChatCommand() async throws {
        let transport = CommandTransport()
        let uploader = CommandAttachmentUploader()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSConversationCommandService(
            client: client,
            attachmentService: uploader
        )
        let draft = ConversationAttachmentDraft(
            id: "draft-1",
            name: "design.pdf",
            mimeType: "application/pdf",
            kind: .file,
            origin: .file,
            data: Data("pdf".utf8)
        )

        _ = try await service.sendNewTurn(
            ConversationSendCommand(
                sessionID: "conversation-1",
                turnID: "turn-attachment",
                content: "请查看附件",
                attachments: [draft]
            )
        )

        let capturedUpload = await uploader.recordedUpload()
        XCTAssertEqual(capturedUpload?.conversationID, "conversation-1")
        XCTAssertEqual(capturedUpload?.attachments, [draft])

        let capturedRequest = await transport.commandRequest()
        let request = try XCTUnwrap(capturedRequest)
        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let attachments = try XCTUnwrap(payload["attachments"] as? [[String: Any]])
        XCTAssertEqual(attachments.count, 1)
        XCTAssertEqual(attachments[0]["id"] as? String, "attachment-1")
        XCTAssertEqual(attachments[0]["name"] as? String, "design.pdf")
        XCTAssertEqual(attachments[0]["mimeType"] as? String, "application/pdf")
        XCTAssertEqual(attachments[0]["type"] as? String, "file")
        XCTAssertEqual(attachments[0]["objectKey"] as? String, "conversation-1/design.pdf")
        XCTAssertEqual(attachments[0]["viewUrl"] as? String, "/api/attachments/object?token=abc")
    }

    func testEndedGuidanceTurnMapsToDomainConflict() async throws {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: InactiveGuidanceTransport()
        )

        do {
            _ = try await ChatOSConversationCommandService(client: client).sendGuidance(
                ConversationSendCommand(
                    sessionID: "conversation-1",
                    turnID: "turn-ended",
                    content: "作为新消息发送"
                )
            )
            XCTFail("Expected inactive turn conflict")
        } catch let error as ConversationCommandError {
            XCTAssertEqual(error, .guidanceTargetInactive)
        }
    }
}

private actor InactiveGuidanceTransport: HTTPTransport {
    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        HTTPResponse(
            statusCode: 409,
            headers: ["content-type": "application/json"],
            body: Data(#"{"error":{"code":"turn_ended","message":"目标轮次已结束，无法追加指令"}}"#.utf8)
        )
    }
}

private actor CommandAttachmentUploader: ConversationAttachmentUploading {
    struct RecordedUpload: Sendable {
        var attachments: [ConversationAttachmentDraft]
        var conversationID: String
    }

    private var upload: RecordedUpload?

    func upload(
        _ attachments: [ConversationAttachmentDraft],
        conversationID: String
    ) async throws -> [ConversationAttachmentReference] {
        upload = RecordedUpload(attachments: attachments, conversationID: conversationID)
        return [
            ConversationAttachmentReference(
                id: "attachment-1",
                name: "design.pdf",
                mimeType: "application/pdf",
                size: 3,
                kind: .file,
                storageProvider: "minio",
                bucket: "attachments",
                objectKey: "conversation-1/design.pdf",
                viewURL: "/api/attachments/object?token=abc"
            ),
        ]
    }

    func recordedUpload() -> RecordedUpload? { upload }
}

private actor CommandTransport: HTTPTransport {
    private var requests: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let body: String
        switch request.url.path {
        case "/api/chatos/conversations/conversation-1/runtime-settings":
            body = #"{"selected_model_id":"model-config-1","selected_model_name":"gpt-5.6-luna","selected_thinking_level":"high","remote_connection_id":null,"workspace_root":"/workspace","reasoning_enabled":true,"plan_mode_enabled":false}"#
        case "/api/chatos/ai-model-configs":
            body = #"[{"id":"model-config-1","name":"Terra","model_name":"gpt-5.6-terra","thinking_level":"medium","temperature":0.5,"enabled":true}]"#
        case "/api/chatos/agent/chat/send":
            body = #"{"accepted":true,"turn_id":"turn-1","user_message_id":"message-1"}"#
        case "/api/chatos/agent/chat/guidance":
            body = #"{"accepted":true,"turn_id":"turn-running"}"#
        case "/api/chatos/agent/chat/stop":
            body = #"{"success":true,"message":"停止中"}"#
        default:
            return HTTPResponse(statusCode: 404, headers: [:], body: Data())
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func commandRequest() -> HTTPRequest? {
        requests.first { $0.url.path.hasSuffix("/agent/chat/send") }
    }

    func guidanceRequest() -> HTTPRequest? {
        requests.first { $0.url.path.hasSuffix("/agent/chat/guidance") }
    }

    func stopRequest() -> HTTPRequest? {
        requests.first { $0.url.path.hasSuffix("/agent/chat/stop") }
    }
}
