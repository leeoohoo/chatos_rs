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

    func testRepairsPersistedUnsupportedLevelsWithoutLosingRemoteBinding() async throws {
        for (selected, enabled, expected) in [("none", false, "low"), ("minimal", true, "low"), ("max", true, "max")] {
            let host = RuntimeSettingsHostStub()
            await host.seedLegacySettings(level: selected, enabled: enabled)
            let service = NativeLocalAgentConversationRuntimeSettingsService(host: host)
            try await service.configure(ownerUserID: "user-1", bootstrap: bootstrapResult(firstModel: "gpt-6.1-sol"))
            let settings = try await service.fetchSettings(sessionID: "conversation-1")
            XCTAssertEqual(settings.selectedThinkingLevel, expected)
            XCTAssertTrue(settings.reasoningEnabled)
            XCTAssertEqual(settings.remoteConnectionID, "remote-1")
            let selection = try await service.resolveSelection(sessionID: "conversation-1")
            XCTAssertEqual(selection.settings.selectedThinkingLevel, expected)
            let count = await host.putCount()
            XCTAssertEqual(count, selected == "max" ? 0 : 1)
        }
    }

    func testMandatoryReasoningModelDefaultsAndDisableRequestStayValid() async throws {
        let host = RuntimeSettingsHostStub()
        let service = NativeLocalAgentConversationRuntimeSettingsService(host: host)
        try await service.configure(ownerUserID: "user-1", bootstrap: bootstrapResult(firstModel: "gpt-6.1-sol"))
        let initial = try await service.fetchSettings(sessionID: "conversation-1")
        XCTAssertEqual(initial.selectedThinkingLevel, "medium")
        let disabled = try await service.updateReasoning(sessionID: "conversation-1", enabled: false)
        XCTAssertEqual(disabled.selectedThinkingLevel, "low")
        XCTAssertTrue(disabled.reasoningEnabled)
        let max = try await service.updateReasoningLevel(sessionID: "conversation-1", level: "max", enabled: true)
        XCTAssertEqual(max.selectedThinkingLevel, "max")
        let options = try await service.fetchAvailableModels()
        XCTAssertEqual(options[0].thinkingLevels, ["low", "medium", "high", "xhigh", "max"])
        XCTAssertEqual(NativeLocalConnectorService.thinkingLevels(provider: "gpt", model: "gpt-6.1-sol"),
                       options[0].thinkingLevels)
    }

    private func bootstrapResult(firstModel: String = "first") -> NativeLocalAgentBootstrapResult {
        let first = LocalAgentModelConfigSnapshot(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            modelConfigRevision: "revision-1",
            credentialRef: "env:CHATOS_LOCAL_AGENT_MODEL_1",
            baseURL: "https://example.test/v1",
            model: firstModel,
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
        let capability = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: "user-1",
            profileKey: "main_chat",
            capabilityPolicyRevision: "policy-1"
        )
        return .init(
            modelSnapshots: [first, second],
            modelOptions: [
                .init(
                    id: "model-1",
                    displayName: "First",
                    modelName: firstModel,
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
            capabilitySnapshot: capability,
            capabilitySnapshotsByModelConfigRef: [
                "model-1": capability,
                "model-2": capability,
            ],
            pluginInstallerCapabilitySnapshotsByModelConfigRef: [
                "model-1": capability,
                "model-2": capability,
            ]
        )
    }
}

private actor RuntimeSettingsHostStub: LocalAgentHostClientServicing {
    private var settings: [String: Any]?
    private var puts = 0

    func seedLegacySettings(level: String, enabled: Bool) {
        settings = ["owner_user_id": "user-1", "conversation_id": "conversation-1",
                    "selected_model_config_ref": "model-1", "selected_model_config_revision": "revision-1",
                    "selected_thinking_level": level, "reasoning_enabled": enabled,
                    "remote_connection_id": "remote-1", "version": UInt64(8), "updated_at_unix_ms": Int64(8_000)]
    }

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
