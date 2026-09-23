import ChatOSCore
import Foundation
import XCTest

final class LocalAgentSkillLibraryTests: XCTestCase {
    func testOverridesAreAccountScopedPersistedAndResettable() throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-skill-library-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let fileURL = folder.appendingPathComponent("overrides.json")
        let key = "backend_engineer"
        let base = try XCTUnwrap(LocalAgentSkillCatalog.profession(key: key))

        let library = LocalAgentSkillLibrary(fileURL: fileURL)
        XCTAssertEqual(library.professions(ownerUserID: "alice").count, 33)
        XCTAssertEqual(library.projectTypes(ownerUserID: "alice").count, 27)
        try library.updateProfessionBilingual(
            ownerUserID: "alice",
            key: key,
            label: "后端负责人",
            description: "负责服务端架构",
            skillMarkdown: "# 自定义职业规则\n只使用客户端授权的工具。",
            labelEN: "Backend Lead",
            descriptionEN: "Own backend architecture",
            skillMarkdownEN: "# Custom role\nUse only client-authorized tools."
        )

        XCTAssertEqual(library.profession(ownerUserID: "alice", key: key)?.label, "后端负责人")
        XCTAssertEqual(library.profession(ownerUserID: "bob", key: key)?.label, base.label)
        XCTAssertTrue(library.hasProfessionOverride(ownerUserID: "alice", key: key))

        let reopened = LocalAgentSkillLibrary(fileURL: fileURL)
        XCTAssertEqual(reopened.profession(ownerUserID: "alice", key: key)?.label, "后端负责人")
        XCTAssertEqual(reopened.profession(ownerUserID: "alice", key: key)?.labelEN, "Backend Lead")
        XCTAssertTrue(
            reopened.profession(ownerUserID: "alice", key: key)?.skillMarkdownEN
                .contains("client-authorized") == true
        )
        XCTAssertEqual(reopened.profession(ownerUserID: "alice", key: key)?.key, key)
        XCTAssertEqual(reopened.profession(ownerUserID: "alice", key: key)?.categoryKey, base.categoryKey)

        try reopened.resetProfession(ownerUserID: "alice", key: key)
        XCTAssertEqual(reopened.profession(ownerUserID: "alice", key: key), base)
        XCTAssertFalse(reopened.hasProfessionOverride(ownerUserID: "alice", key: key))
    }

    func testProjectTypeOverridePersistsAndInvalidValuesAreRejected() throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-project-skill-library-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let fileURL = folder.appendingPathComponent("overrides.json")
        let library = LocalAgentSkillLibrary(fileURL: fileURL)

        try library.updateProjectTypeBilingual(
            ownerUserID: "alice",
            key: "web_application",
            label: "Web 产品",
            description: "面向浏览器的产品",
            ruleMarkdown: "# Web 产品规则\n先验证用户路径。",
            labelEN: "Web Product",
            descriptionEN: "A browser-based product",
            ruleMarkdownEN: "# Web product rules\nValidate user journeys first."
        )
        let reopened = LocalAgentSkillLibrary(fileURL: fileURL)
        XCTAssertEqual(
            reopened.projectType(ownerUserID: "alice", key: "web_application")?.label,
            "Web 产品"
        )
        XCTAssertEqual(
            reopened.projectType(ownerUserID: "alice", key: "web_application")?.labelEN,
            "Web Product"
        )
        XCTAssertThrowsError(try reopened.updateProjectType(
            ownerUserID: "alice",
            key: "unknown",
            label: "未知",
            description: "未知",
            ruleMarkdown: "未知"
        ))
        XCTAssertThrowsError(try reopened.updateProfession(
            ownerUserID: "alice",
            key: "backend_engineer",
            label: "",
            description: "说明",
            skillMarkdown: "正文"
        ))
    }
}
