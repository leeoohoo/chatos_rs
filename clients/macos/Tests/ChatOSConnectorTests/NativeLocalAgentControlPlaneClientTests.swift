@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentControlPlaneClientTests: XCTestCase {
    func testOnlyMissingRemoteCapabilityUsesPersistedLocalControlPlane() {
        XCTAssertTrue(NativeLocalConnectorService.shouldUsePersistedCapability(
            after: NativeConnectorError.server(status: 404, message: "missing")
        ))
        XCTAssertFalse(NativeLocalConnectorService.shouldUsePersistedCapability(
            after: NativeConnectorError.server(status: 500, message: "failed")
        ))
        XCTAssertFalse(NativeLocalConnectorService.shouldUsePersistedCapability(
            after: NativeConnectorError.notPaired
        ))
    }

    func testLatestSnapshotCommandsAreOwnerScopedAndDecodeResults() async throws {
        let model = LocalAgentModelConfigSnapshot(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            modelConfigRevision: "revision-2",
            credentialRef: "env:CHATOS_LOCAL_AGENT_MODEL_MODEL_1",
            baseURL: "https://example.invalid/v1",
            model: "gpt-test",
            provider: "openai",
            supportsResponses: true,
            thinkingLevel: "medium"
        )
        let capability = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: "user-1",
            profileKey: "main_chat",
            capabilityPolicyRevision: "policy-2",
            instructions: "Use the task tools."
        )
        let host = ControlPlaneHostStub(model: model, capability: capability)
        let client = NativeLocalAgentControlPlaneClient(host: host)

        let models = try await client.latestModels(ownerUserID: "user-1")
        let restoredCapability = try await client.latestCapabilities(
            ownerUserID: "user-1",
            profileKey: "main_chat"
        )

        XCTAssertEqual(models, [model])
        XCTAssertEqual(restoredCapability, capability)
        let commands = try await host.commandObjects()
        XCTAssertEqual(
            commands.compactMap { decodedString($0["type"]) },
            ["list_latest_model_config_snapshots", "get_latest_capability_policy_snapshot"]
        )
        XCTAssertTrue(commands.allSatisfy {
            decodedString($0["owner_user_id"]) == "user-1"
        })
        XCTAssertEqual(decodedString(commands[1]["profile_key"]), "main_chat")
    }

    private func decodedString(_ value: LocalAgentJSONValue?) -> String? {
        guard case let .string(decoded)? = value else { return nil }
        return decoded
    }
}

private actor ControlPlaneHostStub: LocalAgentHostClientServicing {
    private let model: LocalAgentModelConfigSnapshot
    private let capability: LocalAgentCapabilityPolicySnapshot
    private var commands: [Data] = []

    init(
        model: LocalAgentModelConfigSnapshot,
        capability: LocalAgentCapabilityPolicySnapshot
    ) {
        self.model = model
        self.capability = capability
    }

    func start(ownerUserID: String) async throws {}

    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: command) as? [String: Any]
        )
        switch object["type"] as? String {
        case "list_latest_model_config_snapshots":
            return try response(type: "model_config_snapshots", valueKey: "snapshots", value: [model])
        case "get_latest_capability_policy_snapshot":
            return try response(
                type: "capability_policy_snapshot",
                valueKey: "snapshot",
                value: capability
            )
        default:
            throw NativeLocalAgentHostError.invalidCommand
        }
    }

    func commandObjects() throws -> [[String: LocalAgentJSONValue]] {
        try commands.map { data in
            try JSONDecoder().decode([String: LocalAgentJSONValue].self, from: data)
        }
    }

    private func response<T: Encodable>(
        type: String,
        valueKey: String,
        value: T
    ) throws -> Data {
        let encoded = try JSONEncoder().encode(value)
        let valueObject = try JSONSerialization.jsonObject(with: encoded)
        return try JSONSerialization.data(withJSONObject: [
            "type": type,
            valueKey: valueObject,
        ])
    }
}
