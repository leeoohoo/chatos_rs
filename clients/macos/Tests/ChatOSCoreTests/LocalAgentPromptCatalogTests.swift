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
}
