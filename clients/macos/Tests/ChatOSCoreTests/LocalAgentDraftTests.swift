import ChatOSCore
import XCTest

final class LocalAgentDraftTests: XCTestCase {
    func testDraftMapsToProfileAndMemberWithoutCreationAuthority() throws {
        let draft = LocalAgentDraft(
            name: "测试工程师",
            role: "质量保障",
            responsibility: "验证项目群聊",
            rolePrompt: "先读取群聊，再执行验证。",
            modelConfigID: "model-1",
            pluginIDs: ["plugin.test"],
            rationale: "补齐测试职责"
        )
        try draft.validate()
        XCTAssertEqual(draft.profileDraft.defaultPluginIDs, ["plugin.test"])
        XCTAssertEqual(draft.memberDraft.pluginAllowlist, ["plugin.test"])
        XCTAssertEqual(draft.memberDraft.role, "质量保障")
    }

    func testDraftRejectsDuplicatePluginIDs() {
        let draft = LocalAgentDraft(
            name: "测试工程师",
            role: "质量保障",
            rolePrompt: "执行验证。",
            modelConfigID: "model-1",
            pluginIDs: ["plugin.test", "plugin.test"]
        )
        XCTAssertThrowsError(try draft.validate())
    }
}
