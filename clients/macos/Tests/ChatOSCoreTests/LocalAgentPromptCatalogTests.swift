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
            case .managerSystem:
                [
                    "agent_name": "agent",
                    "responsibility": "responsibility",
                    "role_prompt": "role prompt",
                    "capability_discovery_skill": "capability skill",
                    "staffing_instructions": "staffing",
                    "project_instructions": "project",
                    "manager_instructions": "manager",
                    "todo_status_instructions": "todo status",
                    "compact_communication_skill": "compact communication skill",
                    "profession_skill": "profession skill",
                ]
            case .executorSystem:
                [
                    "agent_name": "agent",
                    "role_prompt": "role prompt",
                    "capability_discovery_skill": "capability skill",
                    "executor_instructions": "executor",
                    "compact_communication_skill": "compact communication skill",
                    "profession_skill": "profession skill",
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
        XCTAssertTrue(rendered.contains("不属于任何群聊、私聊或项目"))
        XCTAssertTrue(rendered.contains("chat_read_all_unread"))
        XCTAssertTrue(rendered.contains("统一调用 chat_send_message"))
        XCTAssertTrue(rendered.contains("禁止索要或编排 conversation_ref"))
        XCTAssertTrue(rendered.contains("只有团队明确绑定且职业为 project_manager"))
        XCTAssertTrue(rendered.contains("普通 blocked Todo 不是 Human 待办"))

        let todoStatus = LocalAgentPromptCatalog.render(.todoStatusCycle)
        XCTAssertTrue(todoStatus.contains("Agent 通讯层唤醒"))
        XCTAssertTrue(todoStatus.contains("不是 Todo 执行层继续运行"))
        XCTAssertTrue(todoStatus.contains("chat_read_all_unread"))
        XCTAssertTrue(todoStatus.contains("禁止管理 conversation_ref"))
    }

    func testManagerAndExecutorPromptsUseDifferentRuntimeIdentities() {
        let manager = LocalAgentPromptCatalog.render(
            .managerSystem,
            values: [
                "agent_name": "丹青",
                "responsibility": "负责设计",
                "role_prompt": "保持一致",
                "capability_discovery_skill": "capability",
                "staffing_instructions": "",
                "project_instructions": "",
                "manager_instructions": "manager",
                "todo_status_instructions": "",
                "compact_communication_skill": "communication",
                "profession_skill": "profession",
            ]
        )
        XCTAssertTrue(manager.contains("通讯 Run 属于你这个 Agent"))
        XCTAssertTrue(manager.contains("恢复你自己的长期 Memory"))
        XCTAssertTrue(manager.contains("chat_read_all_unread"))
        XCTAssertFalse(manager.contains("当前群目标"))

        let executor = LocalAgentPromptCatalog.render(
            .executorSystem,
            values: [
                "agent_name": "丹青",
                "role_prompt": "完成任务",
                "capability_discovery_skill": "capability",
                "executor_instructions": "executor",
                "compact_communication_skill": "communication",
                "profession_skill": "profession",
            ]
        )
        XCTAssertTrue(executor.contains("本轮只绑定一个 Todo"))
        XCTAssertTrue(executor.contains("这个 Todo 自己的独立 Memory"))
        XCTAssertTrue(executor.contains("不负责给群聊或私聊发消息"))
        XCTAssertFalse(executor.contains("chat_read_all_unread"))
    }

}
