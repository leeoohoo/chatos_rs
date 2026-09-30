@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentConversationRuntimeSettingsServiceTests: XCTestCase {
    func testPersistsDefaultThenAppliesModelAndReasoningSelection() async throws {
        let host = RuntimeSettingsHostStub()
        let service = NativeLocalAgentConversationRuntimeSettingsService(host: host)
        let bootstrap = bootstrapResult()
        try await service.configure(ownerUserID: "user-1", bootstrap: bootstrap)

        let initial = try await service.fetchSettings(sessionID: "conversation-1")
        XCTAssertEqual(initial.selectedModelID, "model-1")
        XCTAssertEqual(initial.selectedThinkingLevel, "medium")
        XCTAssertTrue(initial.reasoningEnabled)

        let changedModel = try await service.updateModel(
            sessionID: "conversation-1",
            modelID: "model-2"
        )
        XCTAssertEqual(changedModel.selectedModelID, "model-2")
        XCTAssertEqual(changedModel.selectedThinkingLevel, "none")
        XCTAssertFalse(changedModel.reasoningEnabled)

        let changedReasoning = try await service.updateReasoningLevel(
            sessionID: "conversation-1",
            level: "high",
            enabled: true
        )
        XCTAssertEqual(changedReasoning.selectedThinkingLevel, "high")
        XCTAssertTrue(changedReasoning.reasoningEnabled)

        let selection = try await service.resolveSelection(sessionID: "conversation-1")
        XCTAssertEqual(selection.modelSnapshot.modelConfigRef, "model-2")
        XCTAssertEqual(selection.modelSnapshot.modelConfigRevision, "revision-2")
        XCTAssertEqual(selection.settings.version, 3)
    }

    func testRejectsUnsupportedReasoningLevelBeforeIPC() async throws {
        let host = RuntimeSettingsHostStub()
        let service = NativeLocalAgentConversationRuntimeSettingsService(host: host)
        try await service.configure(ownerUserID: "user-1", bootstrap: bootstrapResult())
        _ = try await service.fetchSettings(sessionID: "conversation-1")

        do {
            _ = try await service.updateReasoningLevel(
                sessionID: "conversation-1",
                level: "auto",
                enabled: true
            )
            XCTFail("expected unsupported level")
        } catch let error as NativeLocalAgentConversationRuntimeSettingsError {
            guard case .unsupportedThinkingLevel = error else {
                return XCTFail("unexpected error: \(error)")
            }
        }
        let putCount = await host.putCount()
        XCTAssertEqual(putCount, 1)
    }

    private func bootstrapResult() -> NativeLocalAgentBootstrapResult {
        let first = LocalAgentModelConfigSnapshot(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            modelConfigRevision: "revision-1",
            credentialRef: "env:CHATOS_LOCAL_AGENT_MODEL_1",
            baseURL: "https://example.test/v1",
            model: "first",
            provider: "openai",
            supportsResponses: true,
            thinkingLevel: "medium"
        )
        let second = LocalAgentModelConfigSnapshot(
            ownerUserID: "user-1",
            modelConfigRef: "model-2",
            modelConfigRevision: "revision-2",
            credentialRef: "env:CHATOS_LOCAL_AGENT_MODEL_2",
            baseURL: "https://example.test/v1",
            model: "second",
            provider: "openai",
            supportsResponses: true
        )
        return .init(
            modelSnapshots: [first, second],
            modelOptions: [
                .init(
                    id: "model-1",
                    displayName: "First",
                    modelName: "first",
                    provider: "openai",
                    thinkingLevel: "medium",
                    supportsReasoning: true,
                    thinkingLevels: ["none", "minimal", "low", "medium", "high", "xhigh"]
                ),
                .init(
                    id: "model-2",
                    displayName: "Second",
                    modelName: "second",
                    provider: "openai",
                    supportsReasoning: true,
                    thinkingLevels: ["none", "minimal", "low", "medium", "high", "xhigh"]
                ),
            ],
            capabilitySnapshot: .init(
                ownerUserID: "user-1",
                profileKey: "main_chat",
                capabilityPolicyRevision: "policy-1"
            )
        )
    }
}

private actor RuntimeSettingsHostStub: LocalAgentHostClientServicing {
    private var settings: [String: Any]?
    private var puts = 0

    func start(ownerUserID: String) async throws {}

    func stop() async {}

    func request(command: Data) async throws -> Data {
        guard let command = try JSONSerialization.jsonObject(with: command) as? [String: Any],
              let type = command["type"] as? String else {
            throw NativeLocalAgentHostError.invalidCommand
        }
        switch type {
        case "get_conversation_runtime_settings":
            guard let settings else {
                throw NativeLocalAgentHostError.hostError(
                    code: "not_found",
                    message: "missing",
                    retryable: false
                )
            }
            return try response(settings)
        case "put_conversation_runtime_settings":
            puts += 1
            let version = ((settings?["version"] as? UInt64) ?? 0) + 1
            let stored: [String: Any] = [
                "owner_user_id": command["owner_user_id"] as Any,
                "conversation_id": command["conversation_id"] as Any,
                "selected_model_config_ref": command["selected_model_config_ref"] as Any,
                "selected_model_config_revision": command["selected_model_config_revision"] as Any,
                "selected_thinking_level": command["selected_thinking_level"] ?? NSNull(),
                "remote_connection_id": command["remote_connection_id"] ?? NSNull(),
                "reasoning_enabled": command["reasoning_enabled"] as Any,
                "version": version,
                "updated_at_unix_ms": Int64(version) * 1_000,
            ]
            settings = stored
            return try response(stored)
        default:
            throw NativeLocalAgentHostError.invalidCommand
        }
    }

    func putCount() -> Int { puts }

    private func response(_ settings: [String: Any]) throws -> Data {
        try JSONSerialization.data(withJSONObject: [
            "type": "conversation_runtime_settings",
            "settings": settings,
        ])
    }
}
