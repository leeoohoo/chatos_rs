import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentGroupChatScheduler {
    static func initialMessages(
        profile: LocalAgentProfile,
        delivery: ProjectAgentDelivery,
        profession: LocalAgentProfessionDefinition,
        progressiveSkillSnapshot: LocalAgentProgressiveSkillSnapshot,
        communicationSkill: LocalAgentCommunicationSkillSnapshot,
        builtinCapabilities: Set<LocalAgentTodoBuiltinCapability>
    ) throws -> [AgentMessage] {
        let staffingInstructions = LocalAgentPermission.canManageStaff(
            profile.draft.defaultSkillIDs
        ) ? LocalAgentPromptCatalog.render(.permissionStaffManagement) : ""
        let projectInstructions = LocalAgentPermission.canAccessLocalProjects(
            profile.draft.defaultSkillIDs
        ) ? LocalAgentPromptCatalog.render(.permissionLocalProjects) : ""
        let requirementSurveySkill = ""
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
                "skill_markdown": progressiveSkillSnapshot.routerMarkdown,
            ]
        )
        if delivery.lane == .manager {
            let system = LocalAgentPromptCatalog.render(
                .managerSystem,
                values: [
                    "agent_name": profile.draft.name,
                    "responsibility": profile.draft.description.isEmpty
                        ? profile.draft.rolePrompt
                        : profile.draft.description,
                    "role_prompt": profile.draft.rolePrompt,
                    "capability_discovery_skill": try BundledAgentSkillLoader.load(
                        named: "chatos-capability-discovery"
                    ).instructions,
                    "staffing_instructions": staffingInstructions,
                    "project_instructions": projectInstructions,
                    "requirement_survey_skill": requirementSurveySkill,
                    "manager_instructions": heartbeatInstructions,
                    "todo_status_instructions": todoStatusInstructions,
                    "compact_communication_skill": communicationSkill.promptBlock,
                    "profession_skill": professionSkill,
                ]
            )
            return [
                .init(role: .system, content: system),
                .init(role: .user, content: LocalAgentPromptCatalog.render(.managerWakeUser)),
            ]
        }
        let system = LocalAgentPromptCatalog.render(
            .executorSystem,
            values: [
                "agent_name": profile.draft.name,
                "role_prompt": profile.draft.rolePrompt,
                "capability_discovery_skill": try BundledAgentSkillLoader.load(
                    named: "chatos-capability-discovery"
                ).instructions,
                "requirement_survey_skill": requirementSurveySkill,
                "executor_instructions": todoInstructions,
                "compact_communication_skill": communicationSkill.promptBlock,
                "profession_skill": professionSkill,
            ]
        )
        return [
            .init(role: .system, content: system),
            .init(role: .user, content: LocalAgentPromptCatalog.render(.executorWakeUser)),
        ]
    }

    static func failureDetail(_ error: Error) -> String {
        let value = error.localizedDescription.trimmingCharacters(in: .whitespacesAndNewlines)
        return String((value.isEmpty ? "本地 Agent 运行失败。" : value).prefix(8_000))
    }
}
