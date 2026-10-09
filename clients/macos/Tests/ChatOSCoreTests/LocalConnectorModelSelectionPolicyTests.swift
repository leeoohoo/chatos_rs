import ChatOSCore
import Testing

struct LocalConnectorModelSelectionPolicyTests {
    @Test
    func taskSwitchOnlyControlsTaskCreationCandidates() {
        for taskEnabled in [false, true] {
            #expect(LocalConnectorModelSelectionPolicy.permits(
                enabled: true, hasAPIKey: true, taskEnabled: taskEnabled, scope: .general
            ))
            #expect(LocalConnectorModelSelectionPolicy.permits(
                enabled: true, hasAPIKey: true, taskEnabled: taskEnabled, scope: .taskCreation
            ) == taskEnabled)
        }
    }

    @Test
    func disabledOrCredentiallessModelsRemainUnavailableInBothScopes() {
        for scope in [LocalConnectorModelSelectionScope.general, .taskCreation] {
            for taskEnabled in [false, true] {
                #expect(!LocalConnectorModelSelectionPolicy.permits(
                    enabled: false, hasAPIKey: true, taskEnabled: taskEnabled, scope: scope
                ))
                #expect(!LocalConnectorModelSelectionPolicy.permits(
                    enabled: true, hasAPIKey: false, taskEnabled: taskEnabled, scope: scope
                ))
            }
        }
    }

    @Test
    func scopesProduceIndependentCandidateLists() {
        let models = [model("general-only", taskEnabled: false), model("task", taskEnabled: true)]
        #expect(LocalConnectorModelSelectionPolicy.models(from: models, scope: .general)
            .map(\.id) == ["general-only", "task"])
        #expect(LocalConnectorModelSelectionPolicy.models(from: models, scope: .taskCreation)
            .map(\.id) == ["task"])
    }

    private func model(_ id: String, taskEnabled: Bool) -> LocalConnectorModelConfig {
        .init(id: id, name: id, provider: "gpt", modelName: id,
              enabled: true, taskEnabled: taskEnabled, hasAPIKey: true,
              supportsImages: false, supportsReasoning: true)
    }
}
