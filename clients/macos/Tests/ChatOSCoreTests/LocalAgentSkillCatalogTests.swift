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

        let projectManager = try XCTUnwrap(
            LocalAgentSkillCatalog.profession(key: "project_manager")
        )
        for requiredInstruction in [
            "project_dashboard_get",
            "project_dashboard_update",
            "expected_revision",
            "todo_ref",
            "不得在看板中复制或伪造系统统计",
            "只有当前团队明确绑定的项目经理",
        ] {
            XCTAssertTrue(
                projectManager.skillMarkdown.contains(requiredInstruction),
                requiredInstruction
            )
        }
        XCTAssertTrue(projectManager.skillMarkdownEN.contains("project_dashboard_get"))
        XCTAssertTrue(projectManager.skillMarkdownEN.contains("system-owned"))
        XCTAssertTrue(projectManager.skillMarkdownEN.contains("explicitly bound Project Manager"))
    }

    func testEveryProfessionAndProjectTypeHasDetailedProgressiveDisclosureResources() throws {
        let expectedPaths = [
            "references/collaboration-and-escalation.md",
            "references/deliverables-and-evidence.md",
            "references/quality-gates-and-risks.md",
            "references/worked-examples-and-counterexamples.md",
            "references/workflow.md",
        ]
        for language in ChatOSLanguage.allCases {
            for profession in LocalAgentSkillCatalog.professions {
                let skill = LocalAgentProgressiveSkillCatalog.boundProfessionSkill(
                    profession,
                    language: language
                )
                XCTAssertEqual(skill.kind, .profession, profession.key)
                XCTAssertGreaterThan(skill.instructions.count, 500, profession.key)
                XCTAssertEqual(skill.resources.map(\.relativePath).sorted(), expectedPaths)
                XCTAssertEqual(Set(skill.resources.map(\.contentSHA256)).count, 5)
                for resource in skill.resources {
                    XCTAssertGreaterThan(resource.markdown.count, 450, "\(profession.key): \(resource.relativePath)")
                    XCTAssertTrue(resource.markdown.contains(skill.label))
                    XCTAssertEqual(resource.contentSHA256.count, 64)
                }
                let examples = try XCTUnwrap(skill.resources.first {
                    $0.relativePath == "references/worked-examples-and-counterexamples.md"
                })
                XCTAssertTrue(examples.markdown.contains(
                    language == .english ? "## Good example" : "## 正确示例"
                ), profession.key)
                XCTAssertTrue(examples.markdown.contains(
                    language == .english ? "## Critical counterexample" : "## 关键反例"
                ), profession.key)
                XCTAssertFalse(examples.markdown.contains("missing worked example"), profession.key)
                XCTAssertFalse(examples.markdown.contains("缺少专属示例"), profession.key)
            }
            for projectType in LocalAgentSkillCatalog.projectTypes {
                let skill = LocalAgentProgressiveSkillCatalog.boundProjectTypeSkill(
                    projectType,
                    language: language
                )
                XCTAssertEqual(skill.kind, .projectType, projectType.key)
                XCTAssertGreaterThan(skill.instructions.count, 500, projectType.key)
                XCTAssertEqual(skill.resources.map(\.relativePath).sorted(), expectedPaths)
                for resource in skill.resources {
                    XCTAssertGreaterThan(resource.markdown.count, 450, "\(projectType.key): \(resource.relativePath)")
                    XCTAssertTrue(resource.markdown.contains(skill.label))
                }
                let examples = try XCTUnwrap(skill.resources.first {
                    $0.relativePath == "references/worked-examples-and-counterexamples.md"
                })
                XCTAssertTrue(examples.markdown.contains(
                    language == .english ? "## Good example" : "## 正确示例"
                ), projectType.key)
                XCTAssertTrue(examples.markdown.contains(
                    language == .english ? "## Critical counterexample" : "## 关键反例"
                ), projectType.key)
                XCTAssertFalse(examples.markdown.contains("missing worked example"), projectType.key)
                XCTAssertFalse(examples.markdown.contains("缺少专属示例"), projectType.key)
            }
        }
    }

    func testProgressiveSnapshotRouterIsCompactBoundAndHashStable() throws {
        let profession = try XCTUnwrap(
            LocalAgentSkillCatalog.profession(key: "backend_engineer")
        )
        let project = try XCTUnwrap(
            LocalAgentSkillCatalog.projectType(key: "web_application")
        )
        let first = LocalAgentProgressiveSkillCatalog.boundSnapshot(
            profession: profession,
            projectType: project,
            language: .simplifiedChinese
        )
        let second = LocalAgentProgressiveSkillCatalog.boundSnapshot(
            profession: profession,
            projectType: project,
            language: .simplifiedChinese
        )
        XCTAssertEqual(first, second)
        XCTAssertEqual(first.skills.count, 2)
        XCTAssertTrue(first.routerMarkdown.contains("agent_skill_activate"))
        XCTAssertTrue(first.routerMarkdown.contains(first.skills[0].skillRef))
        XCTAssertTrue(first.routerMarkdown.contains(first.skills[1].skillRef))
        XCTAssertFalse(first.routerMarkdown.contains("工具与插件"))
        XCTAssertLessThan(first.routerMarkdown.count, 1_500)

        let direct = LocalAgentProgressiveSkillCatalog.boundSnapshot(
            profession: profession,
            projectType: nil,
            language: .english
        )
        XCTAssertEqual(direct.skills.map(\.kind), [.profession])
        XCTAssertTrue(direct.routerMarkdown.contains("unlisted Skill"))
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
        XCTAssertEqual(policy.maximumDocumentsPerRun, 20)
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
        XCTAssertEqual(managerZH.version, 2)
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
