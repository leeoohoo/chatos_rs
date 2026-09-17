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
            thinkingLevel: "high",
            rationale: "补齐测试职责"
        )
        try draft.validate()
        XCTAssertTrue(draft.profileDraft.defaultPluginIDs.isEmpty)
        XCTAssertEqual(draft.profileDraft.thinkingLevel, "high")
        XCTAssertTrue(draft.memberDraft.pluginAllowlist.isEmpty)
        XCTAssertEqual(draft.memberDraft.role, "质量保障")
    }

    func testDraftRejectsEmptyRole() {
        let draft = LocalAgentDraft(
            name: "测试工程师",
            role: "",
            rolePrompt: "执行验证。",
            modelConfigID: "model-1"
        )
        XCTAssertThrowsError(try draft.validate())
    }

    func testLegacyDraftWithoutThinkingLevelStillDecodes() throws {
        let data = Data(
            #"{"name":"测试工程师","role":"质量保障","rolePrompt":"执行验证。","modelConfigID":"model-1"}"#.utf8
        )
        let draft = try JSONDecoder().decode(LocalAgentDraft.self, from: data)
        XCTAssertNil(draft.thinkingLevel)
    }
}
