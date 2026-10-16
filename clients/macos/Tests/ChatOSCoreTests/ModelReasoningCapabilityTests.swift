import ChatOSCore
import XCTest

final class ModelReasoningCapabilityTests: XCTestCase {
    func testModelOptionsUseActualModelCapabilitiesNotProviderVocabulary() {
        for provider in ["openai", "gpt", "openai_compatible", "my_api"] {
            let option = ConversationModelOption(id: "model", displayName: "Custom", modelName: "gpt-6.1-sol",
                                                  provider: provider, thinkingLevel: "minimal")
            XCTAssertTrue(option.supportsReasoning)
            XCTAssertEqual(option.thinkingLevel, "low")
            XCTAssertEqual(option.thinkingLevels, ["low", "medium", "high", "xhigh", "max"])
            XCTAssertEqual(LocalAgentThinkingLevelCatalog.values(provider: provider, model: option.modelName),
                           ["auto", "low", "medium", "high", "xhigh", "max"])
        }
        let previous = ConversationModelOption(id: "previous", displayName: "Previous", modelName: "gpt-5.6-sol",
                                              thinkingLevel: "minimal", thinkingLevels: ["none", "minimal", "low"])
        XCTAssertEqual(previous.thinkingLevel, "low")
        XCTAssertEqual(previous.thinkingLevels, ["low", "medium", "high", "xhigh", "max"])
        let dated = ConversationModelOption(id: "dated", displayName: "Snapshot", modelName: "gpt-6.1-sol-2026-10-09")
        XCTAssertEqual(dated.thinkingLevel, "medium")
    }
}
