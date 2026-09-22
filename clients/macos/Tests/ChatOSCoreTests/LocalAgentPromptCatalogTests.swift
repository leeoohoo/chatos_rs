import ChatOSCore
import XCTest

final class LocalAgentPromptCatalogTests: XCTestCase {
    func testEveryBundledTemplateLoadsWithItsDeclaredValues() {
        for template in LocalAgentPromptTemplate.allCases {
            let values: [String: String] = switch template {
            case .conversationProjectTeam:
                ["room_goal": "目标"]
            case .managerCycle:
                ["heartbeat_directive": "巡检"]
            case .requirementSurveySkill:
                ["available_scenarios": "read_results、review_execution"]
            case .heartbeatDirective:
                ["heartbeat_prompt": "处理未读"]
            case .professionSkill:
                [
                    "skill_name": "profession",
                    "profession_key": "project_manager",
                    "skill_markdown": "profession body",
                ]
            case .projectSkill:
                [
                    "skill_name": "project",
                    "project_type_key": "software_development",
                    "rule_markdown": "project body",
                ]
            case .groupChatSystem:
                [
                    "conversation_role": "team",
                    "agent_name": "agent",
                    "member_role": "role",
                    "responsibility": "responsibility",
                    "role_prompt": "role prompt",
                    "conversation_context": "context",
                    "capability_discovery_skill": "capability skill",
                    "staffing_instructions": "staffing",
                    "project_instructions": "project",
                    "requirement_survey_skill": "requirement survey skill",
                    "manager_instructions": "manager",
                    "executor_instructions": "executor",
                    "todo_status_instructions": "todo status",
                    "compact_communication_skill": "compact communication skill",
                    "profession_skill": "profession skill",
                    "project_skill": "project skill",
                ]
            case .deliveryUser:
                [
                    "trigger_kind": "message",
                    "attachment_count": "0",
                    "trigger_payload": "{}",
                    "requested_action": "act",
                ]
            case .builderUser:
                ["brief": "brief"]
            case .approvalUser:
                [
                    "source": "plugin",
                    "cwd": "/project",
                    "operation": "tool --flag",
                    "requested_permissions": "read",
                    "risk_level": "low",
                    "risk_reason": "none",
                ]
            default:
                [:]
            }
            let rendered = LocalAgentPromptCatalog.render(template, values: values)
            XCTAssertFalse(rendered.isEmpty, template.rawValue)
        }
    }

    func testPromptTemplatesSubstituteRuntimeValues() {
        let rendered = LocalAgentPromptCatalog.render(
            .conversationProjectTeam,
            values: ["room_goal": "交付桌面客户端"]
        )
        XCTAssertTrue(rendered.contains("交付桌面客户端"))
        XCTAssertFalse(rendered.contains("{{room_goal}}"))
    }

    func testLocalProjectPermissionIsAnExplicitNonManagerSkillWithExclusiveToolRoutes() {
        let rendered = LocalAgentPromptCatalog.render(.permissionLocalProjects)
        XCTAssertTrue(rendered.contains(#"<skill name="chatos-local-project-team-management""#))
        XCTAssertTrue(rendered.contains("即使你不是 project_manager"))
        XCTAssertTrue(rendered.contains("project_catalog"))
        XCTAssertTrue(rendered.contains("team_propose_existing"))
        XCTAssertTrue(rendered.contains("team_propose_new_project"))
        XCTAssertTrue(rendered.contains("team_propose_import_directory"))
        XCTAssertTrue(rendered.contains("猜测、补全、改写成 /"))
        XCTAssertTrue(rendered.contains("不创建、移动、复制或建立软链接"))
    }

    func testCommunicationCycleSeparatesProjectPermissionFromTodoManagerRouting() {
        let rendered = LocalAgentPromptCatalog.render(
            .managerCycle,
            values: ["heartbeat_directive": ""]
        )
        XCTAssertTrue(rendered.contains("不能覆盖另行授予的“查看本地项目并创建团队”权限"))
        XCTAssertTrue(rendered.contains("chat_direct_open"))
        XCTAssertTrue(rendered.contains("chat_direct_send"))
        XCTAssertTrue(rendered.contains("Human-Agent 私聊"))
    }

    func testRequirementSurveySkillIsProgressiveAndToolDirected() {
        let entry = LocalAgentPromptCatalog.render(
            .requirementSurveySkill,
            values: [
                "available_scenarios":
                    "create_survey、read_results、resolve_survey、review_execution",
            ]
        )
        let create = LocalAgentPromptCatalog.render(.requirementSurveyCreateSkill)
        let read = LocalAgentPromptCatalog.render(.requirementSurveyReadResultsSkill)
        let resolve = LocalAgentPromptCatalog.render(.requirementSurveyResolveSkill)
        let review = LocalAgentPromptCatalog.render(.requirementSurveyReviewExecutionSkill)

        XCTAssertTrue(entry.contains("需求调研用于"))
        XCTAssertTrue(entry.contains("什么时候使用"))
        XCTAssertTrue(entry.contains("requirement_survey_skill_get"))
        XCTAssertTrue(entry.contains("create_survey"))
        XCTAssertFalse(entry.contains("requirement_survey_create"))
        XCTAssertTrue(create.contains("requirement_survey_list"))
        XCTAssertTrue(create.contains("requirement_survey_create"))
        XCTAssertTrue(create.contains("request_key"))
        XCTAssertTrue(read.contains("requirement_survey_get"))
        XCTAssertTrue(read.contains("Human 已确认"))
        XCTAssertTrue(resolve.contains("requirement_survey_resolve"))
        XCTAssertTrue(resolve.contains("execution_steps"))
        XCTAssertTrue(review.contains("requirement_survey_project_tasks"))
        XCTAssertTrue(review.contains("未覆盖"))
        XCTAssertTrue([create, read, resolve, review].allSatisfy { $0.contains("```json") })
        XCTAssertFalse((entry + create + read + resolve + review).contains("team_ref"))
    }
}
