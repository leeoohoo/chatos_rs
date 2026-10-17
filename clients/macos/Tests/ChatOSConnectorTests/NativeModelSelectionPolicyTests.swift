import ChatOSAgentRuntime
import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeModelSelectionPolicyTests {
    @Test
    func gatewayTaskSwitchDoesNotDisableOtherModelConsumers() throws {
        let model = try JSONDecoder().decode(GatewayModelConfigDTO.self, from: Data(#"{"id":"general","name":"General","provider":"gpt","model":"test-model","enabled":true,"task_enabled":false,"has_api_key":true,"api_key":"test-key","base_url":"https://model-selection.invalid/v1"}"#.utf8))
        #expect(model.isSelectable(for: .general))
        #expect(!model.isSelectable(for: .taskCreation))
        let snapshot = approvalSnapshot(baseURL: model.baseURL ?? "")
        let configuration = try NativeApprovalAgentModelConfiguration(
            snapshot: snapshot,
            apiKey: model.apiKey ?? ""
        )
        // Construct the actual approval client without making any network request.
        _ = try NativeApprovalAgent.makeModelClient(
            model: configuration,
            policy: .init(),
            thinkingLevel: "low"
        )
        #expect(throws: (any Error).self) {
            try NativeApprovalAgentModelConfiguration(
                snapshot: approvalSnapshot(baseURL: "file:///tmp/model"),
                apiKey: "test-key"
            )
        }
        #expect(throws: (any Error).self) {
            try NativeApprovalAgentModelConfiguration(snapshot: snapshot, apiKey: "")
        }
    }

    @Test
    func agentConfigurationIncludesModelsNotOfferedForTaskCreation() {
        let models = [model("general-only", taskEnabled: false), model("task", taskEnabled: true),
                      model("disabled", enabled: false), model("no-key", hasAPIKey: false)]
        let options = LocalAgentBuilderService.modelOptions(from: models)
        #expect(Set(options.map(\.id)) == ["general-only", "task"])
        #expect(options.first(where: { $0.id == "general-only" })?.supportsReasoning == true)
        #expect(options.first(where: { $0.id == "general-only" })?.thinkingLevels.contains("low") == true)
    }

    @Test
    func legacyGatewayFlagsKeepTheirExistingDefaults() throws {
        let model = try JSONDecoder().decode(GatewayModelConfigDTO.self, from: Data(#"{"id":"legacy","name":"Legacy","provider":"gpt","model":"test-model"}"#.utf8))
        #expect(model.isSelectable(for: .general))
        #expect(model.isSelectable(for: .taskCreation))
    }

    private func model(
        _ id: String, enabled: Bool = true, taskEnabled: Bool = true, hasAPIKey: Bool = true
    ) -> LocalConnectorModelConfig {
        .init(id: id, name: id, provider: "gpt", modelName: id,
              enabled: enabled, taskEnabled: taskEnabled, hasAPIKey: hasAPIKey,
              supportsImages: false, supportsReasoning: true)
    }

    private func approvalSnapshot(baseURL: String) -> LocalAgentModelConfigSnapshot {
        .init(
            ownerUserID: "owner-1",
            modelConfigRef: "general",
            modelConfigRevision: "revision-1",
            credentialRef: "env:GENERAL",
            baseURL: baseURL,
            model: "test-model",
            provider: "gpt",
            supportsResponses: true
        )
    }
}
