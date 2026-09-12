// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Native Main Chat command boundary")
struct NativeLocalAgentConversationCommandServiceTests {
    @Test("freezes trusted routing and creates one authoritative local Run")
    func createsLocalRun() async throws {
        let fixture = try await Fixture.make()
        let attachment = ConversationAttachmentDraft(
            id: "attachment-1",
            name: "reference.png",
            mimeType: "image/png",
            kind: .image,
            origin: .file,
            data: Data("visual-reference".utf8)
        )

        let ack = try await fixture.service.sendNewTurn(
            ConversationSendCommand(
                sessionID: "conversation-1",
                turnID: "turn-1",
                messageID: "message-1",
                content: "  Design the hero with stronger hierarchy.  ",
                attachments: [attachment],
                reasoningEnabled: true
            )
        )

        #expect(ack == ConversationCommandAck(
            operationID: "operation-1",
            runID: "run-1",
            turnID: "turn-1",
            userMessageID: "message-1"
        ))
        #expect(await fixture.account.discardedReferences().isEmpty)
        #expect(await fixture.contacts.requestedAgentIDs() == ["agent-1"])
        #expect(await fixture.projects.requests() == ["user-1\u{0}project-1"])

        let request = try requestObject(from: await fixture.transport.lastRequestData())
        #expect(request["owner_user_id"] as? String == "user-1")
        let command = try #require(request["command"] as? [String: Any])
        #expect(command["type"] as? String == "create_main_chat_turn")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["thread_id"] as? String == "conversation-1")
        #expect(payload["turn_id"] as? String == "turn-1")
        #expect(payload["message_id"] as? String == "message-1")
        #expect(payload["project_id"] as? String == "project-1")
        #expect(payload["model_config_id"] as? String == "model-1")
        #expect(payload["content"] as? String == "Design the hero with stronger hierarchy.")
        #expect(payload["remote_connection_id"] == nil)
        #expect(payload["workspace_root"] == nil)
        let attachments = try #require(payload["attachments"] as? [[String: Any]])
        #expect(attachments.first?["payload_reference"] as? String ==
            "attachment-grant:grant-00000000-0000-0000-0000-000000000001")
        #expect(!String(describing: payload).contains("/Users/"))
        let projectSnapshot = try #require(payload["project_snapshot"] as? [String: Any])
        let projectPayload = try #require(projectSnapshot["payload"] as? [String: Any])
        #expect(projectPayload["project_id"] as? String == "project-1")
        #expect(projectPayload["workspace_id"] == nil)
    }

    @Test("cleans staged grants when IPC creation fails before an acknowledgement")
    func cleansAttachmentsOnCreationFailure() async throws {
        let fixture = try await Fixture.make(responseType: "wrong_request")

        await #expect(throws: NativeLocalAgentIPCError.self) {
            _ = try await fixture.service.sendNewTurn(
                ConversationSendCommand(
                    sessionID: "conversation-1",
                    turnID: "turn-1",
                    messageID: "message-1",
                    content: "Design it",
                    attachments: [Fixture.attachment]
                )
            )
        }

        #expect(await fixture.account.discardedReferences().count == 1)
    }

    @Test("cancels by authoritative local Run ID")
    func cancelsLocalRun() async throws {
        let fixture = try await Fixture.make(responseType: "accepted")

        try await fixture.service.cancelRun(runID: "run-1")

        let request = try requestObject(from: await fixture.transport.lastRequestData())
        let command = try #require(request["command"] as? [String: Any])
        #expect(command["type"] as? String == "cancel_run")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["run_id"] as? String == "run-1")
        #expect(payload["expected_version"] as? UInt64 == 3)
        #expect(await fixture.account.activeClientRequestCount() == 1)
    }
}

private struct Fixture {
    let service: NativeLocalAgentConversationCommandService
    let transport: MainChatCommandTransport
    let account: MainChatAccountSession
    let contacts: MainChatContactContexts
    let projects: MainChatProjects

    static let attachment = ConversationAttachmentDraft(
        id: "attachment-1",
        name: "reference.png",
        mimeType: "image/png",
        kind: .image,
        origin: .file,
        data: Data("visual-reference".utf8)
    )

    static func make(
        responseType: String = "run_created",
        returnedOwnerID: String = "user-1"
    ) async throws -> Fixture {
        let transport = MainChatCommandTransport(
            responseType: responseType,
            returnedOwnerID: returnedOwnerID
        )
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        let reference = LocalAgentAttachmentReference(
            attachmentID: "attachment-1",
            mediaType: "image/png",
            payloadReference: "attachment-grant:grant-00000000-0000-0000-0000-000000000001",
            payloadDigest: "sha256:" + String(repeating: "a", count: 64),
            byteSize: 16
        )
        let account = MainChatAccountSession(client: client, staged: [reference])
        let scopes = NativeLocalAgentConversationScopeStore()
        await scopes.update(
            conversations: [WorkspaceConversation(
                id: "conversation-1",
                title: "Design",
                projectID: "project-1",
                contactID: "contact-1",
                contactAgentID: "agent-1",
                messageCount: 0,
                updatedAt: .now,
                isArchived: false
            )],
            accountID: "user-1"
        )
        let contacts = MainChatContactContexts()
        let projects = MainChatProjects()
        return Fixture(
            service: NativeLocalAgentConversationCommandService(
                accountSession: account,
                scopes: scopes,
                runtimeSettings: MainChatRuntimeSettings(),
                contactContexts: contacts,
                projects: projects
            ),
            transport: transport,
            account: account,
            contacts: contacts,
            projects: projects
        )
    }
}

