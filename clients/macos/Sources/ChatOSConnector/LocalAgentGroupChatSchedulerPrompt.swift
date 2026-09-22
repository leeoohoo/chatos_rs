import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentGroupChatScheduler {
    static func initialMessages(
        profile: LocalAgentProfile,
        member: ProjectAgentRoomMember,
        room: ProjectAgentRoom,
        delivery: ProjectAgentDelivery,
        profession: LocalAgentProfessionDefinition,
        projectType: LocalProjectTypeDefinition?,
        contextLanguage: ChatOSLanguage,
        communicationSkill: LocalAgentCommunicationSkillSnapshot,
        builtinCapabilities: Set<LocalAgentTodoBuiltinCapability>,
        triggerMessage: ProjectAgentMessage,
        triggerAttachments: [ProjectAgentMessageAttachmentPayload]
    ) -> [AgentMessage] {
        let conversationRole: String
        let conversationContext: String
        switch room.conversationKind {
        case .projectTeam:
            conversationRole = LocalAgentPromptCatalog.render(.conversationRoleProjectTeam)
            conversationContext = LocalAgentPromptCatalog.render(
                .conversationProjectTeam,
                values: [
                    "room_goal": room.draft.goal.isEmpty
                        ? LocalAgentPromptCatalog.render(.roomGoalUnset)
                        : room.draft.goal,
                ]
            )
        case .humanAgentDirect:
            conversationRole = LocalAgentPromptCatalog.render(.conversationRoleHumanAgentDirect)
            conversationContext = LocalAgentPromptCatalog.render(.conversationHumanAgentDirect)
        case .agentAgentDirect:
            conversationRole = LocalAgentPromptCatalog.render(.conversationRoleAgentAgentDirect)
            conversationContext = LocalAgentPromptCatalog.render(.conversationAgentAgentDirect)
        }
        let staffingInstructions = LocalAgentPermission.canManageStaff(
            profile.draft.defaultSkillIDs
        ) ? LocalAgentPromptCatalog.render(.permissionStaffManagement) : ""
        let projectInstructions = LocalAgentPermission.canAccessLocalProjects(
            profile.draft.defaultSkillIDs
        ) ? LocalAgentPromptCatalog.render(.permissionLocalProjects) : ""
        var requirementSurveySkills: [String] = []
        if builtinCapabilities.contains(.requirementSurveyRead) {
            requirementSurveySkills.append(
                LocalAgentPromptCatalog.render(.requirementSurveyReadSkill)
            )
        }
        if builtinCapabilities.contains(.requirementSurveyWrite) {
            requirementSurveySkills.append(
                LocalAgentPromptCatalog.render(.requirementSurveyWriteSkill)
            )
        }
        let requirementSurveySkill = requirementSurveySkills.joined(separator: "\n\n")
        let heartbeatDirective: String
        if delivery.triggerKind == .heartbeat {
            heartbeatDirective = LocalAgentPromptCatalog.render(
                .heartbeatDirective,
                values: [
                    "heartbeat_prompt": profile.draft.heartbeatPrompt.isEmpty
                        ? LocalAgentPromptCatalog.render(.heartbeatDefault)
                        : profile.draft.heartbeatPrompt,
                ]
            )
        } else {
            heartbeatDirective = ""
        }
        let heartbeatInstructions = delivery.lane == .manager
            ? LocalAgentPromptCatalog.render(
                .managerCycle,
                values: ["heartbeat_directive": heartbeatDirective]
            )
            : ""
        let todoInstructions = delivery.triggerKind == .todo
            ? LocalAgentPromptCatalog.render(.executorCycle)
            : ""
        let todoStatusInstructions = delivery.triggerKind == .todoStatus
            ? LocalAgentPromptCatalog.render(.todoStatusCycle)
            : ""
        let professionSkill = LocalAgentPromptCatalog.render(
            .professionSkill,
            values: [
                "skill_name": profession.chatOSSkillName,
                "profession_key": profession.key,
                "skill_markdown": contextLanguage == .english
                    ? profession.skillMarkdownEN
                    : profession.skillMarkdown,
            ]
        )
        let projectSkill: String
        if let projectType {
            projectSkill = LocalAgentPromptCatalog.render(
                .projectSkill,
                values: [
                    "skill_name": projectType.skillName,
                    "project_type_key": projectType.key,
                    "rule_markdown": contextLanguage == .english
                        ? projectType.ruleMarkdownEN
                        : projectType.ruleMarkdown,
                ]
            )
        } else {
            projectSkill = ""
        }
        let system = LocalAgentPromptCatalog.render(
            .groupChatSystem,
            values: [
                "conversation_role": conversationRole,
                "agent_name": profile.draft.name,
                "member_role": member.draft.role,
                "responsibility": member.draft.responsibility.isEmpty
                    ? profile.draft.description
                    : member.draft.responsibility,
                "role_prompt": profile.draft.rolePrompt,
                "conversation_context": conversationContext,
                "capability_discovery_skill": LocalAgentPromptCatalog.render(
                    .capabilityDiscoverySkill
                ),
                "staffing_instructions": staffingInstructions,
                "project_instructions": projectInstructions,
                "requirement_survey_skill": requirementSurveySkill,
                "manager_instructions": heartbeatInstructions,
                "executor_instructions": todoInstructions,
                "todo_status_instructions": todoStatusInstructions,
                "compact_communication_skill": communicationSkill.promptBlock,
                "profession_skill": professionSkill,
                "project_skill": projectSkill,
            ]
        )
        let requestedAction = switch delivery.triggerKind {
        case .heartbeat: LocalAgentPromptCatalog.render(.actionHeartbeat)
        case .todo: LocalAgentPromptCatalog.render(.actionTodo)
        case .todoStatus: LocalAgentPromptCatalog.render(.actionTodoStatus)
        default: LocalAgentPromptCatalog.render(.actionDefault)
        }
        let triggerPayload = (try? JSONEncoder().encode([
            "content": triggerMessage.content,
            "sender_kind": triggerMessage.senderKind.rawValue,
        ])).map { String(decoding: $0, as: UTF8.self) }
            ?? #"{"content":"","sender_kind":"system"}"#
        let envelope = LocalAgentPromptCatalog.render(
            .deliveryUser,
            values: [
                "trigger_kind": delivery.triggerKind.rawValue,
                "attachment_count": String(triggerAttachments.count),
                "trigger_payload": triggerPayload,
                "requested_action": requestedAction,
            ]
        )
        return [
            .init(role: .system, content: system),
            .init(
                role: .user,
                content: envelope,
                attachments: triggerAttachments.map { payload in
                    AgentMessageAttachment(
                        name: payload.attachment.name,
                        mimeType: payload.attachment.mimeType,
                        kind: AgentMessageAttachment.Kind(
                            rawValue: payload.attachment.kind.rawValue
                        ) ?? .file,
                        localFileURL: payload.localFileURL
                    )
                }
            ),
        ]
    }

    static func failureDetail(_ error: Error) -> String {
        let value = error.localizedDescription.trimmingCharacters(in: .whitespacesAndNewlines)
        return String((value.isEmpty ? "本地 Agent 运行失败。" : value).prefix(8_000))
    }
}
