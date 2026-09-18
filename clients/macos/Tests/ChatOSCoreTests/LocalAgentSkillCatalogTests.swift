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

    func testCompactCommunicationSkillIsBilingualAudienceScopedAndStable() {
        let policy = AgentCommunicationPolicy.standard
        XCTAssertEqual(policy.recommendedMessageCharacters, 800)
        XCTAssertEqual(policy.maximumMessageCharacters, 2_000)
        XCTAssertEqual(policy.maximumDocumentsPerMessage, 5)
        XCTAssertEqual(policy.maximumDocumentBytes, 2 * 1_024 * 1_024)
        XCTAssertEqual(policy.maximumDocumentBytesPerRun, 8 * 1_024 * 1_024)

        let managerZH = LocalAgentCompactCommunicationSkill.snapshot(
            language: .simplifiedChinese,
            audience: .manager
        )
        let managerEN = LocalAgentCompactCommunicationSkill.snapshot(
            language: .english,
            audience: .manager
        )
        let executorZH = LocalAgentCompactCommunicationSkill.snapshot(
            language: .simplifiedChinese,
            audience: .executor
        )

        XCTAssertEqual(managerZH.name, "chatos-compact-communication")
        XCTAssertEqual(managerZH.version, 1)
        XCTAssertEqual(managerZH.contentSHA256.count, 64)
        XCTAssertEqual(
            managerZH.contentSHA256,
            LocalAgentCompactCommunicationSkill.snapshot(
                language: .simplifiedChinese,
                audience: .manager
            ).contentSHA256
        )
        XCTAssertTrue(managerZH.markdown.contains("chat_document_create"))
        XCTAssertTrue(managerEN.markdown.contains("Lead with the conclusion"))
        XCTAssertTrue(executorZH.markdown.contains("todo_progress_append"))
        XCTAssertNotEqual(managerZH.contentSHA256, managerEN.contentSHA256)
        XCTAssertNotEqual(managerZH.contentSHA256, executorZH.contentSHA256)
        XCTAssertTrue(managerZH.promptBlock.contains(#"binding="product-owned""#))
    }
}