private actor MainChatCommandTransport: LocalAgentFrameTransport {
    private let responseType: String
    private let returnedOwnerID: String
    private var request: Data?

    init(responseType: String, returnedOwnerID: String) {
        self.responseType = responseType
        self.returnedOwnerID = returnedOwnerID
    }

    func exchange(_ data: Data) async throws -> Data {
        request = data
        let object = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let requestID = object["request_id"] as? String ?? "missing"
        let command = try #require(object["command"] as? [String: Any])
        let commandType = try #require(command["type"] as? String)
        let response: [String: Any]
        if commandType == "get_run" {
            response = ["type": "run", "payload": Self.runPayload(ownerUserID: returnedOwnerID)]
        } else if responseType == "accepted" {
            response = ["type": "accepted", "payload": ["operation_id": "operation-1"]]
        } else {
            response = [
                "type": "run_created",
                "payload": [
                    "operation_id": "operation-1",
                    "run": Self.runPayload(ownerUserID: returnedOwnerID),
                ],
            ]
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": responseType == "wrong_request" ? "another-request" : requestID,
            "response": response,
        ])
    }

    func lastRequestData() -> Data? { request }

    private static func runPayload(ownerUserID: String) -> [String: Any] {
        [
            "run_id": "run-1",
            "profile_key": "main_chat",
            "owner_user_id": ownerUserID,
            "owner_entity_type": "conversation",
            "owner_entity_id": "conversation-1",
            "project_id": "project-1",
            "status": "model_running",
            "version": 3,
            "step_seq": 1,
            "iteration": 0,
            "retry_count": 0,
            "model_config_id": "model-1",
            "model_config_revision": 1,
            "model_runtime_snapshot": [:],
            "context_strategy": "provider_native",
            "prompt_revision": "prompt-1",
            "capability_snapshot_ref": "main-chat-capabilities-v1",
            "created_at": "2026-09-12T03:00:00Z",
            "updated_at": "2026-09-12T03:00:01Z",
        ]
    }
}

private actor MainChatAccountSession: NativeLocalAgentAccountSessionAccess {
    private let ipcClient: NativeLocalAgentIPCClient
    private let staged: [LocalAgentAttachmentReference]
    private var discarded: [LocalAgentAttachmentReference] = []
    private var activeRequests = 0

    init(client: NativeLocalAgentIPCClient, staged: [LocalAgentAttachmentReference]) {
        ipcClient = client
        self.staged = staged
    }

    func client(accountID: String) async throws -> NativeLocalAgentIPCClient {
        guard accountID == "user-1" else { throw TestFailure.unexpected }
        return ipcClient
    }

    func activeClient() async throws -> NativeLocalAgentIPCClient {
        activeRequests += 1
        return ipcClient
    }

    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] {
        guard accountID == "user-1" else { throw TestFailure.unexpected }
        return attachments.isEmpty ? [] : staged
    }

    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {
        if accountID == "user-1" { discarded.append(contentsOf: references) }
    }

    func discardedReferences() -> [LocalAgentAttachmentReference] { discarded }
    func activeClientRequestCount() -> Int { activeRequests }
}

private struct MainChatRuntimeSettings: ConversationRuntimeSettingsServicing {
    func fetchSettings(sessionID: String) async throws -> ConversationRuntimeSettings {
        guard sessionID == "conversation-1" else { throw TestFailure.unexpected }
        return ConversationRuntimeSettings(selectedModelID: "model-1")
    }

    func fetchAvailableModels() async throws -> [ConversationModelOption] { [] }
    func updateModel(sessionID: String, modelID: String) async throws
        -> ConversationRuntimeSettings { throw TestFailure.unexpected }
    func updateRemoteConnection(sessionID: String, connectionID: String?) async throws
        -> ConversationRuntimeSettings { throw TestFailure.unexpected }
    func updateReasoning(sessionID: String, enabled: Bool) async throws
        -> ConversationRuntimeSettings { throw TestFailure.unexpected }
    func updateReasoningLevel(sessionID: String, level: String, enabled: Bool) async throws
        -> ConversationRuntimeSettings { throw TestFailure.unexpected }
}

private actor MainChatContactContexts: LocalAgentContactRuntimeContextServicing {
    private var requested: [String] = []

    func fetchRuntimeContext(agentID: String) async throws -> LocalAgentContactRuntimeContext {
        requested.append(agentID)
        return LocalAgentContactRuntimeContext(
            agentID: agentID,
            name: "Designer",
            description: nil,
            category: "design",
            roleDefinition: "Create polished visual systems.",
            skills: [],
            revision: "revision-1"
        )
    }

    func requestedAgentIDs() -> [String] { requested }
}

private actor MainChatProjects: NativeLocalAgentProjectRecordLoading {
    private var requested: [String] = []

    func localAgentProjectRecord(
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalProjectRecord {
        requested.append("\(ownerUserID)\u{0}\(projectID)")
        return LocalProjectRecord(
            id: projectID,
            ownerUserID: ownerUserID,
            draft: LocalProjectDraft(
                name: "Website",
                description: "Editorial product page",
                workspaceID: "workspace-private"
            ),
            revision: 4,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 2
        )
    }

    func requests() -> [String] { requested }
}

private enum TestFailure: Error {
    case unexpected
}

private func requestObject(from data: Data?) throws -> [String: Any] {
    let data = try #require(data)
    return try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
}
