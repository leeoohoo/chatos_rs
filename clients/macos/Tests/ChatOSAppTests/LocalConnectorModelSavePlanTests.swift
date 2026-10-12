import ChatOSCore
import XCTest
@testable import ChatOSApp

final class LocalConnectorModelSavePlanTests: XCTestCase {
    func testTurningTaskSwitchOffPreservesMemoryAndApprovalDefaults() throws {
        let model = LocalConnectorModelConfig(
            id: "chosen-model", name: "Chosen", provider: "gpt", modelName: "test",
            enabled: true, taskEnabled: true, hasAPIKey: true,
            supportsImages: false, supportsReasoning: true
        )
        let settings = LocalConnectorModelSettings(
            modelRequestMaxRetries: 5, memorySummaryModelConfigID: model.id,
            memorySummaryThinkingLevel: "medium", commandApprovalModelConfigID: model.id,
            commandApprovalThinkingLevel: "low"
        )
        var draft = LocalConnectorTaskModelDraft(model: model)
        draft.taskEnabled = false
        let plan = try LocalConnectorModelSavePlan(settings: settings, models: [model],
                                                   drafts: [model.id: draft])
        XCTAssertEqual(plan.settings, settings)
        XCTAssertEqual(plan.updates[model.id]?.taskEnabled, false)
        XCTAssertEqual(plan.updates.count, 1)
    }
}
