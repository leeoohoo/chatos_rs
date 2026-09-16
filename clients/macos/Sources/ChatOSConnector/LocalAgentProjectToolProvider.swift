import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Host-owned project tools available to local project-team Agents. The model only receives
/// display labels and run-scoped opaque options. Real project identifiers stay inside the client,
/// where the selected option is resolved and copied into the durable proposal without a model hop.
public struct LocalAgentProjectToolProvider: AgentToolProvider, Sendable {
    public static let proposeTeamToolName = "team_propose"

    private let store: any AgentGroupChatStore
    private let projects: [LocalProjectRecord]
    private let context: LocalAgentChatRunContext
    private let now: @Sendable () -> Int64

    public init(
        store: any AgentGroupChatStore,
        projects: [LocalProjectRecord],
        context: LocalAgentChatRunContext,
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) {
        self.store = store
        self.projects = projects.sorted {
            ($0.draft.name.localizedStandardCompare($1.draft.name) == .orderedAscending)
                || ($0.draft.name == $1.draft.name && $0.id < $1.id)
        }
        self.context = context
        self.now = now
    }

    public func definitions() async throws -> [AgentToolDefinition] {
        guard try await canAccessLocalProjects() else { return [] }
        let options = try await projectOptions()
        let labels = options.map { "\($0.token)=\($0.label)" }.joined(separator: "；")
        let schema: [String: Any] = [
            "type": "object",
            "properties": [
                "project_option": [
                    "type": "string",
                    "enum": options.map(\.token),
                    "description": "单选项目。选项由客户端生成，本轮映射为：\(labels)",
                ],
                "new_project_name": ["type": "string", "minLength": 1, "maxLength": 160],
                "new_project_description": ["type": "string", "maxLength": 8_000],
                "team_name": ["type": "string", "minLength": 1, "maxLength": 160],
                "team_goal": ["type": "string", "maxLength": 8_000],
            ],
            "required": ["project_option", "team_name"],
            "additionalProperties": false,
        ]
        return [
            .init(
                name: Self.proposeTeamToolName,
                description: "向 Human 提交创建 Agent 团队的提案。project_option 是客户端提供的本轮临时单选项：可选择一个尚未创建团队的本地项目，或选择 new_project 让 ChatOS 在默认工作区新建项目。真实项目 ID 和路径不会进入模型上下文；客户端内部完成映射、持久化和最终绑定。",
                schema: try JSONSerialization.data(withJSONObject: schema, options: [.sortedKeys]),
                effect: .write
            ),
        ]
    }

    public func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard call.name == Self.proposeTeamToolName else {
            return .failure("项目团队工具不可用：\(call.name)")
        }
        guard try await canAccessLocalProjects() else {
            throw AgentGroupChatError.permissionDenied
        }
        let arguments = try decode(TeamProposalArguments.self, from: call.arguments)
        let options = try await projectOptions()
        guard let selected = options.first(where: { $0.token == arguments.projectOption }) else {
            throw AgentGroupChatError.invalidField("project_option")
        }
        let draft: LocalAgentTeamCreationProposalDraft
        if let projectID = selected.projectID {
            guard arguments.newProjectName == nil,
                  arguments.newProjectDescription == nil else {
                throw AgentGroupChatError.invalidField("new_project")
            }
            draft = .init(
                existingProjectID: projectID,
                teamName: arguments.teamName,
                teamGoal: arguments.teamGoal ?? ""
            )
        } else {
            guard let name = arguments.newProjectName else {
                throw AgentGroupChatError.invalidField("new_project_name")
            }
            draft = .init(
                newProjectName: name,
                newProjectDescription: arguments.newProjectDescription ?? "",
                teamName: arguments.teamName,
                teamGoal: arguments.teamGoal ?? ""
            )
        }
        let proposal = try await store.createTeamProposal(
                ownerUserID: context.ownerUserID,
                sourceRoomID: context.roomID,
                proposerAgentID: context.agentID,
                sourceDeliveryID: context.deliveryID,
                requestKey: call.id,
                draft: draft,
                nowUnixMs: now()
            )
        return try outcome(ProposalResponse(
            proposalID: proposal.id,
            status: proposal.status.rawValue,
            projectLabel: selected.label,
            createsNewProject: selected.projectID == nil
        ))
    }

    private struct ProjectOption: Sendable {
        let token: String
        let label: String
        let projectID: String?
    }

    private struct TeamProposalArguments: Decodable {
        let projectOption: String
        let newProjectName: String?
        let newProjectDescription: String?
        let teamName: String
        let teamGoal: String?
    }

    private struct ProposalResponse: Encodable {
        let proposalID: String
        let status: String
        let projectLabel: String
        let createsNewProject: Bool
    }

    private func projectOptions() async throws -> [ProjectOption] {
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        let occupiedProjectIDs = Set(rooms.map(\.projectID))
        let available = projects.filter { !occupiedProjectIDs.contains($0.id) }
        return [.init(token: "new_project", label: "新建项目", projectID: nil)]
            + available.enumerated().map { offset, project in
                .init(
                    token: "existing_\(offset + 1)",
                    label: project.draft.name,
                    projectID: project.id
                )
            }
    }

    private func canAccessLocalProjects() async throws -> Bool {
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        guard let current = profiles.first(where: { $0.id == context.agentID }) else {
            throw AgentGroupChatError.notFound
        }
        return LocalAgentPermission.canAccessLocalProjects(current.draft.defaultSkillIDs)
    }

    private func decode<Value: Decodable>(_ type: Value.Type, from json: String) throws -> Value {
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(Value.self, from: Data(json.utf8))
        } catch {
            throw AgentGroupChatError.invalidField("toolArguments")
        }
    }

    private func outcome<Value: Encodable>(_ value: Value) throws -> AgentToolOutcome {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return .init(String(decoding: try encoder.encode(value), as: UTF8.self))
    }
}
