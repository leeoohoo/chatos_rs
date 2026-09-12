// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native local Agent Ask User service")
struct NativeLocalAgentAskUserPromptServiceTests {
    @Test("lists a typed prompt and sends the selected answer to its exact Run")
    func submitsSelection() async throws {
        let fixture = try await AskUserFixture.make()
        let prompts = try await fixture.service.fetchPrompts(sessionID: "thread-1", limit: 100)
        let prompt = try #require(prompts.first)
        #expect(prompt.id == "interaction-1")
        #expect(prompt.turnID == "turn-1")
        #expect(prompt.choice?.options.map(\.value) == ["visual-a", "visual-b"])

        let updated = try await fixture.service.submit(
            promptID: prompt.id,
            sessionID: prompt.sessionID,
            submission: AskUserSubmission(selection: .single("visual-b"))
        )

        #expect(updated.status == .ok)
        let request = try #require(await fixture.transport.lastRequest())
        let command = try commandObject(request)
        #expect(command["type"] as? String == "answer_user_question")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["run_id"] as? String == "run-1")
        #expect(payload["interaction_id"] as? String == "interaction-1")
        let answer = try #require(payload["answer"] as? [String: Any])
        #expect(answer["selected_option_ids"] as? [String] == ["visual-b"])
        #expect(try await fixture.service.fetchPrompts(
            sessionID: "thread-1",
            limit: 100
        ).first?.status == .ok)
    }

    @Test("cancelling a prompt cancels its owning Run")
    func cancelsRun() async throws {
        let fixture = try await AskUserFixture.make()
        let updated = try await fixture.service.cancel(
            promptID: "interaction-1",
            sessionID: "thread-1"
        )

        #expect(updated.status == .canceled)
        let command = try commandObject(try #require(await fixture.transport.lastRequest()))
        #expect(command["type"] as? String == "cancel_run")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["run_id"] as? String == "run-1")
        #expect(payload["expected_version"] as? UInt64 == 4)
    }

    @Test("an empty answer is rejected before IPC")
    func rejectsEmptyAnswer() async throws {
        let fixture = try await AskUserFixture.make(options: [])

        await #expect(throws: NativeLocalAgentAskUserPromptError.invalidAnswer) {
            _ = try await fixture.service.submit(
                promptID: "interaction-1",
                sessionID: "thread-1",
                submission: AskUserSubmission(values: ["answer": "   "])
            )
        }
        #expect(await fixture.transport.lastRequest() == nil)
    }

    @Test("Task Runner Ask User is answered through the same native Host service")
    func submitsTaskRunnerSelection() async throws {
        let taskState = LocalAgentTaskStateStore()
        let task = askUserTaskSnapshot()
        let run = askUserTaskRunSnapshot()
        try await taskState.restoreLocalAgentTasks([task], runs: [run])
        try await taskState.applyLocalAgentTaskEvent(.init(
            eventSeq: 11,
            emittedAt: askUserTimestamp,
            event: .userInteraction(.init(
                interactionID: "task-interaction-1",
                runID: run.runID,
                prompt: "Which Task visual direction should I continue?",
                options: [
                    .init(optionID: "visual-a", label: "Direction A"),
                    .init(optionID: "visual-b", label: "Direction B"),
                ],
                imageReferences: ["reference://task-preview"]
            ))
        ))
        let interactionState = LocalAgentUnifiedInteractionState(
            mainChat: ConversationHistoryStore(),
            taskRunner: taskState
        )
        let transport = AskUserTransport()
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        let service = NativeLocalAgentAskUserPromptService(
            accountSession: AskUserAccountSession(client: client),
            state: interactionState
        )

        let updated = try await service.submit(
            promptID: "task-interaction-1",
            sessionID: "thread-1",
            submission: AskUserSubmission(selection: .single("visual-b"))
        )

        #expect(updated.status == .ok)
        let command = try commandObject(try #require(await transport.lastRequest()))
        #expect(command["type"] as? String == "answer_user_question")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["run_id"] as? String == run.runID)
        #expect(payload["interaction_id"] as? String == "task-interaction-1")
        let answer = try #require(payload["answer"] as? [String: Any])
        #expect(answer["selected_option_ids"] as? [String] == ["visual-b"])
    }
}

private struct AskUserFixture {
    var service: NativeLocalAgentAskUserPromptService
    var transport: AskUserTransport

