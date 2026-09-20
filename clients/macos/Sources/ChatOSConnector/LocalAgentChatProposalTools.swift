import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    func proposeMember(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let currentProfile = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        let requestedThinkingLevel = try Self.optionalString(
            arguments,
            key: "thinking_level"
        )?.trimmingCharacters(in: .whitespacesAndNewlines)
        let thinkingLevel = requestedThinkingLevel.flatMap { $0.isEmpty ? nil : $0 }
            ?? currentProfile.draft.thinkingLevel
        let draft = LocalAgentDraft(
            name: try Self.requiredString(arguments, key: "name"),
            role: try Self.requiredString(arguments, key: "role"),
            responsibility: try Self.optionalString(arguments, key: "responsibility") ?? "",
            rolePrompt: try Self.requiredString(arguments, key: "role_prompt"),
            modelConfigID: currentProfile.draft.modelConfigID,
            thinkingLevel: thinkingLevel,
            professionKey: try Self.requiredString(arguments, key: "profession_key"),
            rationale: try Self.optionalString(arguments, key: "rationale") ?? ""
        )
        try draft.validate()
        let proposal = try await store.createAgentProposal(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            proposerAgentID: context.agentID,
            sourceDeliveryID: context.deliveryID,
            requestKey: call.id,
            draft: draft,
            nowUnixMs: now()
        )
        return try Self.outcome(ProposalAcknowledgement(
            type: "agent_creation",
            status: proposal.status.rawValue,
            subject: proposal.draft.name
        ))
    }

    func proposeMemberRemoval(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let targetReference = try Self.requiredString(arguments, key: "target_agent_ref")
        guard let targetAgentID = await references.agentID(reference: targetReference) else {
            return Self.structuredFailure(
                code: "invalid_agent_ref",
                field: "target_agent_ref",
                message: "Agent 引用无效或已经过期，请重新读取当前会话成员。",
                retryable: true,
                nextTool: Self.listMembersToolName
            )
        }
        let proposal = try await store.createAgentRemovalProposal(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            proposerAgentID: context.agentID,
            sourceDeliveryID: context.deliveryID,
            requestKey: call.id,
            draft: .init(
                targetAgentID: targetAgentID,
                reason: try Self.requiredString(arguments, key: "reason"),
                handoffPlan: try Self.optionalString(arguments, key: "handoff_plan") ?? ""
            ),
            nowUnixMs: now()
        )
        return try Self.outcome(ProposalAcknowledgement(
            type: "agent_removal",
            status: proposal.status.rawValue,
            subject: "团队成员移出提案"
        ))
    }

    func proposeExistingMember(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canManageStaff() else { throw AgentGroupChatError.permissionDenied }
        let arguments = try Self.arguments(call)
        let teamReference = try Self.requiredString(arguments, key: "team_ref")
        guard let targetTeamRoomID = await references.teamID(reference: teamReference) else {
            return Self.structuredFailure(
                code: "invalid_team_ref",
                field: "team_ref",
                message: "团队引用无效或已经过期，请重新读取工作区快照。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        let agentReference = try Self.requiredString(arguments, key: "target_agent_ref")
        guard let targetAgentID = await references.agentID(reference: agentReference) else {
            return Self.structuredFailure(
                code: "invalid_agent_ref",
                field: "target_agent_ref",
                message: "Agent 引用无效或已经过期，请重新读取工作区快照。",
                retryable: true,
                nextTool: Self.workspaceSnapshotToolName
            )
        }
        do {
            let proposal = try await store.createMembershipProposal(
                ownerUserID: context.ownerUserID,
                sourceRoomID: context.roomID,
                proposerAgentID: context.agentID,
                sourceDeliveryID: context.deliveryID,
                requestKey: call.id,
                draft: .init(
                    targetTeamRoomID: targetTeamRoomID,
                    targetAgentID: targetAgentID,
                    role: try Self.requiredString(arguments, key: "role"),
                    responsibility: try Self.optionalString(
                        arguments,
                        key: "responsibility"
                    ) ?? ""
                ),
                nowUnixMs: now()
            )
            return try Self.outcome(ProposalAcknowledgement(
                type: "existing_agent_membership",
                status: proposal.status.rawValue,
                subject: "现有 Agent 入队提案"
            ))
        } catch let error as AgentGroupChatError {
            let message = switch error {
            case .conflict: "该 Agent 已经是目标团队成员，或相同提案已被处理。"
            case .notFound: "目标团队或 Agent 已不存在，请重新读取工作区快照。"
            case .permissionDenied: "当前 Agent 没有人员管理权限，或本次运行身份已失效。"
            case .invalidField(let field): "邀请参数不符合要求：\(field)。"
            case .storage: "本地成员提案暂时无法保存。"
            case .notMember: "当前 Agent 已不在发起会话中。"
            }
            return Self.structuredFailure(
                code: Self.errorCode(error),
                field: Self.errorField(error),
                message: message,
                retryable: error == .notFound,
                nextTool: error == .notFound ? Self.workspaceSnapshotToolName : nil
            )
        }
    }

    func canManageStaff() async throws -> Bool {
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let current = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        return LocalAgentPermission.canManageStaff(current.draft.defaultSkillIDs)
    }

    func managesAnyProjectTeam() async throws -> Bool {
        try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        ).contains {
            $0.conversationKind == .projectTeam
                && $0.projectManagerAgentID == context.agentID
        }
    }

    func isProjectManager(teamRoomID: String) async throws -> Bool {
        guard let room = try await store.room(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID
        ), room.status == .active, room.conversationKind == .projectTeam,
        room.projectManagerAgentID == context.agentID else { return false }
        return try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID
        ).contains { $0.agentID == context.agentID && $0.status == .active }
    }

    func isTeamMember(teamRoomID: String) async throws -> Bool {
        try await store.listMembers(
            ownerUserID: context.ownerUserID,
            roomID: teamRoomID
        ).contains { $0.agentID == context.agentID && $0.status == .active }
    }

}
