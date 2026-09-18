import ChatOSCore
import XCTest

final class LocalAgentSkillCatalogTests: XCTestCase {
    func testBundledChatOSCatalogIsCompleteBilingualAndUsesNativeCapabilities() throws {
        XCTAssertEqual(LocalAgentSkillCatalog.professions.count, 33)
        XCTAssertEqual(LocalAgentSkillCatalog.projectTypes.count, 27)
        let engineer = try XCTUnwrap(
            LocalAgentSkillCatalog.profession(key: "backend_engineer")
        )
        XCTAssertTrue(engineer.skillMarkdown.contains("工具与插件"))
        XCTAssertTrue(engineer.skillMarkdown.contains("project_read"))
        XCTAssertTrue(engineer.skillMarkdownEN.contains("Tools and plugins"))
        XCTAssertTrue(engineer.skillMarkdownEN.contains("project_write"))
        let web = try XCTUnwrap(LocalAgentSkillCatalog.projectType(key: "web_application"))
        XCTAssertTrue(web.ruleMarkdown.contains("Todo 能力与插件"))
        XCTAssertTrue(web.ruleMarkdown.contains("浏览器"))
        XCTAssertTrue(web.ruleMarkdownEN.contains("Todo capabilities and plugins"))
        XCTAssertFalse(engineer.skillMarkdown.contains("Relay"))
        XCTAssertFalse(engineer.skillMarkdown.contains("company."))
        XCTAssertFalse(web.ruleMarkdown.contains("company."))

        let taskCreators = LocalAgentSkillCatalog.professions
            .filter(\.canCreateTasks)
            .map(\.key)
        XCTAssertEqual(taskCreators, ["project_manager"])
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
