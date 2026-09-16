import ChatOSCore
import XCTest

final class LocalAgentSkillCatalogTests: XCTestCase {
    func testBundledRelayCatalogIsCompleteAndCarriesFullRules() throws {
        XCTAssertEqual(LocalAgentSkillCatalog.professions.count, 33)
        XCTAssertEqual(LocalAgentSkillCatalog.projectTypes.count, 27)
        let engineer = try XCTUnwrap(
            LocalAgentSkillCatalog.profession(key: "backend_engineer")
        )
        XCTAssertTrue(engineer.skillMarkdown.contains("通用职业工作基线"))
        XCTAssertTrue(engineer.skillMarkdown.count > 2_000)
        let web = try XCTUnwrap(LocalAgentSkillCatalog.projectType(key: "web_application"))
        XCTAssertTrue(web.ruleMarkdown.contains("项目治理与完成定义"))
        XCTAssertTrue(web.ruleMarkdown.contains("Web"))
        XCTAssertTrue(web.ruleMarkdown.count > 4_000)
    }

    func testDraftsRejectKeysOutsideProgramOwnedCatalog() {
        XCTAssertThrowsError(try LocalAgentProfileDraft(
            name: "伪造职业",
            rolePrompt: "test",
            modelConfigID: "model",
            professionKey: "made_up"
        ).validate())
        XCTAssertThrowsError(try LocalProjectDraft(
            name: "伪造类型",
            workspaceID: "workspace",
            projectTypeKey: "made_up"
        ).validate())
    }
}
