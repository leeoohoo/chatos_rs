import Foundation

/// Product-owned prompt templates used by the local Agent runtime.
///
/// Prompt prose lives in bundled Markdown resources so it can be reviewed, versioned and
/// replaced without mixing policy text into the scheduler implementation. Code supplies only
/// program-owned facts through the declared placeholders.
public enum LocalAgentPromptTemplate: String, CaseIterable, Sendable {
    case conversationRoleProjectTeam = "AgentPrompt.ConversationRole.ProjectTeam"
    case conversationRoleHumanAgentDirect = "AgentPrompt.ConversationRole.HumanAgentDirect"
    case conversationRoleAgentAgentDirect = "AgentPrompt.ConversationRole.AgentAgentDirect"
    case conversationProjectTeam = "AgentPrompt.Conversation.ProjectTeam"
    case conversationHumanAgentDirect = "AgentPrompt.Conversation.HumanAgentDirect"
    case conversationAgentAgentDirect = "AgentPrompt.Conversation.AgentAgentDirect"
    case roomGoalUnset = "AgentPrompt.Conversation.RoomGoalUnset"
    case permissionStaffManagement = "AgentPrompt.Permission.StaffManagement"
    case permissionLocalProjects = "AgentPrompt.Permission.LocalProjects"
    case managerCycle = "AgentPrompt.Cycle.Manager"
    case heartbeatDefault = "AgentPrompt.Cycle.HeartbeatDefault"
    case heartbeatDirective = "AgentPrompt.Cycle.HeartbeatDirective"
    case executorCycle = "AgentPrompt.Cycle.Executor"
    case todoStatusCycle = "AgentPrompt.Cycle.TodoStatus"
    case professionSkill = "AgentPrompt.Skill.Profession"
    case projectSkill = "AgentPrompt.Skill.Project"
    case capabilityDiscoverySkill = "AgentPrompt.Skill.CapabilityDiscovery"
    case groupChatSystem = "AgentPrompt.GroupChat.System"
    case actionHeartbeat = "AgentPrompt.Action.Heartbeat"
    case actionTodo = "AgentPrompt.Action.Todo"
    case actionTodoStatus = "AgentPrompt.Action.TodoStatus"
    case actionDefault = "AgentPrompt.Action.Default"
    case deliveryUser = "AgentPrompt.GroupChat.DeliveryUser"
    case builderSystem = "AgentPrompt.Builder.System"
    case builderUser = "AgentPrompt.Builder.User"
    case agentDefaultRole = "AgentPrompt.Agent.DefaultRole"
    case approvalSystem = "AgentPrompt.Approval.System"
    case approvalUser = "AgentPrompt.Approval.User"

    fileprivate var requiredPlaceholders: Set<String> {
        switch self {
        case .conversationProjectTeam:
            ["room_goal"]
        case .managerCycle:
            ["heartbeat_directive"]
        case .heartbeatDirective:
            ["heartbeat_prompt"]
        case .professionSkill:
            ["skill_name", "profession_key", "skill_markdown"]
        case .projectSkill:
            ["skill_name", "project_type_key", "rule_markdown"]
        case .groupChatSystem:
            [
                "conversation_role", "agent_name", "member_role", "responsibility",
                "role_prompt", "conversation_context", "capability_discovery_skill",
                "staffing_instructions", "project_instructions", "manager_instructions",
                "executor_instructions", "todo_status_instructions", "profession_skill",
                "project_skill",
            ]
        case .deliveryUser:
            ["trigger_kind", "attachment_count", "trigger_payload", "requested_action"]
        case .builderUser:
            ["brief"]
        case .approvalUser:
            [
                "source", "cwd", "operation", "requested_permissions", "risk_level",
                "risk_reason",
            ]
        case .conversationRoleProjectTeam, .conversationRoleHumanAgentDirect,
             .conversationRoleAgentAgentDirect, .conversationHumanAgentDirect,
             .conversationAgentAgentDirect, .roomGoalUnset,
             .permissionStaffManagement, .permissionLocalProjects, .heartbeatDefault,
             .executorCycle, .todoStatusCycle, .capabilityDiscoverySkill,
             .actionHeartbeat, .actionTodo,
             .actionTodoStatus, .actionDefault, .builderSystem, .agentDefaultRole,
             .approvalSystem:
            []
        }
    }
}

public enum LocalAgentPromptCatalog {
    private static let templates: [LocalAgentPromptTemplate: String] = {
        var result: [LocalAgentPromptTemplate: String] = [:]
        for template in LocalAgentPromptTemplate.allCases {
            guard let url = Bundle.module.url(
                forResource: template.rawValue,
                withExtension: "md"
            ), let text = try? String(contentsOf: url, encoding: .utf8) else {
                fatalError("Bundled Agent prompt template is missing: \(template.rawValue)")
            }
            let normalized = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard placeholders(in: normalized) == template.requiredPlaceholders else {
                fatalError("Bundled Agent prompt placeholders are invalid: \(template.rawValue)")
            }
            result[template] = normalized
        }
        return result
    }()

    public static func render(
        _ template: LocalAgentPromptTemplate,
        values: [String: String] = [:]
    ) -> String {
        guard values.keys.allSatisfy(template.requiredPlaceholders.contains),
              Set(values.keys) == template.requiredPlaceholders,
              var output = templates[template] else {
            fatalError("Agent prompt values do not match template: \(template.rawValue)")
        }
        for (key, value) in values {
            output = output.replacingOccurrences(of: "{{\(key)}}", with: value)
        }
        return output
    }

    private static func placeholders(in template: String) -> Set<String> {
        let pattern = #"\{\{([a-z0-9_]+)\}\}"#
        guard let expression = try? NSRegularExpression(pattern: pattern) else { return [] }
        let range = NSRange(template.startIndex..., in: template)
        return Set(expression.matches(in: template, range: range).compactMap { match in
            guard let swiftRange = Range(match.range(at: 1), in: template) else { return nil }
            return String(template[swiftRange])
        })
    }
}