    static func make(
        options: [LocalAgentUserInteractionOption] = [
            .init(optionID: "visual-a", label: "方向 A"),
            .init(optionID: "visual-b", label: "方向 B"),
        ]
    ) async throws -> AskUserFixture {
        let store = ConversationHistoryStore()
        let binding = LocalAgentMainChatRunBinding(
            runID: "run-1",
            threadID: "thread-1",
            turnID: "turn-1",
            messageID: "message-1",
            userMessage: LocalAgentStoredMessage(
                recordID: "message-1",
                runID: "run-1",
                threadID: "thread-1",
                turnID: "turn-1",
                sequence: 1,
                role: .user,
                content: "Design it",
                messageMode: .semantic,
                messageSource: "main_chat",
                memorySyncStatus: .pending,
                createdAt: askUserTimestamp
            )
        )
        try await store.applyLocalAgentUIEvent(
            LocalAgentUIEvent(
                eventSeq: 1,
                emittedAt: askUserTimestamp,
                event: .userInteraction(LocalAgentUserInteractionEvent(
                    interactionID: "interaction-1",
                    runID: "run-1",
                    prompt: "Which visual direction should I continue?",
                    options: options,
                    imageReferences: [],
                    details: .object([
                        "title": .string("选择视觉方向"),
                        "allows_multiple": .bool(false),
                    ])
                ))
            ),
            mainChatBinding: binding
        )
        let transport = AskUserTransport()
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        let service = NativeLocalAgentAskUserPromptService(
            accountSession: AskUserAccountSession(client: client),
            state: store
        )
        return AskUserFixture(service: service, transport: transport)
    }
}

private let askUserTimestamp = "2026-09-12T06:00:00Z"

private func askUserTaskSnapshot() -> LocalAgentTaskSnapshot {
    LocalAgentTaskSnapshot(
        taskID: "task-1",
        revision: 1,
        sourceThreadID: "thread-1",
        sourceTurnID: "turn-1",
        projectID: "project-1",
        initialRunID: "task-run-1",
        currentRunID: "task-run-1",
        runIDs: ["task-run-1"],
        objective: "Refine the selected visual direction",
        acceptanceCriteria: ["Visual review accepted"],
        status: "running",
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        createdAt: askUserTimestamp,
        updatedAt: askUserTimestamp
    )
}

private func askUserTaskRunSnapshot() -> LocalAgentRunSnapshot {
    LocalAgentRunSnapshot(
        runID: "task-run-1",
        profileKey: "task_runner",
        ownerUserID: "user-1",
        ownerEntityType: "task",
        ownerEntityID: "task-1",
        projectID: "project-1",
        status: .paused,
        version: 1,
        stepSeq: 1,
        iteration: 1,
        retryCount: 0,
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        modelRuntimeSnapshot: .object([:]),
        contextStrategy: "provider_native",
        promptRevision: "prompt-1",
        capabilitySnapshotRef: "capabilities-1",
        pendingInteraction: .object(["type": .string("ask_user")]),
        createdAt: askUserTimestamp,
        updatedAt: askUserTimestamp
    )
}

private struct AskUserAccountSession: NativeLocalAgentAccountSessionAccess {
    var client: NativeLocalAgentIPCClient

    func client(accountID: String) async throws -> NativeLocalAgentIPCClient { client }
    func activeClient() async throws -> NativeLocalAgentIPCClient { client }

    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] { [] }

    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {}
}

private actor AskUserTransport: LocalAgentFrameTransport {
    private var request: Data?

    func exchange(_ request: Data) async throws -> Data {
        self.request = request
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        let requestID = try #require(object["request_id"] as? String)
        let command = try #require(object["command"] as? [String: Any])
        let commandType = try #require(command["type"] as? String)
        let response: [String: Any]
        if commandType == "get_run" {
            response = ["type": "run", "payload": Self.pendingRun()]
        } else {
            response = [
                "type": "accepted",
                "payload": ["operation_id": "operation-1"],
            ]
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": requestID,
            "response": response,
        ])
    }

    func lastRequest() -> Data? { request }

    private static func pendingRun() -> [String: Any] {
        [
            "run_id": "run-1",
            "profile_key": "main_chat",
            "owner_user_id": "user-1",
            "owner_entity_type": "conversation",
            "owner_entity_id": "thread-1",
            "project_id": NSNull(),
            "status": "paused",
            "version": 4,
            "step_seq": 1,
            "iteration": 1,
            "retry_count": 0,
            "model_config_id": "model-1",
            "model_config_revision": 1,
            "model_runtime_snapshot": [:],
            "context_strategy": "provider_native",
            "prompt_revision": "prompt-1",
            "capability_snapshot_ref": "capabilities-1",
            "pending_interaction": [
                "type": "ask_user",
                "interaction_id": "interaction-1",
                "question": [
                    "prompt": "Which visual direction should I continue?",
                    "options": [
                        ["option_id": "visual-a", "label": "方向 A"],
                        ["option_id": "visual-b", "label": "方向 B"],
                    ],
                    "image_references": [],
                    "details": [
                        "title": "选择视觉方向",
                        "allows_multiple": false,
                    ],
                ],
            ],
            "created_at": "2026-09-12T06:00:00Z",
            "updated_at": "2026-09-12T06:00:01Z",
        ]
    }
}

private func commandObject(_ request: Data) throws -> [String: Any] {
    let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
    return try #require(object["command"] as? [String: Any])
}
