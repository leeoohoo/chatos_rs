import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Host-owned project tools available to local project-team Agents. The model only receives
/// display labels and run-scoped opaque options. Real project identifiers stay inside the client,
/// where the selected option is resolved and copied into the durable proposal without a model hop.
public struct LocalAgentProjectToolProvider: AgentToolProvider, Sendable {
    public static let catalogToolName = "project_catalog"
    public static let proposeExistingTeamToolName = "team_propose_existing"
    public static let proposeNewProjectTeamToolName = "team_propose_new_project"
    public static let proposeImportedDirectoryTeamToolName = "team_propose_import_directory"

    private let store: any AgentGroupChatStore
    private let projects: [LocalProjectRecord]
    private let projectTypes: [LocalProjectTypeDefinition]
    private let projectsService: NativeLocalProjectsService?
    private let context: LocalAgentChatRunContext
    private let now: @Sendable () -> Int64

    public init(
        store: any AgentGroupChatStore,
        projects: [LocalProjectRecord],
        projectTypes: [LocalProjectTypeDefinition] = LocalAgentSkillCatalog.projectTypes,
        projectsService: NativeLocalProjectsService? = nil,
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
        self.projectTypes = projectTypes
        self.projectsService = projectsService
        self.context = context
        self.now = now
    }

    public func definitions() async throws -> [AgentToolDefinition] {
        guard try await canAccessLocalProjects() else { return [] }
        let options = try await existingProjectOptions()
        let labels = options.map { "\($0.token)=\($0.label)" }.joined(separator: "；")
        let teamProperties: [String: Any] = [
            "team_name": ["type": "string", "minLength": 1, "maxLength": 160],
            "team_goal": ["type": "string", "maxLength": 8_000],
        ]
        let projectTypeProperty: [String: Any] = [
            "type": "string",
            "enum": projectTypes.map(\.key),
            "description": projectTypes.map {
                "\($0.key)=\($0.label)"
            }.joined(separator: "；"),
        ]
        let existingSchema: [String: Any] = [
            "type": "object",
            "properties": [
                "project_option": [
                    "type": "string",
                    "enum": options.map(\.token),
                    "description": "单选项目。选项由客户端生成，本轮映射为：\(labels)",
                ],
                "team_name": teamProperties["team_name"]!,
                "team_goal": teamProperties["team_goal"]!,
            ],
            "required": ["project_option", "team_name"],
            "additionalProperties": false,
        ]
        let newProjectSchema: [String: Any] = [
            "type": "object",
            "properties": [
                "project_name": ["type": "string", "minLength": 1, "maxLength": 160],
                "project_description": ["type": "string", "maxLength": 8_000],
                "project_type": projectTypeProperty,
                "team_name": teamProperties["team_name"]!,
                "team_goal": teamProperties["team_goal"]!,
            ],
            "required": ["project_name", "project_type", "team_name"],
            "additionalProperties": false,
        ]
        let importDirectorySchema: [String: Any] = [
            "type": "object",
            "properties": [
                "absolute_path": [
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 4_096,
                    "description": "Human 明确给出的已有本机目录绝对路径，必须以 / 开头。禁止填写 http://、https://、git@ 等 Git 仓库 URL。不会创建、移动、复制或链接目录。",
                ],
                "project_name": ["type": "string", "minLength": 1, "maxLength": 160],
                "project_description": ["type": "string", "maxLength": 8_000],
                "project_type": projectTypeProperty,
                "team_name": teamProperties["team_name"]!,
                "team_goal": teamProperties["team_goal"]!,
            ],
            "required": ["absolute_path", "project_type", "team_name"],
            "additionalProperties": false,
        ]
        let emptyObjectSchema = try JSONSerialization.data(withJSONObject: [
            "type": "object",
            "properties": [:],
            "additionalProperties": false,
        ], options: [.sortedKeys])
        var definitions: [AgentToolDefinition] = [
            .init(
                name: Self.catalogToolName,
                description: "读取当前账户的全部活跃本地项目概况，返回准确总数、项目显示名称、是否已有活跃团队，以及可用于 team_propose_existing 的本轮临时选项。拥有‘查看本地项目并创建团队’权限即可调用，不要求当前 Agent 是项目经理或任何团队成员。不会返回真实项目 ID 或路径。回答项目总数、项目清单或哪些项目尚未建团队前必须调用。",
                schema: emptyObjectSchema
            ),
        ]
        if !options.isEmpty {
            definitions.append(.init(
                name: Self.proposeExistingTeamToolName,
                description: "为 project_catalog 中尚无活跃团队的已有 ChatOS 项目提交建团队提案。只传该工具 schema 中的 project_option、team_name、team_goal；不得传新项目字段、absolute_path 或 URL。拥有‘查看本地项目并创建团队’权限即可调用，不要求项目经理职业。只生成提案，必须等待 Human 确认；确认结果会作为新的未读系统消息再次唤醒你，收到后必须核对状态并回复。",
                schema: try JSONSerialization.data(
                    withJSONObject: existingSchema,
                    options: [.sortedKeys]
                ),
                effect: .write
            ))
        }
        definitions.append(.init(
            name: Self.proposeNewProjectTeamToolName,
            description: "在 Human 明确要求新建 ChatOS 项目并同时创建团队时提交提案，目录由 ChatOS 在默认工作区创建。只传该工具 schema 中的新项目与团队字段；不得传 project_option、absolute_path 或 URL。拥有‘查看本地项目并创建团队’权限即可调用，不要求项目经理职业。只生成提案，必须等待 Human 确认；确认结果会作为新的未读系统消息再次唤醒你，收到后必须核对状态并回复。",
            schema: try JSONSerialization.data(
                withJSONObject: newProjectSchema,
                options: [.sortedKeys]
            ),
            effect: .write
        ))
        if projectsService != nil {
            definitions.append(.init(
                name: Self.proposeImportedDirectoryTeamToolName,
                description: "把 Human 当前消息明确给出的、已经存在的本机绝对目录注册为 ChatOS 项目并创建团队。absolute_path 必须原样使用 Human 给出的以 / 开头的目录；不得猜测、补全或编造 /。GitHub/GitLab URL、git@ 和其他远程仓库地址都不是本机路径。目录必须位于已授权工作区且不能是软链接；导入不会创建、移动、复制或链接目录。拥有‘查看本地项目并创建团队’权限即可调用，不要求项目经理职业。只生成提案，必须等待 Human 确认；确认结果会作为新的未读系统消息再次唤醒你，收到后必须核对状态并回复。",
                schema: try JSONSerialization.data(
                    withJSONObject: importDirectorySchema,
                    options: [.sortedKeys]
                ),
                effect: .write
            ))
        }
        return definitions
    }

    public func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard try await canAccessLocalProjects() else {
            throw AgentGroupChatError.permissionDenied
        }
        if call.name == Self.catalogToolName {
            return try outcome(try await projectCatalog())
        }
        let draft: LocalAgentTeamCreationProposalDraft
        let projectLabel: String
        switch call.name {
        case Self.proposeExistingTeamToolName:
            let arguments = try decode(ExistingTeamProposalArguments.self, from: call.arguments)
            let options = try await existingProjectOptions()
            guard let selected = options.first(where: {
                $0.token == arguments.projectOption
            }), let projectID = selected.projectID else {
                throw AgentGroupChatError.invalidField("project_option")
            }
            draft = .init(
                existingProjectID: projectID,
                teamName: arguments.teamName,
                teamGoal: arguments.teamGoal ?? ""
            )
            projectLabel = selected.label
        case Self.proposeNewProjectTeamToolName:
            let arguments = try decode(NewProjectTeamProposalArguments.self, from: call.arguments)
            guard projectTypes.contains(where: { $0.key == arguments.projectType }) else {
                throw AgentGroupChatError.invalidField("project_type")
            }
            draft = .init(
                newProjectName: arguments.projectName,
                newProjectDescription: arguments.projectDescription ?? "",
                newProjectTypeKey: arguments.projectType,
                teamName: arguments.teamName,
                teamGoal: arguments.teamGoal ?? ""
            )
            projectLabel = arguments.projectName
        case Self.proposeImportedDirectoryTeamToolName:
            let arguments = try decode(ImportedDirectoryTeamProposalArguments.self, from: call.arguments)
            guard let projectsService else {
                throw AgentGroupChatError.permissionDenied
            }
            guard arguments.absolutePath.hasPrefix("/"),
                  !Self.looksLikeRemoteRepository(arguments.absolutePath) else {
                return .failure(
                    "absolute_path 只接受 Human 当前消息明确给出的、以 / 开头的已有本机目录；"
                        + "不得猜测或编造 /。GitHub/GitLab 和其他远程仓库 URL 不是本机路径。"
                        + "若 Human 要求新建空白 ChatOS 项目并创建团队，请使用 team_propose_new_project；"
                        + "若 Human 要求注册本地仓库，请让 Human 提供该仓库已经存在的本机绝对目录。"
                )
            }
            guard projectTypes.contains(where: { $0.key == arguments.projectType }) else {
                throw AgentGroupChatError.invalidField("project_type")
            }
            let prepared = try await projectsService.prepareExistingDirectoryImport(
                ownerUserID: context.ownerUserID,
                absolutePath: arguments.absolutePath,
                name: arguments.projectName,
                description: arguments.projectDescription ?? "",
                projectTypeKey: arguments.projectType
            )
            if let existing = projects.first(where: {
                $0.draft.workspaceID == prepared.draft.workspaceID
                    && $0.draft.relativeRoot == prepared.draft.relativeRoot
            }) {
                let occupied = try await occupiedProjectIDs()
                guard !occupied.contains(existing.id) else {
                    throw AgentGroupChatError.conflict
                }
                draft = .init(
                    existingProjectID: existing.id,
                    teamName: arguments.teamName,
                    teamGoal: arguments.teamGoal ?? ""
                )
                projectLabel = existing.draft.name
            } else {
                draft = .init(
                    importedProjectDraft: prepared.draft,
                    importedProjectAbsolutePath: prepared.absolutePath,
                    teamName: arguments.teamName,
                    teamGoal: arguments.teamGoal ?? ""
                )
                projectLabel = prepared.draft.name
            }
        default:
            return .failure("项目团队工具不可用：\(call.name)")
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
            status: proposal.status.rawValue,
            projectLabel: projectLabel,
            createsNewProject: draft.existingProjectID == nil
        ))
    }

    private struct ProjectOption: Sendable {
        let token: String
        let label: String
        let projectID: String?
    }

    private struct ExistingTeamProposalArguments: Decodable {
        let projectOption: String
        let teamName: String
        let teamGoal: String?
    }

    private struct NewProjectTeamProposalArguments: Decodable {
        let projectName: String
        let projectDescription: String?
        let projectType: String
        let teamName: String
        let teamGoal: String?
    }

    private struct ImportedDirectoryTeamProposalArguments: Decodable {
        let absolutePath: String
        let projectName: String?
        let projectDescription: String?
        let projectType: String
        let teamName: String
        let teamGoal: String?
    }

    private struct ProposalResponse: Encodable {
        let status: String
        let projectLabel: String
        let createsNewProject: Bool
    }

    private struct ProjectCatalogResponse: Encodable {
        let totalProjectCount: Int
        let availableForTeamCount: Int
        let projects: [ProjectCatalogEntry]
    }

    private struct ProjectCatalogEntry: Encodable {
        let name: String
        let hasActiveTeam: Bool
        let projectOption: String?
    }

    private func existingProjectOptions() async throws -> [ProjectOption] {
        let occupiedProjectIDs = try await occupiedProjectIDs()
        let available = projects.filter { !occupiedProjectIDs.contains($0.id) }
        return available.enumerated().map { offset, project in
                .init(
                    token: "existing_\(offset + 1)",
                    label: project.draft.name,
                    projectID: project.id
                )
            }
    }

    private func projectCatalog() async throws -> ProjectCatalogResponse {
        let occupied = try await occupiedProjectIDs()
        let available = projects.filter { !occupied.contains($0.id) }
        let optionByProjectID = Dictionary(uniqueKeysWithValues: available.enumerated().map {
            ($0.element.id, "existing_\($0.offset + 1)")
        })
        return .init(
            totalProjectCount: projects.count,
            availableForTeamCount: available.count,
            projects: projects.map {
                .init(
                    name: $0.draft.name,
                    hasActiveTeam: occupied.contains($0.id),
                    projectOption: optionByProjectID[$0.id]
                )
            }
        )
    }

    private func occupiedProjectIDs() async throws -> Set<String> {
        let rooms = try await store.listRooms(
            ownerUserID: context.ownerUserID,
            includeArchived: false
        )
        return Set(rooms.map(\.projectID))
    }

    private static func looksLikeRemoteRepository(_ value: String) -> Bool {
        let lowercased = value.lowercased()
        return lowercased.hasPrefix("http://")
            || lowercased.hasPrefix("https://")
            || lowercased.hasPrefix("ssh://")
            || lowercased.hasPrefix("git://")
            || lowercased.hasPrefix("git@")
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
